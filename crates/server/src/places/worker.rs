//! The background place indexer.
//!
//! One `std::thread` ("fyr-places") indexes one map at a time, following
//! the same rules as the full-text indexer (`crate::fulltext::worker`):
//!
//! * **Picking work:** admin build requests first (FIFO), then a scan of
//!   `maps/` resumes unfinished indexes and starts vector maps no larger
//!   than `auto_max_bytes` that the admin has not excluded (deleting an
//!   index excludes the map until a build is requested again). Indexes of
//!   maps that changed or disappeared are deleted. Raster and unreadable
//!   maps are skipped (their status says why).
//! * **Indexing** (see [`run_job`]): tiles of a few zoom levels
//!   ([`super::zoom_plan`]) are read in archive order, `chunk_tiles` per
//!   transaction; every transaction stores the places, the resume position
//!   and the counters together. After each transaction the thread sleeps
//!   `throttle` × its work time.
//! * **Stopping:** each chunk (and every few tiles inside it) checks for
//!   shutdown, an admin delete, preemption by an admin build, a map that
//!   changed or vanished, and free disk space under the guard. An
//!   interrupted transaction rolls back, so the stored position is always a
//!   clean resume point.

use super::store::{self, PlaceWriter};
use super::tiles::{self, MapContent, MbtilesArchive, PmtilesArchive, PmtilesWalk};
use super::{index_path, MapFormat};
use crate::fulltext::store::{ArchiveStamp, IndexState};
use crate::fulltext::worker::{low_disk_at, throttle_pause, CancelReason};
use crate::fulltext::{index_size, remove_index_files};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tracing::{info, warn};

const EXCLUDED_FILE: &str = "places-excluded.json";
/// Cancellation is checked every this many tiles inside a chunk.
const CANCEL_CHECK_EVERY: u32 = 16;
/// A chunk also ends after this many decompressed tile bytes.
const CHUNK_BYTES: usize = 16 * 1024 * 1024;
/// Identical PMTiles runs with named features are expanded to at most this
/// many tile positions (runs are normally empty ocean or land).
const MAX_RUN_EXPANSION: u64 = 256;

/// Runtime settings (from [`types::Config`], overridable in tests).
#[derive(Debug, Clone)]
pub struct PlacesSettings {
    pub index_dir: PathBuf,
    pub maps_dir: PathBuf,
    pub enabled: bool,
    /// Vector maps up to this many bytes are indexed automatically
    /// (0 = only on admin request).
    pub auto_max_bytes: u64,
    /// Pause while free space on the index volume is below this.
    pub min_free_bytes: u64,
    /// Sleep `throttle` × work time after each chunk.
    pub throttle: f64,
    /// Tile entries per transaction.
    pub chunk_tiles: u32,
    /// Idle rescan period (and back-off after a disk-space pause).
    pub rescan_interval: Duration,
}

impl PlacesSettings {
    pub fn from_config(config: &types::Config) -> Self {
        Self {
            index_dir: config.index_dir(),
            maps_dir: config.maps_dir(),
            enabled: config.places.enabled,
            auto_max_bytes: config.places.max_map_mb.saturating_mul(1024 * 1024),
            // One disk guard for every background index.
            min_free_bytes: config.fulltext.min_free_mb.saturating_mul(1024 * 1024),
            throttle: 0.5,
            chunk_tiles: 256,
            rescan_interval: Duration::from_secs(60),
        }
    }
}

/// Shared between the worker and the API for one running job.
#[derive(Debug, Default)]
pub struct PlaceJob {
    pub(super) cancel: AtomicU8,
    pub processed: AtomicU64,
    pub total: AtomicU64,
    pub places: AtomicU64,
    /// Admin-requested job (ignores the automatic size cap, not preempted).
    pub forced: bool,
    /// Test hook: stop (as [`CancelReason::Yield`]) after this many chunks.
    pub max_chunks: Option<u32>,
}

impl PlaceJob {
    pub fn new(forced: bool) -> Self {
        Self {
            forced,
            ..Default::default()
        }
    }

    pub fn cancel(&self, reason: CancelReason) {
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
    Completed { places: u64 },
    Cancelled(CancelReason),
    /// Free space is under the guard; resumable.
    LowDisk,
    /// The map changed or vanished; its index was deleted.
    Stale,
    /// Raster or unreadable map: nothing to index (no index file is kept).
    Unsupported(String),
    Failed(String),
}

struct ActiveJob {
    filename: String,
    control: Arc<PlaceJob>,
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
    /// What each map version holds, so scans do not reopen every map.
    probes: HashMap<String, (ArchiveStamp, MapContent)>,
}

struct Shared {
    state: Mutex<WorkerState>,
    wake: Condvar,
    idle: Condvar,
}

fn lock(shared: &Shared) -> MutexGuard<'_, WorkerState> {
    shared.state.lock().unwrap_or_else(|p| p.into_inner())
}

/// Errors for the admin API.
#[derive(Debug, PartialEq, Eq)]
pub enum PlacesError {
    Disabled,
    InvalidName,
    NotFound,
    /// Raster or unreadable map (path-free reason).
    Unsupported(String),
    Busy,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildOutcome {
    Queued,
    Running,
    AlreadyReady,
}

/// Per-map status for `GET /api/places/status`. Filenames and counters
/// only: no paths.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MapPlacesStatus {
    pub filename: String,
    /// `not_indexed`, `queued`, `indexing`, `paused`, `ready`, `failed`,
    /// `excluded`, `disabled` or `unsupported`.
    pub state: &'static str,
    /// `vector`, `raster` or `unknown`.
    pub content: &'static str,
    pub places: u64,
    pub processed_tiles: u64,
    pub total_tiles: u64,
    pub percent: f64,
    pub auto: bool,
    pub index_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacesStatusResponse {
    pub enabled: bool,
    pub auto_max_mb: u64,
    pub min_free_mb: u64,
    pub low_disk: bool,
    pub maps: Vec<MapPlacesStatus>,
}

/// Owner of the background place indexer (one per server).
pub struct PlaceIndexer {
    settings: PlacesSettings,
    shared: Arc<Shared>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl PlaceIndexer {
    pub fn new(settings: PlacesSettings) -> Self {
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
        Self::new(PlacesSettings::from_config(config))
    }

    pub fn settings(&self) -> &PlacesSettings {
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
            .name("fyr-places".to_string())
            .spawn(move || worker_loop(shared, settings))
        {
            Ok(handle) => *self.thread.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle),
            Err(error) => {
                warn!("Place indexer could not start: {}", error);
                lock(&self.shared).started = false;
            }
        }
    }

    /// Wake the worker (e.g. after a new map arrived).
    pub fn notify(&self) {
        lock(&self.shared).wake = true;
        self.shared.wake.notify_all();
    }

    /// Stop the worker (the running job stays resumable) and wait up to
    /// `wait` for the thread to exit.
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
                warn!("Place indexer did not stop within {:?}", wait);
            }
        }
    }

    fn map_path(&self, filename: &str) -> Result<PathBuf, PlacesError> {
        if !super::is_valid_map_name(filename) {
            return Err(PlacesError::InvalidName);
        }
        let path = self.settings.maps_dir.join(filename);
        if !path.is_file() {
            return Err(PlacesError::NotFound);
        }
        Ok(path)
    }

    /// Admin request to (re)build the index of `filename`, regardless of the
    /// automatic size cap. A preemptible automatic job yields to it.
    pub fn request_build(&self, filename: &str) -> Result<BuildOutcome, PlacesError> {
        if !self.settings.enabled {
            return Err(PlacesError::Disabled);
        }
        let path = self.map_path(filename)?;
        let stamp = ArchiveStamp::of(&path).ok_or(PlacesError::NotFound)?;
        match probe_cached(&self.shared, filename, &path, stamp) {
            MapContent::Vector { .. } => {}
            MapContent::Raster => return Err(PlacesError::Unsupported(raster_note().to_string())),
            MapContent::Unsupported(reason) => return Err(PlacesError::Unsupported(reason)),
        }
        let index = index_path(&self.settings.index_dir, filename);

        let mut state = lock(&self.shared);
        if state.excluded.remove(filename) {
            save_excluded(&self.settings.index_dir, &state.excluded);
        }
        if state.current.as_ref().is_some_and(|j| j.filename == filename) {
            return Ok(BuildOutcome::Running);
        }
        if let Some(meta) = store::inspect(&index) {
            if meta.matches(filename, &stamp) && meta.state == IndexState::Ready {
                return Ok(BuildOutcome::AlreadyReady);
            }
            if !meta.matches(filename, &stamp) || meta.state == IndexState::Failed {
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
    /// map is excluded from automatic indexing until a build is requested.
    pub fn remove(&self, filename: &str, wait: Duration) -> Result<(), PlacesError> {
        if !super::is_valid_map_name(filename) {
            return Err(PlacesError::InvalidName);
        }
        {
            let mut state = lock(&self.shared);
            state.excluded.insert(filename.to_string());
            save_excluded(&self.settings.index_dir, &state.excluded);
        }
        self.drop_index(filename, wait)
    }

    /// Called before a map is deleted from `maps/`: stop indexing it, delete
    /// its index and forget any exclusion.
    pub fn forget_map(&self, filename: &str, wait: Duration) {
        if !super::is_valid_map_name(filename) {
            return;
        }
        {
            let mut state = lock(&self.shared);
            if state.excluded.remove(filename) {
                save_excluded(&self.settings.index_dir, &state.excluded);
            }
            state.probes.remove(filename);
        }
        let _ = self.drop_index(filename, wait);
    }

    fn drop_index(&self, filename: &str, wait: Duration) -> Result<(), PlacesError> {
        let mut state = lock(&self.shared);
        state.queue.retain(|f| f != filename);
        if let Some(job) = state.current.as_ref().filter(|j| j.filename == filename) {
            job.control.cancel(CancelReason::Removed);
        }
        let end = Instant::now() + wait;
        while state.current.as_ref().is_some_and(|j| j.filename == filename) {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(PlacesError::Busy);
            }
            state = self.shared.idle.wait_timeout(state, left).unwrap_or_else(|p| p.into_inner()).0;
        }
        drop(state);
        if remove_index_files(&index_path(&self.settings.index_dir, filename)) {
            Ok(())
        } else {
            Err(PlacesError::Busy)
        }
    }

    /// Status of every map in `maps/`. Blocking (reads index metadata and
    /// map headers); call on the blocking pool.
    pub fn status(&self) -> PlacesStatusResponse {
        let settings = &self.settings;
        let (queue, current, excluded, low_disk) = {
            let state = lock(&self.shared);
            (
                state.queue.clone(),
                state.current.as_ref().map(|j| (j.filename.clone(), Arc::clone(&j.control))),
                state.excluded.clone(),
                state.low_disk,
            )
        };
        let maps = list_maps(&settings.maps_dir)
            .into_iter()
            .map(|(filename, path)| {
                let stamp = ArchiveStamp::of(&path);
                let content = stamp.map(|s| probe_cached(&self.shared, &filename, &path, s));
                let index = index_path(&settings.index_dir, &filename);
                let meta = store::inspect(&index).filter(|m| stamp.is_some_and(|s| m.matches(&filename, &s)));
                let vector = matches!(content, Some(MapContent::Vector { .. }));
                let mut status = MapPlacesStatus {
                    filename: filename.clone(),
                    state: "not_indexed",
                    content: match content {
                        Some(MapContent::Vector { .. }) => "vector",
                        Some(MapContent::Raster) => "raster",
                        _ => "unknown",
                    },
                    places: 0,
                    processed_tiles: 0,
                    total_tiles: 0,
                    percent: 0.0,
                    auto: vector
                        && settings.auto_max_bytes > 0
                        && stamp.is_some_and(|s| s.len <= settings.auto_max_bytes),
                    index_bytes: index_size(&index),
                    note: None,
                };
                if let Some(meta) = &meta {
                    status.places = meta.places;
                    status.processed_tiles = meta.processed;
                    status.total_tiles = meta.total;
                    status.note = meta.note.clone().or_else(|| bad_tiles_note(meta.bad_tiles));
                    status.state = match meta.state {
                        IndexState::Ready => "ready",
                        IndexState::Failed => "failed",
                        IndexState::Building => "paused",
                    };
                }
                match (&current, &content) {
                    (Some((name, control)), _) if *name == filename => {
                        status.state = "indexing";
                        status.processed_tiles = control.processed.load(Ordering::Relaxed);
                        status.total_tiles = control.total.load(Ordering::Relaxed);
                        status.places = control.places.load(Ordering::Relaxed);
                    }
                    (_, Some(MapContent::Raster)) => {
                        status.state = "unsupported";
                        status.note = Some(raster_note().to_string());
                    }
                    (_, Some(MapContent::Unsupported(reason))) => {
                        status.state = "unsupported";
                        status.note = Some(reason.clone());
                    }
                    _ if queue.iter().any(|q| *q == filename) => status.state = "queued",
                    _ if excluded.contains(&filename) && meta.is_none() => status.state = "excluded",
                    _ if !settings.enabled => status.state = "disabled",
                    _ => {}
                }
                if status.state == "paused" && low_disk {
                    status.note = Some("Paused: free disk space is below the indexing threshold".to_string());
                }
                status.percent = match status.state {
                    "ready" => 100.0,
                    _ if status.total_tiles == 0 => 0.0,
                    _ => {
                        let pct = 100.0 * status.processed_tiles as f64 / status.total_tiles as f64;
                        (pct.min(100.0) * 10.0).round() / 10.0
                    }
                };
                status
            })
            .collect();
        PlacesStatusResponse {
            enabled: settings.enabled,
            auto_max_mb: settings.auto_max_bytes / (1024 * 1024),
            min_free_mb: settings.min_free_bytes / (1024 * 1024),
            low_disk,
            maps,
        }
    }
}

fn raster_note() -> &'static str {
    "Raster map: there are no place names to index"
}

fn bad_tiles_note(bad: u64) -> Option<String> {
    (bad > 0).then(|| format!("{bad} tiles could not be read and were skipped"))
}

fn probe_cached(shared: &Shared, filename: &str, path: &Path, stamp: ArchiveStamp) -> MapContent {
    if let Some((cached, content)) = lock(shared).probes.get(filename) {
        if *cached == stamp {
            return content.clone();
        }
    }
    let Some(format) = MapFormat::of(filename) else {
        return MapContent::Unsupported("Not a map".to_string());
    };
    let content = super::probe_map(path, format);
    lock(shared).probes.insert(filename.to_string(), (stamp, content.clone()));
    content
}

/// `(filename, path)` of every `.pmtiles` / `.mbtiles` file in `maps_dir`.
pub fn list_maps(maps_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(maps_dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let path = entry.path();
            (super::is_valid_map_name(&name) && path.is_file()).then_some((name, path))
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

fn worker_loop(shared: Arc<Shared>, settings: PlacesSettings) {
    info!("Place indexer started");
    let mut backoff_until: Option<Instant> = None;
    loop {
        if lock(&shared).shutdown {
            break;
        }
        let in_backoff =
            backoff_until.is_some_and(|until| Instant::now() < until) && lock(&shared).queue.is_empty();
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

        let control = Arc::new(PlaceJob::new(forced));
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
        info!("Indexing places of {}", filename);
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| run_job(&settings, &filename, &control)))
            .unwrap_or_else(|_| {
                mark_failed(&settings, &filename, "The indexer crashed on this map");
                JobOutcome::Failed("indexer crashed".to_string())
            });
        match &outcome {
            JobOutcome::Completed { places } => info!("Place index ready for {} ({} places)", filename, places),
            JobOutcome::Failed(reason) => warn!("Place indexing failed for {}: {}", filename, reason),
            JobOutcome::LowDisk => warn!("Place indexing paused: free disk space below threshold"),
            other => info!("Place indexing of {} stopped: {:?}", filename, other),
        }
        {
            let mut state = lock(&shared);
            state.current = None;
            state.low_disk = outcome == JobOutcome::LowDisk;
        }
        shared.idle.notify_all();
        match outcome {
            JobOutcome::Cancelled(CancelReason::Shutdown) => break,
            JobOutcome::LowDisk | JobOutcome::Failed(_) => backoff_until = Some(Instant::now() + settings.rescan_interval),
            _ => {}
        }
    }
    shared.idle.notify_all();
    info!("Place indexer stopped");
}

fn low_disk(settings: &PlacesSettings) -> bool {
    low_disk_at(&settings.index_dir, settings.min_free_bytes)
}

fn pick_job(shared: &Shared, settings: &PlacesSettings) -> Option<(String, bool)> {
    if low_disk(settings) {
        lock(shared).low_disk = true;
        return None;
    }
    lock(shared).low_disk = false;

    loop {
        let next = lock(shared).queue.pop_front();
        let Some(filename) = next else { break };
        if settings.maps_dir.join(&filename).is_file() {
            return Some((filename, true));
        }
    }

    let maps = list_maps(&settings.maps_dir);
    remove_orphans(settings, &maps);
    let excluded = lock(shared).excluded.clone();
    for (filename, path) in maps {
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
                _ => {
                    remove_index_files(&index);
                }
            }
        }
        if settings.auto_max_bytes == 0 || stamp.len > settings.auto_max_bytes {
            continue;
        }
        if matches!(probe_cached(shared, &filename, &path, stamp), MapContent::Vector { .. }) {
            return Some((filename, false));
        }
    }
    None
}

/// Delete place indexes whose map no longer exists.
fn remove_orphans(settings: &PlacesSettings, maps: &[(String, PathBuf)]) {
    let expected: HashSet<String> = maps.iter().map(|(name, _)| super::index_file_name(name)).collect();
    let Ok(entries) = std::fs::read_dir(&settings.index_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if super::is_index_file_name(&name) && !expected.contains(&name) {
            remove_index_files(&entry.path());
        }
    }
}

fn mark_failed(settings: &PlacesSettings, filename: &str, note: &str) {
    let path = index_path(&settings.index_dir, filename);
    if path.is_file() {
        if let Ok((writer, _)) = PlaceWriter::open(&path) {
            let _ = writer.mark_failed(note);
        }
    }
}

// ---------------------------------------------------------------------------
// Tile sources
// ---------------------------------------------------------------------------

enum Source {
    Pm(PmtilesArchive),
    Mb(MbtilesArchive),
}

impl Source {
    fn open(path: &Path, format: MapFormat) -> Result<Self, String> {
        match format {
            MapFormat::Pmtiles => PmtilesArchive::open(path).map(Source::Pm),
            MapFormat::Mbtiles => MbtilesArchive::open(path).map(Source::Mb),
        }
    }

    fn content(&self) -> MapContent {
        match self {
            Source::Pm(archive) => archive.content(),
            Source::Mb(archive) => archive.content(),
        }
    }

    /// Tile entries at zoom `z` (PMTiles runs count once).
    fn count(&mut self, z: u8, keep_going: impl FnMut() -> bool) -> Result<u64, String> {
        match self {
            Source::Pm(archive) => archive.count(tiles::zoom_first_id(z), tiles::zoom_first_id(z + 1), keep_going),
            Source::Mb(archive) => archive.count(z).map_err(|_| "The map could not be read".to_string()),
        }
    }
}

/// One tile (or PMTiles run of identical tiles) to process.
struct TileItem {
    z: u8,
    /// Tile positions sharing `data`: XYZ for MBTiles, or a run of IDs.
    at: TileAt,
    /// Compressed bytes, `None` when unreadable or too large.
    data: Option<Vec<u8>>,
    /// Resume position after this item.
    next_pos: u64,
}

enum TileAt {
    Xyz(u32, u32),
    Run { first: u64, count: u64 },
    Invalid,
}

enum PassCursor {
    Pm(PmtilesWalk),
    Mb {
        z: u8,
        col: u32,
        row: u32,
        buffer: VecDeque<tiles::MbTile>,
        done: bool,
    },
}

impl PassCursor {
    fn new(source: &mut Source, z: u8, pos: u64) -> Result<Self, String> {
        match source {
            Source::Pm(archive) => {
                let lo = tiles::zoom_first_id(z).max(pos);
                Ok(PassCursor::Pm(archive.walk(lo, tiles::zoom_first_id(z + 1))?))
            }
            Source::Mb(_) => Ok(PassCursor::Mb {
                z,
                col: (pos >> 32) as u32,
                row: pos as u32,
                buffer: VecDeque::new(),
                done: false,
            }),
        }
    }

    fn next(&mut self, source: &mut Source, z: u8) -> Result<Option<TileItem>, String> {
        match (self, source) {
            (PassCursor::Pm(walk), Source::Pm(archive)) => {
                let Some(entry) = walk.next(archive)? else { return Ok(None) };
                let data = archive.tile_data(&entry);
                Ok(Some(TileItem {
                    z,
                    at: TileAt::Run {
                        first: entry.tile_id,
                        count: u64::from(entry.run_length),
                    },
                    data,
                    next_pos: entry.tile_id.saturating_add(u64::from(entry.run_length)),
                }))
            }
            (PassCursor::Mb { z, col, row, buffer, done }, Source::Mb(archive)) => {
                if buffer.is_empty() && !*done {
                    let fetched = archive
                        .tiles_from(*z, *col, *row, 64)
                        .map_err(|_| "The map could not be read".to_string())?;
                    if fetched.is_empty() {
                        *done = true;
                    }
                    buffer.extend(fetched);
                }
                let Some(tile) = buffer.pop_front() else { return Ok(None) };
                // Advance past this tile (row + 1 never overflows: rows < 2^26).
                *col = tile.x;
                *row = tile.row.saturating_add(1);
                let next_pos = (u64::from(*col) << 32) | u64::from(*row);
                Ok(Some(TileItem {
                    z: *z,
                    at: if tile.y == u32::MAX { TileAt::Invalid } else { TileAt::Xyz(tile.x, tile.y) },
                    data: tile.data,
                    next_pos,
                }))
            }
            _ => Err("tile source mismatch".to_string()),
        }
    }
}

/// Places in one tile item; `None` when the tile could not be decoded.
fn places_in(item: &TileItem, compression: Option<tiles::Compression>) -> Option<Vec<super::Place>> {
    let raw = item.data.as_ref()?;
    let decoded = tiles::decompress(raw, compression, tiles::MAX_DECODED_TILE_BYTES)
        .or_else(|| tiles::decompress(raw, None, tiles::MAX_DECODED_TILE_BYTES))?;
    let candidates = super::extract_candidates(&decoded).ok()?;
    if candidates.is_empty() {
        return Some(Vec::new());
    }
    let mut places = Vec::new();
    match item.at {
        TileAt::Xyz(x, y) => {
            places.extend(candidates.iter().filter_map(|c| super::locate(c, item.z, x, y)));
        }
        TileAt::Run { first, count } => {
            for id in first..first.saturating_add(count.min(MAX_RUN_EXPANSION)) {
                let Some((z, x, y)) = tiles::tile_id_to_zxy(id) else { break };
                places.extend(candidates.iter().filter_map(|c| super::locate(c, z, x, y)));
            }
        }
        TileAt::Invalid => return None,
    }
    Some(places)
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

enum ChunkError {
    Cancelled(CancelReason),
    Db(rusqlite::Error),
    Source(String),
}

impl From<rusqlite::Error> for ChunkError {
    fn from(error: rusqlite::Error) -> Self {
        ChunkError::Db(error)
    }
}

/// Index (or resume indexing) one map. Blocking.
pub fn run_job(settings: &PlacesSettings, filename: &str, control: &Arc<PlaceJob>) -> JobOutcome {
    let map_path = settings.maps_dir.join(filename);
    let Some(format) = MapFormat::of(filename) else {
        return JobOutcome::Unsupported("Not a map".to_string());
    };
    let Some(stamp) = ArchiveStamp::of(&map_path) else {
        return JobOutcome::Stale;
    };
    if std::fs::create_dir_all(&settings.index_dir).is_err() {
        return JobOutcome::Failed("index directory is not writable".to_string());
    }
    if low_disk(settings) {
        return JobOutcome::LowDisk;
    }
    let index = index_path(&settings.index_dir, filename);

    let mut source = match std::panic::catch_unwind(AssertUnwindSafe(|| Source::open(&map_path, format))) {
        Ok(Ok(source)) => source,
        Ok(Err(reason)) => {
            remove_index_files(&index);
            return JobOutcome::Unsupported(reason);
        }
        Err(_) => return JobOutcome::Unsupported("The map could not be read".to_string()),
    };
    let zooms = match source.content() {
        MapContent::Vector { min_zoom, max_zoom } => super::zoom_plan(min_zoom, max_zoom),
        MapContent::Raster => {
            remove_index_files(&index);
            return JobOutcome::Unsupported(raster_note().to_string());
        }
        MapContent::Unsupported(reason) => {
            remove_index_files(&index);
            return JobOutcome::Unsupported(reason);
        }
    };
    if zooms.is_empty() {
        return JobOutcome::Unsupported("The map has no usable zoom levels".to_string());
    }
    let tile_compression = match &source {
        Source::Pm(archive) => Some(archive.header.tile_compression),
        Source::Mb(_) => None,
    };

    let existing = store::inspect(&index).filter(|m| m.matches(filename, &stamp));
    let opened = match &existing {
        Some(meta) if meta.state == IndexState::Ready => return JobOutcome::Completed { places: meta.places },
        Some(meta) if meta.state == IndexState::Building && meta.zooms == zooms => PlaceWriter::open(&index),
        _ => {
            remove_index_files(&index);
            let mut total = 0u64;
            for z in &zooms {
                match source.count(*z, || control.cancelled().is_none()) {
                    Ok(n) => total += n,
                    Err(_) if control.cancelled().is_some() => {
                        return JobOutcome::Cancelled(control.cancelled().unwrap_or(CancelReason::Yield))
                    }
                    Err(reason) => return JobOutcome::Unsupported(reason),
                }
            }
            PlaceWriter::create(&index, filename, stamp, &zooms, total)
        }
    };
    let (mut writer, meta) = match opened {
        Ok(opened) => opened,
        Err(error) if store::disk_full(&error) => return JobOutcome::LowDisk,
        Err(_) => return JobOutcome::Failed("index database could not be opened".to_string()),
    };

    let mut pass = meta.pass as usize;
    let mut pos = meta.pos;
    let mut processed = meta.processed;
    let mut places = meta.places;
    let mut bad_tiles = meta.bad_tiles;
    control.total.store(meta.total, Ordering::Relaxed);
    control.processed.store(processed, Ordering::Relaxed);
    control.places.store(places, Ordering::Relaxed);

    let check = |writer: PlaceWriter| -> Result<PlaceWriter, JobOutcome> {
        if let Some(reason) = control.cancelled() {
            return Err(JobOutcome::Cancelled(reason));
        }
        if ArchiveStamp::of(&map_path) != Some(stamp) {
            drop(writer);
            remove_index_files(&index);
            return Err(JobOutcome::Stale);
        }
        if low_disk(settings) {
            return Err(JobOutcome::LowDisk);
        }
        Ok(writer)
    };
    let failure = |writer: &PlaceWriter, error: ChunkError| -> JobOutcome {
        match error {
            ChunkError::Cancelled(reason) => JobOutcome::Cancelled(reason),
            ChunkError::Db(error) if store::disk_full(&error) => JobOutcome::LowDisk,
            ChunkError::Db(_) => {
                let _ = writer.mark_failed("Writing the place index failed");
                JobOutcome::Failed("index write failed".to_string())
            }
            ChunkError::Source(reason) => {
                warn!("Place indexing of {} stopped: {}", filename, reason);
                let _ = writer.mark_failed("The map's tile directory is damaged");
                JobOutcome::Failed("map directory unreadable".to_string())
            }
        }
    };

    let mut chunks = 0u32;
    while pass < zooms.len() {
        let z = zooms[pass];
        let mut cursor = match PassCursor::new(&mut source, z, pos) {
            Ok(cursor) => cursor,
            Err(reason) => return failure(&writer, ChunkError::Source(reason)),
        };
        loop {
            writer = match check(writer) {
                Ok(writer) => writer,
                Err(outcome) => return outcome,
            };
            let started = Instant::now();
            let result = (|| -> Result<bool, ChunkError> {
                let batch = writer.batch()?;
                let mut tiles_done = 0u32;
                let mut bytes = 0usize;
                let (mut new_pos, mut new_processed, mut new_places, mut new_bad) = (pos, processed, places, bad_tiles);
                let mut exhausted = false;
                while tiles_done < settings.chunk_tiles.max(1) && bytes < CHUNK_BYTES {
                    if tiles_done % CANCEL_CHECK_EVERY == 0 {
                        if let Some(reason) = control.cancelled() {
                            return Err(ChunkError::Cancelled(reason));
                        }
                    }
                    let Some(item) = cursor.next(&mut source, z).map_err(ChunkError::Source)? else {
                        exhausted = true;
                        break;
                    };
                    bytes += item.data.as_ref().map_or(0, Vec::len);
                    match places_in(&item, tile_compression) {
                        Some(found) => {
                            for place in &found {
                                if batch.add(place)? == store::Added::Inserted {
                                    new_places += 1;
                                }
                            }
                        }
                        None => new_bad += 1,
                    }
                    new_pos = item.next_pos;
                    new_processed += 1;
                    tiles_done += 1;
                }
                if exhausted {
                    batch.commit(pass as u32 + 1, 0, new_processed, new_places, new_bad)?;
                } else {
                    batch.commit(pass as u32, new_pos, new_processed, new_places, new_bad)?;
                }
                pos = if exhausted { 0 } else { new_pos };
                processed = new_processed;
                places = new_places;
                bad_tiles = new_bad;
                Ok(exhausted)
            })();
            let exhausted = match result {
                Ok(exhausted) => exhausted,
                Err(error) => return failure(&writer, error),
            };
            control.processed.store(processed, Ordering::Relaxed);
            control.places.store(places, Ordering::Relaxed);
            chunks += 1;
            if exhausted {
                break;
            }
            if control.max_chunks.is_some_and(|max| chunks >= max) {
                return JobOutcome::Cancelled(CancelReason::Yield);
            }
            throttle_pause(settings.throttle, started.elapsed(), || control.cancelled().is_some());
        }
        pass += 1;
        if control.max_chunks.is_some_and(|max| chunks >= max) && pass < zooms.len() {
            return JobOutcome::Cancelled(CancelReason::Yield);
        }
    }

    if let Some(reason) = control.cancelled() {
        return JobOutcome::Cancelled(reason);
    }
    let probe = Arc::clone(control);
    match writer.finish(move || probe.cancelled().is_some()) {
        Ok(()) => JobOutcome::Completed { places },
        Err(error) if store::disk_full(&error) => JobOutcome::LowDisk,
        Err(_) => match control.cancelled() {
            Some(reason) => JobOutcome::Cancelled(reason),
            None => JobOutcome::Failed("index optimisation failed".to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// Exclusions
// ---------------------------------------------------------------------------

fn load_excluded(index_dir: &Path) -> HashSet<String> {
    std::fs::read(index_dir.join(EXCLUDED_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .map(|names| names.into_iter().filter(|n| super::is_valid_map_name(n)).collect())
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
