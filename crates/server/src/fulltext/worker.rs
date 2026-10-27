//! The background full-text indexer.
//!
//! One `std::thread` ("fyr-fulltext") indexes one archive at a time:
//!
//! * **Picking work:** admin build requests first (FIFO), then a scan of
//!   `books/` resumes any unfinished index and starts new ones for archives
//!   with at most `auto_max_articles` articles that the admin has not
//!   excluded (deleting an index excludes the archive until a build is
//!   requested again). Indexes of archives that changed or disappeared are
//!   deleted. With nothing to do it sleeps until woken or for
//!   `rescan_interval`.
//! * **Indexing** (two resumable phases, see [`run_job`]): a cheap scan of
//!   the directory queues every HTML article as `(cluster, blob, entry)`;
//!   the queue is then indexed in cluster order (one decompression per
//!   cluster), `chunk_entries` articles per transaction, each transaction
//!   also removing its queue rows. After each transaction the thread sleeps
//!   `throttle` × its work time, so it uses at most a fraction of one core.
//! * **Stopping:** each chunk (and every few articles inside it) checks for
//!   shutdown, an admin delete, preemption by an admin build, an archive
//!   that changed or vanished, and free disk space under the guard. An
//!   interrupted transaction rolls back, so the stored progress always marks
//!   a clean resume point.

use super::store::{self, ArchiveStamp, IndexState, IndexWriter};
use super::{index_path, remove_index_files};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use zim::{MimeType, Namespace, Target, Zim};

const EXCLUDED_FILE: &str = "excluded.json";
/// Cancellation is checked every this many articles inside a chunk.
const CANCEL_CHECK_EVERY: usize = 16;
/// Longest single throttle pause.
const MAX_THROTTLE_PAUSE: Duration = Duration::from_secs(5);

/// Runtime settings (from [`types::Config`], overridable in tests).
#[derive(Debug, Clone)]
pub struct IndexerSettings {
    pub index_dir: PathBuf,
    pub books_dir: PathBuf,
    pub enabled: bool,
    /// Archives with at most this many articles are indexed automatically
    /// (0 = only on admin request).
    pub auto_max_articles: u64,
    /// Pause indexing while free space on the index volume is below this.
    pub min_free_bytes: u64,
    /// Sleep `throttle` × work time after each chunk.
    pub throttle: f64,
    /// Articles stored per transaction.
    pub chunk_entries: u32,
    /// Idle rescan period (and back-off after a disk-space pause).
    pub rescan_interval: Duration,
}

impl IndexerSettings {
    pub fn from_config(config: &types::Config) -> Self {
        Self {
            index_dir: config.index_dir(),
            books_dir: config.books_dir(),
            enabled: config.fulltext.enabled,
            auto_max_articles: config.fulltext.max_articles,
            min_free_bytes: config.fulltext.min_free_mb.saturating_mul(1024 * 1024),
            throttle: 0.5,
            chunk_entries: 200,
            rescan_interval: Duration::from_secs(60),
        }
    }
}

/// Why a running job was asked to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// Server shutdown: the index stays resumable.
    Shutdown,
    /// Admin deleted the index (or the archive).
    Removed,
    /// An admin build request preempted an automatic job.
    Yield,
}

impl CancelReason {
    pub(crate) fn code(self) -> u8 {
        match self {
            CancelReason::Shutdown => 1,
            CancelReason::Removed => 2,
            CancelReason::Yield => 3,
        }
    }

    pub(crate) fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(CancelReason::Shutdown),
            2 => Some(CancelReason::Removed),
            3 => Some(CancelReason::Yield),
            _ => None,
        }
    }
}

/// Shared between the worker and the API for one running job.
#[derive(Debug, Default)]
pub struct JobControl {
    pub(super) cancel: AtomicU8,
    pub cursor: AtomicU32,
    pub total: AtomicU32,
    pub articles: AtomicU64,
    /// Articles queued but not yet stored.
    pub pending: AtomicU64,
    /// Admin-requested job (ignores the automatic size cap, not preempted).
    pub forced: bool,
    /// Test hook: stop (as [`CancelReason::Yield`]) after this many
    /// indexing batches.
    pub max_chunks: Option<u32>,
}

impl JobControl {
    pub fn new(forced: bool) -> Self {
        Self {
            forced,
            ..Default::default()
        }
    }

    pub fn cancel(&self, reason: CancelReason) {
        // First reason wins (a shutdown is not downgraded to a yield).
        let _ = self
            .cancel
            .compare_exchange(0, reason.code(), Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn cancelled(&self) -> Option<CancelReason> {
        CancelReason::from_code(self.cancel.load(Ordering::SeqCst))
    }
}

/// How a job ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    Completed { articles: u64 },
    Cancelled(CancelReason),
    /// Stopped because free space is under the guard; resumable.
    LowDisk,
    /// The archive changed or vanished; its index was deleted.
    Stale,
    Failed(String),
}

struct ActiveJob {
    filename: String,
    control: Arc<JobControl>,
}

#[derive(Default)]
struct WorkerState {
    shutdown: bool,
    wake: bool,
    started: bool,
    queue: VecDeque<String>,
    current: Option<ActiveJob>,
    excluded: HashSet<String>,
    low_disk: bool,
    /// Articles per archive version, so scans do not reopen every archive.
    article_counts: HashMap<String, (ArchiveStamp, u64)>,
}

struct Shared {
    state: Mutex<WorkerState>,
    /// Signalled to wake the worker.
    wake: Condvar,
    /// Signalled whenever a job ends.
    idle: Condvar,
}

fn lock(shared: &Shared) -> MutexGuard<'_, WorkerState> {
    shared.state.lock().unwrap_or_else(|p| p.into_inner())
}

/// Errors for the admin API.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    Disabled,
    InvalidName,
    NotFound,
    /// The index files could not be removed (e.g. still open on Windows).
    Busy,
}

/// Result of a build request.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildOutcome {
    Queued,
    Running,
    AlreadyReady,
}

/// Per-archive status for `GET /api/index/status`. Filenames and counters
/// only: no paths.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArchiveIndexStatus {
    pub filename: String,
    /// `not_indexed`, `queued`, `indexing`, `paused`, `ready`, `failed`,
    /// `excluded` or `disabled`.
    pub state: &'static str,
    /// Articles in the archive, when known.
    pub articles: Option<u64>,
    /// Articles stored in the index so far.
    pub indexed_articles: u64,
    pub processed_entries: u32,
    pub total_entries: u32,
    pub percent: f64,
    /// Whether the archive is indexed without an admin request.
    pub auto: bool,
    pub index_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusResponse {
    pub enabled: bool,
    pub auto_max_articles: u64,
    pub min_free_mb: u64,
    pub low_disk: bool,
    pub archives: Vec<ArchiveIndexStatus>,
}

/// Owner of the background indexer (one per server).
pub struct FulltextIndexer {
    settings: IndexerSettings,
    shared: Arc<Shared>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl FulltextIndexer {
    pub fn new(settings: IndexerSettings) -> Self {
        let excluded = load_excluded(&settings.index_dir);
        Self {
            settings,
            shared: Arc::new(Shared {
                state: Mutex::new(WorkerState {
                    excluded,
                    ..Default::default()
                }),
                wake: Condvar::new(),
                idle: Condvar::new(),
            }),
            thread: Mutex::new(None),
        }
    }

    pub fn from_config(config: &types::Config) -> Self {
        Self::new(IndexerSettings::from_config(config))
    }

    pub fn settings(&self) -> &IndexerSettings {
        &self.settings
    }

    /// Directory readers should look in, or `None` when disabled.
    pub fn reader_dir(&self) -> Option<PathBuf> {
        self.settings.enabled.then(|| self.settings.index_dir.clone())
    }

    /// Spawn the worker thread (no-op when disabled or already started).
    pub fn start(&self) {
        if !self.settings.enabled {
            return;
        }
        {
            let mut state = lock(&self.shared);
            if state.started || state.shutdown {
                return;
            }
            state.started = true;
        }
        let shared = Arc::clone(&self.shared);
        let settings = self.settings.clone();
        match std::thread::Builder::new()
            .name("fyr-fulltext".to_string())
            .spawn(move || worker_loop(shared, settings))
        {
            Ok(handle) => {
                *self.thread.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
            }
            Err(error) => {
                warn!("Full-text indexer could not start: {}", error);
                lock(&self.shared).started = false;
            }
        }
    }

    /// Wake the worker (e.g. after new content arrived).
    pub fn notify(&self) {
        lock(&self.shared).wake = true;
        self.shared.wake.notify_all();
    }

    /// Stop the worker: cancel the running job (it stays resumable) and wait
    /// up to `wait` for the thread to exit.
    pub fn shutdown(&self, wait: Duration) {
        {
            let mut state = lock(&self.shared);
            state.shutdown = true;
            if let Some(job) = &state.current {
                job.control.cancel(CancelReason::Shutdown);
            }
        }
        self.shared.wake.notify_all();
        let handle = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(handle) = handle {
            let end = Instant::now() + wait;
            while !handle.is_finished() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                warn!("Full-text indexer did not stop within {:?}", wait);
            }
        }
    }

    fn archive_path(&self, filename: &str) -> Result<PathBuf, IndexError> {
        if !super::is_valid_archive_name(filename) {
            return Err(IndexError::InvalidName);
        }
        let path = self.settings.books_dir.join(filename);
        if !path.is_file() {
            return Err(IndexError::NotFound);
        }
        Ok(path)
    }

    /// Admin request to (re)start indexing `filename`, regardless of the
    /// automatic size cap. A preemptible automatic job yields to it.
    pub fn request_build(&self, filename: &str) -> Result<BuildOutcome, IndexError> {
        if !self.settings.enabled {
            return Err(IndexError::Disabled);
        }
        let archive_path = self.archive_path(filename)?;
        let index = index_path(&self.settings.index_dir, filename);

        let mut state = lock(&self.shared);
        if state.excluded.remove(filename) {
            save_excluded(&self.settings.index_dir, &state.excluded);
        }
        if state.current.as_ref().is_some_and(|j| j.filename == filename) {
            return Ok(BuildOutcome::Running);
        }
        if let (Some(meta), Some(stamp)) = (store::inspect(&index), ArchiveStamp::of(&archive_path)) {
            if meta.matches(filename, &stamp) && meta.state == IndexState::Ready {
                return Ok(BuildOutcome::AlreadyReady);
            }
            if !meta.matches(filename, &stamp) || meta.state == IndexState::Failed {
                // Retry from scratch.
                remove_index_files(&index);
            }
        }
        if !state.queue.iter().any(|f| f == filename) {
            state.queue.push_back(filename.to_string());
        }
        if let Some(job) = &state.current {
            if !job.control.forced {
                job.control.cancel(CancelReason::Yield);
            }
        }
        state.wake = true;
        drop(state);
        self.shared.wake.notify_all();
        Ok(BuildOutcome::Queued)
    }

    /// Admin request to stop indexing `filename` and delete its index. The
    /// archive is excluded from automatic indexing until a build is
    /// requested again.
    pub fn remove(&self, filename: &str, wait: Duration) -> Result<(), IndexError> {
        if !super::is_valid_archive_name(filename) {
            return Err(IndexError::InvalidName);
        }
        {
            let mut state = lock(&self.shared);
            state.excluded.insert(filename.to_string());
            save_excluded(&self.settings.index_dir, &state.excluded);
        }
        self.drop_index(filename, wait)
    }

    /// Called before an archive is deleted from `books/`: stop indexing it,
    /// delete its index and forget any exclusion (a re-download is indexed
    /// automatically again).
    pub fn forget_archive(&self, filename: &str, wait: Duration) {
        if !super::is_valid_archive_name(filename) {
            return;
        }
        {
            let mut state = lock(&self.shared);
            if state.excluded.remove(filename) {
                save_excluded(&self.settings.index_dir, &state.excluded);
            }
            state.article_counts.remove(filename);
        }
        let _ = self.drop_index(filename, wait);
    }

    fn drop_index(&self, filename: &str, wait: Duration) -> Result<(), IndexError> {
        let mut state = lock(&self.shared);
        state.queue.retain(|f| f != filename);
        if let Some(job) = state.current.as_ref().filter(|j| j.filename == filename) {
            job.control.cancel(CancelReason::Removed);
        }
        let end = Instant::now() + wait;
        while state.current.as_ref().is_some_and(|j| j.filename == filename) {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(IndexError::Busy);
            }
            state = self
                .shared
                .idle
                .wait_timeout(state, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        drop(state);
        if remove_index_files(&index_path(&self.settings.index_dir, filename)) {
            Ok(())
        } else {
            Err(IndexError::Busy)
        }
    }

    /// Status of every ZIM archive in `books/`. Blocking (reads index
    /// metadata and archive headers); call on the blocking pool.
    pub fn status(&self) -> IndexStatusResponse {
        let settings = &self.settings;
        let archives = list_archives(&settings.books_dir);
        let (queue, current, excluded, low_disk) = {
            let state = lock(&self.shared);
            (
                state.queue.clone(),
                state
                    .current
                    .as_ref()
                    .map(|j| (j.filename.clone(), Arc::clone(&j.control))),
                state.excluded.clone(),
                state.low_disk,
            )
        };

        let entries = archives
            .into_iter()
            .map(|(filename, path)| {
                let stamp = ArchiveStamp::of(&path);
                let articles = stamp.and_then(|s| self.article_count(&filename, &path, s));
                let auto = settings.auto_max_articles > 0
                    && articles.is_some_and(|n| n <= settings.auto_max_articles);
                let index = index_path(&settings.index_dir, &filename);
                let meta = store::inspect(&index).filter(|m| stamp.is_some_and(|s| m.matches(&filename, &s)));
                let mut status = ArchiveIndexStatus {
                    filename: filename.clone(),
                    state: "not_indexed",
                    articles,
                    indexed_articles: 0,
                    processed_entries: 0,
                    total_entries: 0,
                    percent: 0.0,
                    auto,
                    index_bytes: super::index_size(&index),
                    note: None,
                };
                let mut pending = 0u64;
                if let Some(meta) = &meta {
                    status.indexed_articles = meta.articles;
                    status.processed_entries = meta.cursor;
                    status.total_entries = meta.total;
                    pending = meta.pending;
                    status.note = meta.note.clone();
                    status.state = match meta.state {
                        IndexState::Ready => "ready",
                        IndexState::Failed => "failed",
                        IndexState::Building => "paused",
                    };
                }
                match &current {
                    Some((name, control)) if *name == filename => {
                        status.state = "indexing";
                        status.processed_entries = control.cursor.load(Ordering::Relaxed);
                        status.total_entries = control.total.load(Ordering::Relaxed);
                        status.indexed_articles = control.articles.load(Ordering::Relaxed);
                        pending = control.pending.load(Ordering::Relaxed);
                    }
                    _ if queue.iter().any(|q| *q == filename) => status.state = "queued",
                    _ if excluded.contains(&filename) && meta.is_none() => status.state = "excluded",
                    _ if !settings.enabled => status.state = "disabled",
                    _ => {}
                }
                if status.state == "paused" && low_disk {
                    status.note = Some("Paused: free disk space is below the indexing threshold".to_string());
                }
                status.percent = if status.state == "ready" {
                    100.0
                } else {
                    progress_percent(status.processed_entries, status.total_entries, status.indexed_articles, pending)
                };
                status
            })
            .collect();

        IndexStatusResponse {
            enabled: settings.enabled,
            auto_max_articles: settings.auto_max_articles,
            min_free_mb: settings.min_free_bytes / (1024 * 1024),
            low_disk,
            archives: entries,
        }
    }

    fn article_count(&self, filename: &str, path: &Path, stamp: ArchiveStamp) -> Option<u64> {
        article_count_cached(&self.shared, filename, path, stamp)
    }
}

/// Overall progress: the directory scan counts for the first 10 %, storing
/// the queued articles for the remaining 90 %.
pub fn progress_percent(scanned: u32, total: u32, stored: u64, pending: u64) -> f64 {
    let pct = if total == 0 {
        0.0
    } else if scanned < total {
        10.0 * f64::from(scanned) / f64::from(total)
    } else if stored + pending == 0 {
        100.0
    } else {
        10.0 + 90.0 * stored as f64 / (stored + pending) as f64
    };
    (pct * 10.0).round() / 10.0
}

fn article_count_cached(shared: &Shared, filename: &str, path: &Path, stamp: ArchiveStamp) -> Option<u64> {
    if let Some((cached_stamp, count)) = lock(shared).article_counts.get(filename) {
        if *cached_stamp == stamp {
            return Some(*count);
        }
    }
    let count = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let zim = Zim::new(path).ok()?;
        let listed = zim.article_list_by_title().ok().flatten().and_then(|l| l.len().ok());
        Some(listed.map(|n| n as u64).unwrap_or(u64::from(zim.header.article_count)))
    }))
    .ok()
    .flatten()?;
    lock(shared)
        .article_counts
        .insert(filename.to_string(), (stamp, count));
    Some(count)
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

fn worker_loop(shared: Arc<Shared>, settings: IndexerSettings) {
    info!("Full-text indexer started");
    let mut backoff_until: Option<Instant> = None;
    loop {
        if lock(&shared).shutdown {
            break;
        }
        let in_backoff = backoff_until.is_some_and(|until| Instant::now() < until)
            && lock(&shared).queue.is_empty();
        let job = if in_backoff { None } else { pick_job(&shared, &settings) };

        let Some((filename, forced)) = job else {
            let mut state = lock(&shared);
            let wait = backoff_until
                .map(|until| until.saturating_duration_since(Instant::now()))
                .filter(|d| !d.is_zero())
                .unwrap_or(settings.rescan_interval);
            let end = Instant::now() + wait;
            while !state.shutdown && !state.wake {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                state = shared.wake.wait_timeout(state, left).unwrap_or_else(|p| p.into_inner()).0;
            }
            state.wake = false;
            if backoff_until.is_some_and(|until| Instant::now() >= until) {
                backoff_until = None;
            }
            continue;
        };

        let control = Arc::new(JobControl::new(forced));
        {
            let mut state = lock(&shared);
            if state.shutdown {
                break;
            }
            state.current = Some(ActiveJob {
                filename: filename.clone(),
                control: Arc::clone(&control),
            });
        }
        info!("Full-text indexing {}", filename);
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| run_job(&settings, &filename, &control)))
            .unwrap_or_else(|_| {
                mark_failed(&settings, &filename, "The indexer crashed on this archive");
                JobOutcome::Failed("indexer crashed".to_string())
            });
        match &outcome {
            JobOutcome::Completed { articles } => info!("Full-text index ready for {} ({} articles)", filename, articles),
            JobOutcome::Failed(reason) => warn!("Full-text indexing failed for {}: {}", filename, reason),
            JobOutcome::LowDisk => warn!("Full-text indexing paused: free disk space below threshold"),
            other => info!("Full-text indexing of {} stopped: {:?}", filename, other),
        }
        {
            let mut state = lock(&shared);
            state.current = None;
            state.low_disk = outcome == JobOutcome::LowDisk;
        }
        shared.idle.notify_all();
        match outcome {
            JobOutcome::Cancelled(CancelReason::Shutdown) => break,
            // Back off so an archive that keeps failing (or a full disk)
            // cannot make the worker spin.
            JobOutcome::LowDisk | JobOutcome::Failed(_) => {
                backoff_until = Some(Instant::now() + settings.rescan_interval)
            }
            _ => {}
        }
    }
    shared.idle.notify_all();
    info!("Full-text indexer stopped");
}

fn low_disk(settings: &IndexerSettings) -> bool {
    low_disk_at(&settings.index_dir, settings.min_free_bytes)
}

/// Whether free space on the volume of `index_dir` is under `min_free_bytes`
/// (shared with the place indexer). `0` disables the guard.
pub(crate) fn low_disk_at(index_dir: &Path, min_free_bytes: u64) -> bool {
    if min_free_bytes == 0 {
        return false;
    }
    let probe = if index_dir.exists() {
        index_dir
    } else {
        index_dir.parent().unwrap_or(index_dir)
    };
    // Unknown free space counts as low: do not fill a disk we cannot measure.
    fs2::available_space(probe).map_or(true, |free| free < min_free_bytes)
}

/// `(filename, path)` of every `.zim` file directly in `books_dir`, sorted.
pub fn list_archives(books_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(books_dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let path = entry.path();
            (super::is_valid_archive_name(&name) && path.is_file()).then_some((name, path))
        })
        .collect();
    out.sort();
    out
}

/// Next job: admin requests first, then unfinished or new automatic
/// builds. Also deletes indexes of changed or removed archives.
fn pick_job(shared: &Shared, settings: &IndexerSettings) -> Option<(String, bool)> {
    if low_disk(settings) {
        lock(shared).low_disk = true;
        return None;
    }
    lock(shared).low_disk = false;

    loop {
        let next = lock(shared).queue.pop_front();
        let Some(filename) = next else { break };
        if settings.books_dir.join(&filename).is_file() {
            return Some((filename, true));
        }
    }

    let archives = list_archives(&settings.books_dir);
    remove_orphans(settings, &archives);
    let excluded = lock(shared).excluded.clone();

    for (filename, path) in archives {
        if excluded.contains(&filename) {
            continue;
        }
        let Some(stamp) = ArchiveStamp::of(&path) else { continue };
        let index = index_path(&settings.index_dir, &filename);
        if index.exists() {
            match store::inspect(&index) {
                Some(meta) if meta.matches(&filename, &stamp) => match meta.state {
                    IndexState::Building => return Some((filename, false)),
                    IndexState::Ready | IndexState::Failed => continue,
                },
                // Stale (archive replaced), old schema or unreadable.
                _ => {
                    remove_index_files(&index);
                }
            }
        }
        if settings.auto_max_articles == 0 {
            continue;
        }
        if article_count_cached(shared, &filename, &path, stamp)
            .is_some_and(|count| count <= settings.auto_max_articles)
        {
            return Some((filename, false));
        }
    }
    None
}

/// Delete index files whose archive no longer exists.
fn remove_orphans(settings: &IndexerSettings, archives: &[(String, PathBuf)]) {
    let expected: HashSet<String> = archives
        .iter()
        .map(|(name, _)| super::index_file_name(name))
        .collect();
    let Ok(entries) = std::fs::read_dir(&settings.index_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let Some(base) = name.strip_suffix(".fts.sqlite") else { continue };
        if !expected.contains(&format!("{base}.fts.sqlite")) {
            remove_index_files(&entry.path());
        }
    }
}

fn mark_failed(settings: &IndexerSettings, filename: &str, note: &str) {
    let path = index_path(&settings.index_dir, filename);
    if let Ok((writer, _)) = IndexWriter::open(&path) {
        let _ = writer.mark_failed(note);
    }
}

fn open_zim(path: &Path) -> Option<Zim> {
    std::panic::catch_unwind(AssertUnwindSafe(|| Zim::new(path).ok()))
        .ok()
        .flatten()
}

fn is_html(mime: &MimeType) -> bool {
    matches!(mime, MimeType::Type(m) if m.starts_with("text/html") || m.starts_with("application/xhtml+xml"))
}

/// Directory entries scanned per transaction in the scan phase.
const SCAN_CHUNK_ENTRIES: u32 = 20_000;

/// Index (or resume indexing) one archive. Blocking.
///
/// Two resumable phases, each committed in transactions together with its
/// progress:
/// 1. **Scan** the directory in URL order (cheap, no content reads) and
///    queue every HTML article as `(cluster, blob, entry)` in the index's
///    `todo` table; `cursor` tracks the next URL index.
/// 2. **Index** the queue in cluster order, `chunk_entries` articles per
///    transaction, deleting queue rows as their articles are stored. Each
///    cluster is decompressed about once no matter how an archive orders
///    its directory, which matters for zstd/xz-compressed archives.
pub fn run_job(settings: &IndexerSettings, filename: &str, control: &Arc<JobControl>) -> JobOutcome {
    let archive_path = settings.books_dir.join(filename);
    let Some(stamp) = ArchiveStamp::of(&archive_path) else {
        return JobOutcome::Stale;
    };
    if std::fs::create_dir_all(&settings.index_dir).is_err() {
        return JobOutcome::Failed("index directory is not writable".to_string());
    }
    if low_disk(settings) {
        return JobOutcome::LowDisk;
    }
    let index = index_path(&settings.index_dir, filename);

    let existing = store::inspect(&index).filter(|m| m.matches(filename, &stamp));
    let zim = open_zim(&archive_path);
    let opened = match (&existing, &zim) {
        (Some(meta), _) if meta.state == IndexState::Ready => {
            return JobOutcome::Completed { articles: meta.articles };
        }
        (Some(meta), Some(zim)) if meta.state == IndexState::Building && meta.total == zim.header.article_count => {
            IndexWriter::open(&index)
        }
        (_, zim) => {
            remove_index_files(&index);
            let total = zim.as_ref().map_or(0, |z| z.header.article_count);
            IndexWriter::create(&index, filename, stamp, total)
        }
    };
    let (mut writer, meta) = match opened {
        Ok(opened) => opened,
        Err(error) if store::is_disk_full(&error) => return JobOutcome::LowDisk,
        Err(_) => return JobOutcome::Failed("index database could not be opened".to_string()),
    };
    let Some(zim) = zim else {
        let _ = writer.mark_failed("The archive could not be read");
        return JobOutcome::Failed("archive could not be read".to_string());
    };

    let total = zim.header.article_count;
    let mut cursor = meta.cursor.min(total);
    let mut articles = meta.articles;
    let mut pending = meta.pending;
    control.total.store(total, Ordering::Relaxed);
    control.cursor.store(cursor, Ordering::Relaxed);
    control.articles.store(articles, Ordering::Relaxed);
    control.pending.store(pending, Ordering::Relaxed);

    let check = |writer: IndexWriter| -> Result<IndexWriter, JobOutcome> {
        if let Some(reason) = control.cancelled() {
            return Err(JobOutcome::Cancelled(reason));
        }
        if ArchiveStamp::of(&archive_path) != Some(stamp) {
            drop(writer);
            remove_index_files(&index);
            return Err(JobOutcome::Stale);
        }
        if low_disk(settings) {
            return Err(JobOutcome::LowDisk);
        }
        Ok(writer)
    };
    let db_failure = |writer: &IndexWriter, error: rusqlite::Error| -> JobOutcome {
        if store::is_disk_full(&error) {
            return JobOutcome::LowDisk;
        }
        let _ = writer.mark_failed("Writing the index failed");
        JobOutcome::Failed("index write failed".to_string())
    };

    // Phase 1: queue articles.
    while cursor < total {
        writer = match check(writer) {
            Ok(writer) => writer,
            Err(outcome) => return outcome,
        };
        let end = cursor.saturating_add(SCAN_CHUNK_ENTRIES).min(total);
        match scan_chunk(&zim, &mut writer, cursor, end, articles, pending, control) {
            Ok(queued) => {
                pending += queued;
                cursor = end;
            }
            Err(ChunkError::Cancelled(reason)) => return JobOutcome::Cancelled(reason),
            Err(ChunkError::Db(error)) => return db_failure(&writer, error),
        }
        control.cursor.store(cursor, Ordering::Relaxed);
        control.pending.store(pending, Ordering::Relaxed);
    }

    // Phase 2: index queued articles in cluster order.
    let mut chunks = 0u32;
    loop {
        writer = match check(writer) {
            Ok(writer) => writer,
            Err(outcome) => return outcome,
        };
        let started = Instant::now();
        let batch = match writer.next_pending(settings.chunk_entries.max(1) as usize) {
            Ok(batch) => batch,
            Err(error) => return db_failure(&writer, error),
        };
        if batch.is_empty() {
            break;
        }
        match index_batch(&zim, &mut writer, &batch, cursor, articles, pending, control) {
            Ok(added) => {
                articles += added;
                pending = pending.saturating_sub(batch.len() as u64);
            }
            Err(ChunkError::Cancelled(reason)) => return JobOutcome::Cancelled(reason),
            Err(ChunkError::Db(error)) => return db_failure(&writer, error),
        }
        control.articles.store(articles, Ordering::Relaxed);
        control.pending.store(pending, Ordering::Relaxed);
        chunks += 1;
        if control.max_chunks.is_some_and(|max| chunks >= max) {
            return JobOutcome::Cancelled(CancelReason::Yield);
        }
        throttle(settings, started.elapsed(), control);
    }

    if let Some(reason) = control.cancelled() {
        return JobOutcome::Cancelled(reason);
    }
    // Merge segments; interruptible by cancellation (stays resumable).
    let probe = Arc::clone(control);
    match writer.finish(move || probe.cancelled().is_some()) {
        Ok(()) => JobOutcome::Completed { articles },
        Err(error) if store::is_disk_full(&error) => JobOutcome::LowDisk,
        Err(_) => match control.cancelled() {
            Some(reason) => JobOutcome::Cancelled(reason),
            None => JobOutcome::Failed("index optimisation failed".to_string()),
        },
    }
}

fn throttle(settings: &IndexerSettings, worked: Duration, control: &JobControl) {
    throttle_pause(settings.throttle, worked, || control.cancelled().is_some());
}

/// Sleep `factor` × `worked` (at most 5 s), waking early when `cancelled`
/// (shared with the place indexer).
pub(crate) fn throttle_pause(factor: f64, worked: Duration, cancelled: impl Fn() -> bool) {
    if factor <= 0.0 || !factor.is_finite() {
        return;
    }
    let pause = worked.mul_f64(factor).min(MAX_THROTTLE_PAUSE);
    let end = Instant::now() + pause;
    while !cancelled() {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

enum ChunkError {
    Cancelled(CancelReason),
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for ChunkError {
    fn from(error: rusqlite::Error) -> Self {
        ChunkError::Db(error)
    }
}

fn is_indexable(entry: &zim::DirectoryEntry) -> bool {
    matches!(entry.namespace, Namespace::Articles | Namespace::UserContent)
        && is_html(&entry.mime_type)
        && !entry.url.is_empty()
        && !entry.url.starts_with("_assets_/")
        && matches!(entry.target, Some(Target::Cluster(_, _)))
}

/// Queue the HTML articles among URL-pointer entries `start..end`.
fn scan_chunk(
    zim: &Zim,
    writer: &mut IndexWriter,
    start: u32,
    end: u32,
    articles: u64,
    pending_before: u64,
    control: &JobControl,
) -> Result<u64, ChunkError> {
    let batch = writer.batch()?;
    let mut queued = 0u64;
    for idx in start..end {
        if idx % 1024 == 0 {
            if let Some(reason) = control.cancelled() {
                return Err(ChunkError::Cancelled(reason));
            }
        }
        let Ok(entry) = zim.get_by_url_index(idx) else { continue };
        if !is_indexable(&entry) {
            continue;
        }
        if let Some(Target::Cluster(cluster, blob)) = entry.target {
            batch.queue(cluster, blob, idx)?;
            queued += 1;
        }
    }
    batch.commit(end, articles, pending_before + queued)?;
    Ok(queued)
}

/// Store the queued articles of `items` (in cluster order) in one
/// transaction and remove them from the queue. Returns articles stored.
fn index_batch(
    zim: &Zim,
    writer: &mut IndexWriter,
    items: &[(u32, u32, u32)],
    cursor: u32,
    articles_before: u64,
    pending_before: u64,
    control: &JobControl,
) -> Result<u64, ChunkError> {
    let batch = writer.batch()?;
    let mut added = 0u64;
    let mut cached: Option<(u32, zim::Cluster<'_>)> = None;
    for (n, &(cluster_idx, blob, entry_idx)) in items.iter().enumerate() {
        if n % CANCEL_CHECK_EVERY == 0 {
            if let Some(reason) = control.cancelled() {
                return Err(ChunkError::Cancelled(reason));
            }
        }
        let Ok(entry) = zim.get_by_url_index(entry_idx) else { continue };
        if !is_indexable(&entry) {
            continue;
        }
        if cached.as_ref().map(|(idx, _)| *idx) != Some(cluster_idx) {
            cached = zim.get_cluster(cluster_idx).ok().map(|c| (cluster_idx, c));
        }
        let Some((_, cluster)) = &cached else { continue };
        let body = match cluster.read() {
            Ok(guard) => match guard.blob(blob) {
                Ok(bytes) if bytes.len() > super::MAX_ARTICLE_HTML_BYTES => String::new(),
                Ok(bytes) => super::article_text(&String::from_utf8_lossy(bytes)),
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let path = crate::library::normalize_zim_url(&entry.url);
        if path.is_empty() {
            continue;
        }
        let title = if entry.title.is_empty() {
            entry.url.replace('_', " ")
        } else {
            entry.title.clone()
        };
        batch.insert(&path, &title, &body)?;
        added += 1;
    }
    if let Some(last) = items.last() {
        batch.dequeue_through(*last)?;
    }
    let pending = pending_before.saturating_sub(items.len() as u64);
    batch.commit(cursor, articles_before + added, pending)?;
    Ok(added)
}

// ---------------------------------------------------------------------------
// Exclusions (archives whose index an admin deleted)
// ---------------------------------------------------------------------------

fn load_excluded(index_dir: &Path) -> HashSet<String> {
    std::fs::read(index_dir.join(EXCLUDED_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .map(|names| names.into_iter().filter(|n| super::is_valid_archive_name(n)).collect())
        .unwrap_or_default()
}

fn save_excluded(index_dir: &Path, excluded: &HashSet<String>) {
    let mut names: Vec<&String> = excluded.iter().collect();
    names.sort();
    let Ok(json) = serde_json::to_vec_pretty(&names) else { return };
    if std::fs::create_dir_all(index_dir).is_err() {
        return;
    }
    let tmp = index_dir.join(format!("{EXCLUDED_FILE}.tmp"));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, index_dir.join(EXCLUDED_FILE));
    }
}
