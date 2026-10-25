//! Download task manager

use crate::router::ContentRouter;
use tokio_stream::StreamExt;
use tokio::io::AsyncWriteExt;
use types::{ContentType, DownloadSource, DownloadStatus, DownloadTask};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;
use tracing::{error, info, warn};

pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 300;
pub const MIN_REQUEST_TIMEOUT_SECS: u64 = 30;
pub const MAX_REQUEST_TIMEOUT_SECS: u64 = 86_400;
const CONNECT_TIMEOUT_SECS: u64 = 15;
/// Consecutive failed attempts (without the partial file growing) before a
/// URL download gives up. An attempt that adds new bytes to the partial file
/// resets the counter, so a flaky link that keeps making progress is resumed
/// until it finishes.
const MAX_DOWNLOAD_ATTEMPTS: u8 = 3;
const RETRY_BACKOFF_MS: u64 = 500;
const PROGRESS_SYNC_BYTES: u64 = 1024 * 1024;

/// Error stored on URL tasks interrupted by a restart whose partial file was kept.
pub const INTERRUPTED_BY_RESTART_MESSAGE: &str = "interrupted by server restart – retry to resume";
/// Error stored on URL tasks stopped by a graceful shutdown.
pub const INTERRUPTED_BY_SHUTDOWN_MESSAGE: &str = "interrupted by server shutdown – retry to resume";
const CANCELLED_BY_USER_MESSAGE: &str = "download cancelled by user";

/// Optional per-task settings supplied when a download is created.
#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    /// Expected SHA-256 of the finished file (64 hex characters, any case).
    pub sha256: Option<String>,
}

/// Why a task could not be re-queued by [`DownloadManager::retry_task`].
#[derive(Debug, thiserror::Error)]
pub enum RetryError {
    #[error("download task not found")]
    NotFound,
    #[error("{0}")]
    NotRetriable(&'static str),
    #[error("failed to persist download tasks: {0}")]
    Persist(anyhow::Error),
}

/// Normalise a user-supplied SHA-256 hex digest. Returns `None` unless the
/// input is exactly 64 hex characters (surrounding whitespace is ignored).
pub fn normalize_sha256(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

/// Process-wide shutdown signal shared with every worker.
#[derive(Default)]
struct ShutdownSignal {
    requested: AtomicBool,
    notify: Notify,
}

impl ShutdownSignal {
    fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

/// Manages active and completed download tasks
pub struct DownloadManager {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    cancel_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    persistence_path: PathBuf,
    data_dir: PathBuf,
    client: Arc<RwLock<reqwest::Client>>,
    shutdown: Arc<ShutdownSignal>,
}

#[derive(Clone)]
struct DownloadRuntime {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    cancel_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    persistence_path: PathBuf,
    data_dir: PathBuf,
    client: reqwest::Client,
    shutdown: Arc<ShutdownSignal>,
}

/// Result of transferring a URL into its partial file.
enum FetchOutcome {
    /// The partial file holds the complete remote file.
    Complete { bytes: u64, total: Option<u64> },
    Cancelled,
    Shutdown,
    Failed { message: String, keep_partial: bool },
}

/// How one response body stream ended.
enum StreamEnd {
    Finished(u64),
    Cancelled,
    Shutdown,
    Network(String),
    Write(String),
}

impl DownloadManager {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let root = data_dir.as_ref().to_path_buf();
        let persistence_path = data_dir.as_ref().join("download_tasks.json");
        let mut tasks = Self::load_tasks(&persistence_path).unwrap_or_else(|error| {
            warn!(
                "Failed to load download tasks from {}: {}",
                persistence_path.display(),
                error
            );
            HashMap::new()
        });

        if Self::recover_interrupted_tasks(&mut tasks, &root) > 0 {
            if let Err(error) = Self::persist_tasks_at(&persistence_path, &tasks) {
                error!("Failed to persist recovered download tasks: {}", error);
            }
        }

        // Runs after recovery so partials of resumable tasks are known and kept.
        Self::cleanup_stale_temp_files(&root, &Self::resumable_partials(&tasks, &root));

        let client = Self::build_http_client(DEFAULT_REQUEST_TIMEOUT_SECS);

        Self {
            tasks: Arc::new(RwLock::new(tasks)),
            cancel_flags: Arc::new(RwLock::new(HashMap::new())),
            persistence_path,
            data_dir: root,
            client: Arc::new(RwLock::new(client)),
            shutdown: Arc::new(ShutdownSignal::default()),
        }
    }

    pub async fn set_request_timeout_secs(&self, timeout_secs: u64) {
        let bounded = timeout_secs.clamp(MIN_REQUEST_TIMEOUT_SECS, MAX_REQUEST_TIMEOUT_SECS);
        let next_client = Self::build_http_client(bounded);

        {
            let mut guard = self.client.write().await;
            *guard = next_client;
        }

        info!(
            "Download HTTP timeout configured to {}s (requested {}s)",
            bounded,
            timeout_secs
        );
    }

    /// Create a new download task
    pub async fn create_task(&self, source: DownloadSource) -> String {
        self.create_task_with_options(source, DownloadOptions::default()).await
    }

    /// Create a new download task with optional checksum verification.
    ///
    /// An invalid `sha256` is ignored here; HTTP callers validate it with
    /// [`normalize_sha256`] first so the user gets a 400 instead.
    pub async fn create_task_with_options(&self, source: DownloadSource, options: DownloadOptions) -> String {
        let task_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().to_rfc3339();

        let task = DownloadTask {
            id: task_id.clone(),
            source: source.clone(),
            status: DownloadStatus::Queued,
            progress: 0.0,
            bytes_downloaded: 0,
            total_bytes: None,
            error: None,
            content_type: None,
            created_at: now.clone(),
            updated_at: now,
            sha256: options.sha256.as_deref().and_then(normalize_sha256),
            etag: None,
            last_modified: None,
            resumable: false,
            resumed_from: None,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let snapshot = {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id.clone(), task);
            tasks.clone()
        };
        {
            let mut flags = self.cancel_flags.write().await;
            flags.insert(task_id.clone(), Arc::clone(&cancel_flag));
        }

        if let Err(error) = self.persist_tasks(&snapshot) {
            error!("Failed to persist download tasks after create: {}", error);
        }

        let runtime = self.runtime().await;
        let task_id_for_worker = task_id.clone();
        match source {
            DownloadSource::Url { url } => {
                tokio::spawn(async move {
                    Self::run_url_download(runtime, task_id_for_worker, url, cancel_flag).await;
                });
            }
            DownloadSource::LocalFile { path } => {
                tokio::spawn(async move {
                    Self::run_local_import(runtime, task_id_for_worker, path).await;
                });
            }
        }

        task_id
    }

    /// Re-queue a failed or cancelled URL task, resuming from its partial
    /// file when one was kept.
    pub async fn retry_task(&self, task_id: &str) -> Result<DownloadTask, RetryError> {
        if self.shutdown.is_requested() {
            return Err(RetryError::NotRetriable("server is shutting down"));
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let (url, retried, snapshot) = {
            // Lock order: cancel_flags, then tasks. Nothing else holds `tasks`
            // while waiting for `cancel_flags`, so this cannot deadlock. Holding
            // both makes the "no worker running" check and the re-queue atomic.
            let mut flags = self.cancel_flags.write().await;
            let mut tasks = self.tasks.write().await;
            let task = tasks.get_mut(task_id).ok_or(RetryError::NotFound)?;

            let DownloadSource::Url { url } = &task.source else {
                return Err(RetryError::NotRetriable("only URL downloads can be retried"));
            };
            let url = url.clone();
            if !matches!(task.status, DownloadStatus::Failed | DownloadStatus::Cancelled) {
                return Err(RetryError::NotRetriable("only failed or cancelled downloads can be retried"));
            }
            if flags.contains_key(task_id) {
                return Err(RetryError::NotRetriable("the previous attempt is still shutting down; try again shortly"));
            }

            let partial = Self::url_partial_path(&self.data_dir, &task.id, &url);
            let kept = std::fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);

            task.status = DownloadStatus::Queued;
            task.error = None;
            task.resumable = false;
            task.bytes_downloaded = kept;
            task.resumed_from = None;
            task.content_type = None;
            Self::recompute_progress(task);
            task.updated_at = chrono::Local::now().to_rfc3339();
            let retried = task.clone();

            flags.insert(task_id.to_string(), Arc::clone(&cancel_flag));
            (url, retried, tasks.clone())
        };

        if let Err(error) = self.persist_tasks(&snapshot) {
            // Roll back so the task does not look queued without a worker.
            let mut flags = self.cancel_flags.write().await;
            let mut tasks = self.tasks.write().await;
            flags.remove(task_id);
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = DownloadStatus::Failed;
                task.resumable = retried.bytes_downloaded > 0;
                task.error = Some("retry could not be saved; try again".to_string());
            }
            return Err(RetryError::Persist(error));
        }

        info!(
            "Retrying download task {} (resuming from {} byte(s))",
            task_id,
            retried.bytes_downloaded
        );

        let runtime = self.runtime().await;
        let task_id_for_worker = task_id.to_string();
        tokio::spawn(async move {
            Self::run_url_download(runtime, task_id_for_worker, url, cancel_flag).await;
        });

        Ok(retried)
    }

    /// Stop all workers and persist the latest task state.
    ///
    /// URL workers stop at the next chunk (or immediately while waiting on a
    /// stalled connection or a retry backoff), keep their partial file and
    /// record themselves as failed + resumable. Waits up to `grace` for
    /// workers to finish, then records the on-disk size of every partial
    /// that still belongs to an unfinished task.
    pub async fn shutdown(&self, grace: Duration) {
        self.shutdown.requested.store(true, Ordering::SeqCst);
        self.shutdown.notify.notify_waiters();

        let deadline = tokio::time::Instant::now() + grace;
        loop {
            if self.cancel_flags.read().await.is_empty() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                warn!("Some download workers did not stop within {:?}; persisting their last known state", grace);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let snapshot = {
            let mut tasks = self.tasks.write().await;
            for task in tasks.values_mut() {
                if Self::is_terminal(task.status) && !task.resumable {
                    continue;
                }
                if let DownloadSource::Url { url } = &task.source {
                    let partial = Self::url_partial_path(&self.data_dir, &task.id, url);
                    if let Ok(meta) = std::fs::metadata(&partial) {
                        task.bytes_downloaded = meta.len();
                        Self::recompute_progress(task);
                    }
                }
            }
            tasks.clone()
        };

        match self.persist_tasks(&snapshot) {
            Ok(()) => info!("Persisted {} download task(s) for shutdown", snapshot.len()),
            Err(error) => error!("Failed to persist download tasks during shutdown: {}", error),
        }
    }

    async fn runtime(&self) -> DownloadRuntime {
        DownloadRuntime {
            tasks: self.tasks.clone(),
            cancel_flags: self.cancel_flags.clone(),
            persistence_path: self.persistence_path.clone(),
            data_dir: self.data_dir.clone(),
            client: {
                let guard = self.client.read().await;
                guard.clone()
            },
            shutdown: Arc::clone(&self.shutdown),
        }
    }

    /// Get task by ID
    pub async fn get_task(&self, task_id: &str) -> Option<DownloadTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Update task status
    pub async fn update_status(&self, task_id: &str, status: DownloadStatus) -> anyhow::Result<()> {
        let snapshot = {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = status;
                task.updated_at = chrono::Local::now().to_rfc3339();
            }
            tasks.clone()
        };

        self.persist_tasks(&snapshot)?;
        Ok(())
    }

    /// Cancel a download task if it exists.
    ///
    /// A running worker notices the flag at its next checkpoint and deletes
    /// its partial file.
    pub async fn cancel_task(&self, task_id: &str) -> anyhow::Result<bool> {
        let flag = {
            let flags = self.cancel_flags.read().await;
            flags.get(task_id).cloned()
        };

        if let Some(cancel_flag) = flag {
            cancel_flag.store(true, Ordering::Relaxed);
        }

        let snapshot = {
            let mut tasks = self.tasks.write().await;
            let Some(task) = tasks.get_mut(task_id) else {
                return Ok(false);
            };

            if !Self::is_terminal(task.status) {
                task.status = DownloadStatus::Cancelled;
                task.error = Some(CANCELLED_BY_USER_MESSAGE.to_string());
                task.resumable = false;
                task.updated_at = chrono::Local::now().to_rfc3339();
            }

            tasks.clone()
        };

        self.persist_tasks(&snapshot)?;
        Ok(true)
    }

    /// Dismiss a download task: cancels it if still active, then removes it from the list.
    ///
    /// Returns `Ok(true)` if the task was found and removed, `Ok(false)` if no task with the
    /// given ID exists. If the task is still in progress its cancel flag is set before removal,
    /// signalling the worker to stop at the next checkpoint (the worker then deletes its own
    /// partial file). For an idle URL task, a kept resumable partial is deleted here.
    ///
    /// Inbox file cleanup for `LocalFile` tasks is handled by the worker itself while it still
    /// holds ownership of the source file; `dismiss_task` does not attempt any file deletion
    /// for them because the path stored in the task is a shared inbox name that may already
    /// have been claimed by a newer upload.
    pub async fn dismiss_task(&self, task_id: &str) -> anyhow::Result<bool> {
        // Signal cancellation if the task is still running
        let flag = {
            let flags = self.cancel_flags.read().await;
            flags.get(task_id).cloned()
        };
        let worker_running = flag.is_some();
        if let Some(cancel_flag) = flag {
            cancel_flag.store(true, Ordering::Relaxed);
        }

        // Remove the task from the map
        let (removed, snapshot) = {
            let mut tasks = self.tasks.write().await;
            let Some(removed) = tasks.remove(task_id) else {
                return Ok(false);
            };
            (removed, tasks.clone())
        };

        // Persist first; only clean up the cancel flag on success
        self.persist_tasks(&snapshot)?;

        {
            let mut flags = self.cancel_flags.write().await;
            flags.remove(task_id);
        }

        if !worker_running {
            if let DownloadSource::Url { url } = &removed.source {
                let partial = Self::url_partial_path(&self.data_dir, &removed.id, url);
                match tokio::fs::remove_file(&partial).await {
                    Ok(()) => info!("Removed partial file of dismissed task {}", removed.id),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => warn!("Failed to remove partial file {}: {}", partial.display(), error),
                }
            }
        }

        Ok(true)
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Vec<DownloadTask> {
        let tasks = self.tasks.read().await;
        let mut values: Vec<_> = tasks.values().cloned().collect();
        values.sort_by(|left, right| {
            let left_updated = chrono::DateTime::parse_from_rfc3339(&left.updated_at).ok();
            let right_updated = chrono::DateTime::parse_from_rfc3339(&right.updated_at).ok();
            let left_created = chrono::DateTime::parse_from_rfc3339(&left.created_at).ok();
            let right_created = chrono::DateTime::parse_from_rfc3339(&right.created_at).ok();

            right_updated
                .cmp(&left_updated)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right_created.cmp(&left_created))
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| right.id.cmp(&left.id))
        });
        values
    }

    fn is_terminal(status: DownloadStatus) -> bool {
        matches!(
            status,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        )
    }

    fn recompute_progress(task: &mut DownloadTask) {
        let denominator = task.total_bytes.unwrap_or(task.bytes_downloaded).max(1);
        task.progress = ((task.bytes_downloaded as f32 / denominator as f32) * 100.0).clamp(0.0, 100.0);
    }

    /// Partial file of a URL task: `inbox/<task_id>-<filename>.part`.
    fn url_partial_path(data_dir: &Path, task_id: &str, url: &str) -> PathBuf {
        let filename = Self::filename_from_url(url, task_id);
        data_dir.join("inbox").join(format!("{}-{}.part", task_id, filename))
    }

    /// Partial files that must survive the stale temp-file cleanup because a
    /// known failed URL task can still resume from them.
    fn resumable_partials(tasks: &HashMap<String, DownloadTask>, data_dir: &Path) -> HashSet<PathBuf> {
        tasks
            .values()
            .filter(|task| task.resumable || !Self::is_terminal(task.status))
            .filter_map(|task| match &task.source {
                DownloadSource::Url { url } => Some(Self::url_partial_path(data_dir, &task.id, url)),
                DownloadSource::LocalFile { .. } => None,
            })
            .collect()
    }

    async fn run_url_download(runtime: DownloadRuntime, task_id: String, url: String, cancel: Arc<AtomicBool>) {
        if let Err(error) = Self::set_task_state(&runtime, &task_id, DownloadStatus::Downloading, None, None, None).await {
            error!("Failed to mark task {} as downloading: {}", task_id, error);
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let inbox_path = runtime.data_dir.join("inbox");
        if let Err(error) = tokio::fs::create_dir_all(&inbox_path).await {
            let message = format!("failed to create inbox directory: {error}");
            let _ = Self::set_task_failed(&runtime, &task_id, message, None, false).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let filename = Self::filename_from_url(&url, &task_id);
        // Prefix with the task id so two concurrent downloads of the same
        // filename never write into the same partial file.
        let temp_path = Self::url_partial_path(&runtime.data_dir, &task_id, &url);
        let final_inbox_path = inbox_path.join(&filename);

        let (downloaded, total_bytes) =
            match Self::fetch_url_to_partial(&runtime, &task_id, &url, &temp_path, &cancel).await {
                FetchOutcome::Complete { bytes, total } => (bytes, total),
                FetchOutcome::Cancelled => {
                    Self::finish_cancelled(&runtime, &task_id, &temp_path).await;
                    return;
                }
                FetchOutcome::Shutdown => {
                    let kept = Self::partial_len(&temp_path).await;
                    let _ = Self::set_task_failed(
                        &runtime,
                        &task_id,
                        INTERRUPTED_BY_SHUTDOWN_MESSAGE.to_string(),
                        Some(kept),
                        kept > 0,
                    )
                    .await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
                FetchOutcome::Failed { message, keep_partial } => {
                    let kept = if keep_partial { Self::partial_len(&temp_path).await } else { 0 };
                    if kept == 0 {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                    }
                    let _ = Self::set_task_failed(&runtime, &task_id, message, Some(kept), kept > 0).await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            };

        let expected_sha256 = {
            let tasks = runtime.tasks.read().await;
            tasks.get(&task_id).and_then(|task| task.sha256.clone())
        };

        if let Some(expected) = expected_sha256 {
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Verifying,
                Some(downloaded),
                total_bytes,
                None,
            )
            .await;

            match Self::sha256_file(&temp_path).await {
                Ok(actual) if actual == expected => {
                    info!("Download task {} passed SHA-256 verification", task_id);
                }
                Ok(actual) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    let message = format!(
                        "checksum mismatch: expected SHA-256 {expected}, got {actual}; the downloaded file was removed"
                    );
                    let _ = Self::set_task_failed(&runtime, &task_id, message, Some(0), false).await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
                Err(error) => {
                    // The data may be fine; keep it so a retry re-verifies it.
                    let message = format!("failed to verify downloaded file: {error}");
                    let _ = Self::set_task_failed(&runtime, &task_id, message, Some(downloaded), true).await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            }
        }

        if cancel.load(Ordering::Relaxed) {
            Self::finish_cancelled(&runtime, &task_id, &temp_path).await;
            return;
        }

        if let Err(error) = tokio::fs::rename(&temp_path, &final_inbox_path).await {
            let message = format!("failed to finalize downloaded file: {error}");
            let _ = Self::set_task_failed(&runtime, &task_id, message, Some(downloaded), true).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let _ = Self::set_task_state(
            &runtime,
            &task_id,
            DownloadStatus::Routing,
            Some(downloaded),
            total_bytes,
            None,
        )
        .await;

        let destination = ContentRouter::route_file(&final_inbox_path, &runtime.data_dir)
            .unwrap_or_else(|| final_inbox_path.clone());

        if destination != final_inbox_path {
            if let Some(parent) = destination.parent() {
                if let Err(error) = tokio::fs::create_dir_all(parent).await {
                    let message = format!("failed to create destination directory: {error}");
                    let _ = Self::set_task_state(
                        &runtime,
                        &task_id,
                        DownloadStatus::Failed,
                        Some(downloaded),
                        total_bytes,
                        Some(message),
                    )
                    .await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            }

            if let Err(error) = Self::move_file(&final_inbox_path, &destination).await {
                let message = format!("failed to route file to destination: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(downloaded),
                    total_bytes,
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        }

        let content_type = destination
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(ContentType::from_extension);

        if let Err(error) = Self::set_task_state(
            &runtime,
            &task_id,
            DownloadStatus::Completed,
            Some(downloaded),
            total_bytes,
            None,
        )
        .await
        {
            error!("Failed to mark task {} completed: {}", task_id, error);
        }

        if let Err(error) = Self::set_task_content_type(&runtime, &task_id, content_type).await {
            error!("Failed to set task content type for {}: {}", task_id, error);
        }

        Self::clear_cancel_flag(&runtime, &task_id).await;
    }

    async fn finish_cancelled(runtime: &DownloadRuntime, task_id: &str, temp_path: &Path) {
        let _ = Self::set_task_state(
            runtime,
            task_id,
            DownloadStatus::Cancelled,
            None,
            None,
            Some(CANCELLED_BY_USER_MESSAGE.to_string()),
        )
        .await;
        let _ = tokio::fs::remove_file(temp_path).await;
        Self::clear_cancel_flag(runtime, task_id).await;
    }

    async fn partial_len(path: &Path) -> u64 {
        tokio::fs::metadata(path).await.map(|meta| meta.len()).unwrap_or(0)
    }

    /// Download `url` into `temp_path`, resuming any bytes already there.
    ///
    /// Each attempt sends `Range: bytes=<len>-` when the partial file is not
    /// empty, guarded by `If-Range` with the validator recorded from the
    /// response that started the file. `206` appends, `200` truncates and
    /// starts over, and `416` accepts the partial only when its size equals
    /// the known total. Network and stream errors are retried with backoff;
    /// an attempt that grows the partial file resets the failure counter.
    async fn fetch_url_to_partial(
        runtime: &DownloadRuntime,
        task_id: &str,
        url: &str,
        temp_path: &Path,
        cancel: &AtomicBool,
    ) -> FetchOutcome {
        let (mut etag, mut last_modified, mut known_total) = {
            let tasks = runtime.tasks.read().await;
            tasks
                .get(task_id)
                .map(|task| (task.etag.clone(), task.last_modified.clone(), task.total_bytes))
                .unwrap_or_default()
        };

        let mut failures: u8 = 0;
        // Largest partial size seen in this run. Only growth past it counts as
        // progress (and resets `failures`); it is never lowered when a
        // response forces a restart from zero, so a server that ignores Range
        // and drops at the same offset every time still runs out of attempts.
        let mut high_water = Self::partial_len(temp_path).await;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return FetchOutcome::Cancelled;
            }
            if runtime.shutdown.is_requested() {
                return FetchOutcome::Shutdown;
            }

            let existing = Self::partial_len(temp_path).await;
            let mut request = runtime.client.get(url);
            if existing > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
                if let Some(validator) = Self::if_range_validator(etag.as_deref(), last_modified.as_deref()) {
                    request = request.header(reqwest::header::IF_RANGE, validator);
                }
            }

            let attempt_error = match request.send().await {
                Err(error) => format!("request failed: {error}"),
                Ok(response) => {
                    let status = response.status();
                    let response_etag = Self::header_string(&response, reqwest::header::ETAG);

                    if status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0 {
                        let content_range = Self::parse_content_range(&response);
                        let etag_changed = matches!((&etag, &response_etag), (Some(old), Some(new)) if old != new);
                        match content_range {
                            Some((start, total)) if start == existing && !etag_changed => {
                                known_total = total
                                    .or_else(|| response.content_length().map(|len| existing + len))
                                    .or(known_total);
                                info!("Download task {} resuming at byte {}", task_id, existing);
                                match Self::open_partial(temp_path, true).await {
                                    Ok(file) => {
                                        let end = Self::stream_body(
                                            runtime, task_id, response, file, existing, known_total, Some(existing), cancel,
                                        )
                                        .await;
                                        match Self::classify_stream_end(end, known_total, temp_path).await {
                                            Ok(outcome) => return outcome,
                                            Err(message) => message,
                                        }
                                    }
                                    Err(error) => {
                                        return FetchOutcome::Failed {
                                            message: format!("failed to open partial file: {error}"),
                                            keep_partial: true,
                                        };
                                    }
                                }
                            }
                            _ => {
                                // Unusable range response: start over from byte zero.
                                warn!(
                                    "Download task {} got an unexpected partial response (range {:?}, etag changed: {}); restarting",
                                    task_id, content_range, etag_changed
                                );
                                if let Err(error) = Self::open_partial(temp_path, false).await {
                                    return FetchOutcome::Failed {
                                        message: format!("failed to reset partial file: {error}"),
                                        keep_partial: false,
                                    };
                                }
                                etag = None;
                                last_modified = None;
                                "server returned an unusable partial response".to_string()
                            }
                        }
                    } else if status.is_success() {
                        // Full body (also when the server ignored Range or the
                        // If-Range validator no longer matches): start over.
                        etag = response_etag;
                        last_modified = Self::header_string(&response, reqwest::header::LAST_MODIFIED);
                        known_total = response.content_length();
                        if existing > 0 {
                            info!(
                                "Download task {} restarting from byte 0 (server sent HTTP {} to a range request)",
                                task_id, status
                            );
                        }
                        let validators = (etag.clone(), last_modified.clone());
                        let _ = Self::update_task(runtime, task_id, move |task| {
                            task.etag = validators.0;
                            task.last_modified = validators.1;
                            task.resumed_from = None;
                        })
                        .await;

                        match Self::open_partial(temp_path, false).await {
                            Ok(file) => {
                                let end = Self::stream_body(
                                    runtime, task_id, response, file, 0, known_total, None, cancel,
                                )
                                .await;
                                match Self::classify_stream_end(end, known_total, temp_path).await {
                                    Ok(outcome) => return outcome,
                                    Err(message) => message,
                                }
                            }
                            Err(error) => {
                                return FetchOutcome::Failed {
                                    message: format!("failed to create temporary file: {error}"),
                                    keep_partial: false,
                                };
                            }
                        }
                    } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && existing > 0 {
                        let total = Self::parse_unsatisfied_range_total(&response).or(known_total);
                        if total == Some(existing) {
                            info!("Download task {} partial file is already complete ({} bytes)", task_id, existing);
                            return FetchOutcome::Complete { bytes: existing, total };
                        }
                        warn!(
                            "Download task {} partial ({} bytes) does not match remote size {:?}; restarting",
                            task_id, existing, total
                        );
                        if let Err(error) = Self::open_partial(temp_path, false).await {
                            return FetchOutcome::Failed {
                                message: format!("failed to reset partial file: {error}"),
                                keep_partial: false,
                            };
                        }
                        etag = None;
                        last_modified = None;
                        "server rejected the resume range".to_string()
                    } else if Self::is_retriable_http_status(status) {
                        format!("server returned HTTP {status}")
                    } else {
                        return FetchOutcome::Failed {
                            message: format!("server returned HTTP {status}"),
                            keep_partial: true,
                        };
                    }
                }
            };

            if cancel.load(Ordering::Relaxed) {
                return FetchOutcome::Cancelled;
            }

            let now_len = Self::partial_len(temp_path).await;
            if now_len > high_water {
                high_water = now_len;
                failures = 0;
            }
            failures += 1;

            if failures >= MAX_DOWNLOAD_ATTEMPTS {
                return FetchOutcome::Failed {
                    message: attempt_error,
                    keep_partial: true,
                };
            }

            warn!(
                "Download task {} attempt failed ({}/{}): {}. {} byte(s) kept; retrying",
                task_id, failures, MAX_DOWNLOAD_ATTEMPTS, attempt_error, now_len
            );

            let backoff = Duration::from_millis(RETRY_BACKOFF_MS * failures as u64);
            let notified = runtime.shutdown.notify.notified();
            if runtime.shutdown.is_requested() {
                return FetchOutcome::Shutdown;
            }
            tokio::select! {
                _ = notified => return FetchOutcome::Shutdown,
                _ = tokio::time::sleep(backoff) => {}
            }
        }
    }

    /// Turn a finished stream into a fetch outcome, or `Err(message)` when the
    /// attempt should be retried.
    async fn classify_stream_end(end: StreamEnd, total: Option<u64>, temp_path: &Path) -> Result<FetchOutcome, String> {
        match end {
            StreamEnd::Finished(bytes) => match total {
                Some(expected) if bytes < expected => Err(format!(
                    "connection closed early ({bytes} of {expected} bytes received)"
                )),
                Some(expected) if bytes > expected => {
                    // More data than announced: the partial cannot be trusted.
                    let _ = tokio::fs::remove_file(temp_path).await;
                    Err(format!("server sent {bytes} bytes but announced {expected}"))
                }
                _ => Ok(FetchOutcome::Complete { bytes, total }),
            },
            StreamEnd::Cancelled => Ok(FetchOutcome::Cancelled),
            StreamEnd::Shutdown => Ok(FetchOutcome::Shutdown),
            StreamEnd::Network(message) => Err(message),
            StreamEnd::Write(message) => Ok(FetchOutcome::Failed { message, keep_partial: false }),
        }
    }

    async fn open_partial(path: &Path, append: bool) -> std::io::Result<tokio::fs::File> {
        if append {
            tokio::fs::OpenOptions::new().append(true).open(path).await
        } else {
            tokio::fs::File::create(path).await
        }
    }

    /// Copy a response body to `file`, starting the byte count at `start`.
    #[allow(clippy::too_many_arguments)]
    async fn stream_body(
        runtime: &DownloadRuntime,
        task_id: &str,
        response: reqwest::Response,
        mut file: tokio::fs::File,
        start: u64,
        total: Option<u64>,
        resumed_from: Option<u64>,
        cancel: &AtomicBool,
    ) -> StreamEnd {
        let _ = Self::set_task_state(runtime, task_id, DownloadStatus::Downloading, Some(start), total, None).await;
        let _ = Self::update_task(runtime, task_id, move |task| task.resumed_from = resumed_from).await;

        let mut downloaded = start;
        let mut last_progress_sync = start;
        let mut stream = response.bytes_stream();

        let end = loop {
            let notified = runtime.shutdown.notify.notified();
            if runtime.shutdown.is_requested() {
                break StreamEnd::Shutdown;
            }

            let next = tokio::select! {
                _ = notified => None,
                item = stream.next() => Some(item),
            };
            let Some(item) = next else {
                break StreamEnd::Shutdown;
            };

            let chunk = match item {
                None => break StreamEnd::Finished(downloaded),
                Some(Err(error)) => break StreamEnd::Network(format!("download stream failed: {error}")),
                Some(Ok(chunk)) => chunk,
            };

            if cancel.load(Ordering::Relaxed) {
                break StreamEnd::Cancelled;
            }

            if let Err(error) = file.write_all(&chunk).await {
                break StreamEnd::Write(format!("failed to write downloaded data: {error}"));
            }

            downloaded += chunk.len() as u64;
            if downloaded.saturating_sub(last_progress_sync) >= PROGRESS_SYNC_BYTES {
                let _ = Self::set_task_state(
                    runtime,
                    task_id,
                    DownloadStatus::Downloading,
                    Some(downloaded),
                    total,
                    None,
                )
                .await;
                last_progress_sync = downloaded;
            }
        };

        // Flush on every exit so the on-disk length matches what was received
        // and the next attempt resumes at the right offset.
        if let Err(error) = file.flush().await {
            return StreamEnd::Write(format!("failed to flush downloaded file: {error}"));
        }
        if let Err(error) = file.sync_data().await {
            warn!("Failed to sync partial file of task {}: {}", task_id, error);
        }
        drop(file);

        if !matches!(end, StreamEnd::Finished(_) | StreamEnd::Cancelled) {
            let _ = Self::set_task_state(
                runtime,
                task_id,
                DownloadStatus::Downloading,
                Some(downloaded),
                total,
                None,
            )
            .await;
        }
        end
    }

    /// `If-Range` requires a strong validator: use the ETag unless it is
    /// weak (`W/"..."`), else fall back to `Last-Modified`.
    fn if_range_validator<'a>(etag: Option<&'a str>, last_modified: Option<&'a str>) -> Option<&'a str> {
        match etag {
            Some(tag) if !tag.starts_with("W/") => Some(tag),
            _ => last_modified,
        }
    }

    fn header_string(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    /// Parse `Content-Range: bytes <start>-<end>/<total|*>` into `(start, total)`.
    fn parse_content_range(response: &reqwest::Response) -> Option<(u64, Option<u64>)> {
        let value = Self::header_string(response, reqwest::header::CONTENT_RANGE)?;
        Self::parse_content_range_value(&value)
    }

    fn parse_content_range_value(value: &str) -> Option<(u64, Option<u64>)> {
        let rest = value.trim().strip_prefix("bytes")?.trim_start();
        let (range, total) = rest.split_once('/')?;
        let (start, _end) = range.trim().split_once('-')?;
        let start = start.trim().parse().ok()?;
        let total = match total.trim() {
            "*" => None,
            other => Some(other.parse().ok()?),
        };
        Some((start, total))
    }

    /// Parse the total from a `416` response's `Content-Range: bytes */<total>`.
    fn parse_unsatisfied_range_total(response: &reqwest::Response) -> Option<u64> {
        let value = Self::header_string(response, reqwest::header::CONTENT_RANGE)?;
        value.trim().strip_prefix("bytes")?.trim().strip_prefix("*/")?.trim().parse().ok()
    }

    /// Stream a file through SHA-256 on the blocking pool; returns lowercase hex.
    async fn sha256_file(path: &Path) -> std::io::Result<String> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let mut file = std::fs::File::open(&path)?;
            let mut context = ring::digest::Context::new(&ring::digest::SHA256);
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                context.update(&buffer[..read]);
            }
            Ok(hex::encode(context.finish().as_ref()))
        })
        .await
        .map_err(std::io::Error::other)?
    }

    async fn set_task_failed(
        runtime: &DownloadRuntime,
        task_id: &str,
        message: String,
        bytes_downloaded: Option<u64>,
        resumable: bool,
    ) -> anyhow::Result<()> {
        Self::update_task(runtime, task_id, move |task| {
            if task.status == DownloadStatus::Cancelled {
                return;
            }
            if task.status != DownloadStatus::Failed {
                info!(
                    "Download task {} status transition: {:?} -> Failed ({})",
                    task.id, task.status, message
                );
            }
            task.status = DownloadStatus::Failed;
            if let Some(bytes) = bytes_downloaded {
                task.bytes_downloaded = bytes;
            }
            Self::recompute_progress(task);
            task.error = Some(message);
            task.resumable = resumable;
        })
        .await
    }

    async fn run_local_import(runtime: DownloadRuntime, task_id: String, source_path: PathBuf) {
        if let Err(error) = Self::set_task_state(&runtime, &task_id, DownloadStatus::Validating, None, None, None).await {
            error!("Failed to mark local import task {} as validating: {}", task_id, error);
            return;
        }

        if Self::is_cancelled(&runtime, &task_id).await {
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Cancelled,
                None,
                None,
                Some("download cancelled by user".to_string()),
            )
            .await;
            let _ = tokio::fs::remove_file(&source_path).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let metadata = match tokio::fs::metadata(&source_path).await {
            Ok(metadata) => metadata,
            Err(error) => {
                let message = format!("local file unavailable: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(0),
                    Some(0),
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        };

        if !metadata.is_file() {
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(0),
                Some(0),
                Some("local import source must be a file".to_string()),
            )
            .await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let total_bytes = metadata.len();

        if let Err(error) = Self::set_task_state(
            &runtime,
            &task_id,
            DownloadStatus::Downloading,
            Some(0),
            Some(total_bytes),
            None,
        )
        .await
        {
            error!("Failed to mark local import task {} as downloading: {}", task_id, error);
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let source_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{task_id}.bin"));

        let inbox_path = runtime.data_dir.join("inbox");
        if let Err(error) = tokio::fs::create_dir_all(&inbox_path).await {
            let message = format!("failed to create inbox directory: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(0),
                Some(total_bytes),
                Some(message),
            )
            .await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let temp_path = inbox_path.join(format!("{}-{}.part", task_id, source_name));
        let final_inbox_path = inbox_path.join(&source_name);

        let mut source_file = match tokio::fs::File::open(&source_path).await {
            Ok(file) => file,
            Err(error) => {
                let message = format!("failed to open local source file: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(0),
                    Some(total_bytes),
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        };

        let mut target_file = match tokio::fs::File::create(&temp_path).await {
            Ok(file) => file,
            Err(error) => {
                let message = format!("failed to create temporary import file: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(0),
                    Some(total_bytes),
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        };

        let mut copied = 0u64;
        let mut last_progress_sync = 0u64;
        let mut buffer = [0u8; 8192];

        loop {
            if runtime.shutdown.is_requested() {
                // Keep the staged source so the user can import it again.
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(copied),
                    Some(total_bytes),
                    Some("interrupted by server shutdown; please retry".to_string()),
                )
                .await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            if Self::is_cancelled(&runtime, &task_id).await {
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Cancelled,
                    Some(copied),
                    Some(total_bytes),
                    Some("download cancelled by user".to_string()),
                )
                .await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                let _ = tokio::fs::remove_file(&source_path).await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            let bytes_read = match tokio::io::AsyncReadExt::read(&mut source_file, &mut buffer).await {
                Ok(read) => read,
                Err(error) => {
                    let message = format!("failed to read local source file: {error}");
                    let _ = Self::set_task_state(
                        &runtime,
                        &task_id,
                        DownloadStatus::Failed,
                        Some(copied),
                        Some(total_bytes),
                        Some(message),
                    )
                    .await;
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            };

            if bytes_read == 0 {
                break;
            }

            if let Err(error) = target_file.write_all(&buffer[..bytes_read]).await {
                let message = format!("failed to write imported data: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(copied),
                    Some(total_bytes),
                    Some(message),
                )
                .await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                let _ = tokio::fs::remove_file(&source_path).await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            copied += bytes_read as u64;
            if copied.saturating_sub(last_progress_sync) >= 1024 * 1024 {
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Downloading,
                    Some(copied),
                    Some(total_bytes),
                    None,
                )
                .await;
                last_progress_sync = copied;
            }
        }

        if let Err(error) = target_file.flush().await {
            let message = format!("failed to flush imported file: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(copied),
                Some(total_bytes),
                Some(message),
            )
            .await;
            let _ = tokio::fs::remove_file(&temp_path).await;
            let _ = tokio::fs::remove_file(&source_path).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        if let Err(error) = tokio::fs::rename(&temp_path, &final_inbox_path).await {
            let message = format!("failed to finalize imported file: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(copied),
                Some(total_bytes),
                Some(message),
            )
            .await;
            let _ = tokio::fs::remove_file(&temp_path).await;
            let _ = tokio::fs::remove_file(&source_path).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let _ = Self::set_task_state(
            &runtime,
            &task_id,
            DownloadStatus::Routing,
            Some(copied),
            Some(total_bytes),
            None,
        )
        .await;

        let destination = ContentRouter::route_file(&final_inbox_path, &runtime.data_dir)
            .unwrap_or_else(|| final_inbox_path.clone());

        if destination != final_inbox_path {
            if let Some(parent) = destination.parent() {
                if let Err(error) = tokio::fs::create_dir_all(parent).await {
                    let message = format!("failed to create destination directory: {error}");
                    let _ = Self::set_task_state(
                        &runtime,
                        &task_id,
                        DownloadStatus::Failed,
                        Some(copied),
                        Some(total_bytes),
                        Some(message),
                    )
                    .await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            }

            if let Err(error) = Self::move_file(&final_inbox_path, &destination).await {
                let message = format!("failed to route file to destination: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(copied),
                    Some(total_bytes),
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        }

        let content_type = destination
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(ContentType::from_extension);

        if let Err(error) = Self::set_task_state(
            &runtime,
            &task_id,
            DownloadStatus::Completed,
            Some(copied),
            Some(total_bytes),
            None,
        )
        .await
        {
            error!("Failed to mark local import task {} completed: {}", task_id, error);
        }

        if let Err(error) = Self::set_task_content_type(&runtime, &task_id, content_type).await {
            error!("Failed to set local import task content type for {}: {}", task_id, error);
        }

        Self::clear_cancel_flag(&runtime, &task_id).await;
    }

    async fn set_task_state(
        runtime: &DownloadRuntime,
        task_id: &str,
        status: DownloadStatus,
        bytes_downloaded: Option<u64>,
        total_bytes: Option<u64>,
        error_message: Option<String>,
    ) -> anyhow::Result<()> {
        Self::update_task(runtime, task_id, |task| {
            if task.status == DownloadStatus::Cancelled && status != DownloadStatus::Cancelled {
                return;
            }

            if task.status != status {
                info!(
                    "Download task {} status transition: {:?} -> {:?}",
                    task_id,
                    task.status,
                    status
                );
            }

            task.status = status;
            // Only `set_task_failed` marks a task resumable.
            task.resumable = false;

            if let Some(bytes) = bytes_downloaded {
                task.bytes_downloaded = bytes;
            }

            if total_bytes.is_some() {
                task.total_bytes = total_bytes;
            }

            Self::recompute_progress(task);
            task.error = error_message;
        })
        .await
    }

    async fn set_task_content_type(
        runtime: &DownloadRuntime,
        task_id: &str,
        content_type: Option<ContentType>,
    ) -> anyhow::Result<()> {
        Self::update_task(runtime, task_id, |task| {
            task.content_type = content_type;
        })
        .await
    }

    async fn update_task<F>(runtime: &DownloadRuntime, task_id: &str, updater: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut DownloadTask),
    {
        let snapshot = {
            let mut tasks = runtime.tasks.write().await;
            if let Some(task) = tasks.get_mut(task_id) {
                updater(task);
                task.updated_at = chrono::Local::now().to_rfc3339();
            }
            tasks.clone()
        };

        Self::persist_tasks_at(&runtime.persistence_path, &snapshot)
    }

    async fn is_cancelled(runtime: &DownloadRuntime, task_id: &str) -> bool {
        let flags = runtime.cancel_flags.read().await;
        flags
            .get(task_id)
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    async fn clear_cancel_flag(runtime: &DownloadRuntime, task_id: &str) {
        let mut flags = runtime.cancel_flags.write().await;
        flags.remove(task_id);
    }

    fn is_retriable_http_status(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
    }

    fn build_http_client(timeout_secs: u64) -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|error| {
                warn!("Failed to build HTTP client with custom timeouts: {}", error);
                reqwest::Client::new()
            })
    }

    fn load_tasks(path: &Path) -> anyhow::Result<HashMap<String, DownloadTask>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let raw = std::fs::read_to_string(path)?;
        let tasks: HashMap<String, DownloadTask> = serde_json::from_str(&raw)?;

        info!("Loaded {} download tasks", tasks.len());
        Ok(tasks)
    }

    fn persist_tasks(&self, tasks: &HashMap<String, DownloadTask>) -> anyhow::Result<()> {
        Self::persist_tasks_at(&self.persistence_path, tasks)
    }

    fn persist_tasks_at(path: &Path, tasks: &HashMap<String, DownloadTask>) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(tasks)?;
        std::fs::write(&tmp_path, json)?;

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }

        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Mark tasks that were still in flight when the server stopped as failed.
    ///
    /// Workers only live as long as the process, so a persisted task in a
    /// non-terminal state after startup has no worker and would otherwise stay
    /// "downloading" forever. URL tasks keep their partial file and become
    /// resumable (`POST /api/download/:task_id/retry`); local-import partials
    /// are removed. Returns the number of recovered tasks.
    fn recover_interrupted_tasks(tasks: &mut HashMap<String, DownloadTask>, data_dir: &Path) -> usize {
        let inbox = data_dir.join("inbox");
        let now = chrono::Local::now().to_rfc3339();
        let mut recovered = 0;

        for task in tasks.values_mut() {
            if Self::is_terminal(task.status) {
                continue;
            }

            match &task.source {
                DownloadSource::Url { url } => {
                    let filename = Self::filename_from_url(url, &task.id);
                    // Older releases used `<filename>.part` without the task id;
                    // those cannot be attributed safely, so drop them.
                    let legacy = inbox.join(format!("{}.part", filename));
                    if legacy.exists() {
                        let _ = std::fs::remove_file(&legacy);
                    }
                    let partial = Self::url_partial_path(data_dir, &task.id, url);
                    let kept = std::fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
                    task.bytes_downloaded = kept;
                    task.resumable = kept > 0;
                    if kept > 0 {
                        info!(
                            "Kept {} byte(s) of interrupted task {} for resume ({})",
                            kept,
                            task.id,
                            partial.display()
                        );
                    }
                }
                DownloadSource::LocalFile { path } => {
                    let source_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("{}.bin", task.id));
                    let temp_path = inbox.join(format!("{}-{}.part", task.id, source_name));
                    if temp_path.exists() {
                        match std::fs::remove_file(&temp_path) {
                            Ok(_) => info!("Removed partial file {} of interrupted task {}", temp_path.display(), task.id),
                            Err(error) => warn!("Failed to remove partial file {}: {}", temp_path.display(), error),
                        }
                    }
                    task.bytes_downloaded = 0;
                    task.resumable = false;
                }
            }

            warn!(
                "Download task {} was interrupted in state {:?} by a server restart; marking as failed",
                task.id,
                task.status
            );
            task.error = Some(match task.source {
                DownloadSource::Url { .. } => INTERRUPTED_BY_RESTART_MESSAGE.to_string(),
                DownloadSource::LocalFile { .. } => "interrupted by server restart; please retry".to_string(),
            });
            task.status = DownloadStatus::Failed;
            Self::recompute_progress(task);
            task.updated_at = now.clone();
            recovered += 1;
        }

        recovered
    }

    /// Move `source` to `destination`, falling back to copy + delete when a
    /// plain rename fails (for example when `maps/` or `books/` is a separate
    /// Docker volume or mount point, which makes rename return EXDEV).
    async fn move_file(source: &Path, destination: &Path) -> std::io::Result<()> {
        match tokio::fs::rename(source, destination).await {
            Ok(()) => Ok(()),
            Err(rename_error) => {
                let copy_target = destination.with_extension(format!(
                    "{}.fyr-move",
                    destination.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                ));
                if let Err(copy_error) = tokio::fs::copy(source, &copy_target).await {
                    let _ = tokio::fs::remove_file(&copy_target).await;
                    warn!(
                        "Moving {} to {} failed (rename: {}; copy: {})",
                        source.display(),
                        destination.display(),
                        rename_error,
                        copy_error
                    );
                    return Err(copy_error);
                }
                if let Err(error) = tokio::fs::rename(&copy_target, destination).await {
                    let _ = tokio::fs::remove_file(&copy_target).await;
                    return Err(error);
                }
                tokio::fs::remove_file(source).await
            }
        }
    }

    /// Remove `.part` files older than 24h, except partials in `keep` that a
    /// failed-but-resumable task can still continue from.
    fn cleanup_stale_temp_files(data_dir: &Path, keep: &HashSet<PathBuf>) {
        let inbox = data_dir.join("inbox");
        if !inbox.exists() {
            return;
        }

        let entries = match std::fs::read_dir(&inbox) {
            Ok(entries) => entries,
            Err(error) => {
                warn!("Failed to scan inbox for stale temp files: {}", error);
                return;
            }
        };

        let ttl = Duration::from_secs(24 * 60 * 60);
        let now = std::time::SystemTime::now();

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("part") {
                continue;
            }
            if keep.contains(&path) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    warn!("Failed to read metadata for {}: {}", path.display(), error);
                    continue;
                }
            };

            let age = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .unwrap_or_default();

            if age < ttl {
                continue;
            }

            match std::fs::remove_file(&path) {
                Ok(_) => info!("Removed stale temp import file {}", path.display()),
                Err(error) => warn!("Failed to remove stale temp file {}: {}", path.display(), error),
            }
        }
    }

    fn filename_from_url(url: &str, task_id: &str) -> String {
        let default = format!("{task_id}.bin");

        let Some(segment) = reqwest::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.path_segments().and_then(|mut it| it.next_back()).map(str::to_string))
            .filter(|segment| !segment.trim().is_empty())
        else {
            return default;
        };

        // Percent-decode the segment so that filenames like "file%20name.epub"
        // become "file_name.epub" rather than "file_20name.epub".
        let decoded = Self::percent_decode_segment(&segment);

        let sanitized: String = decoded
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();

        if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
            default
        } else {
            sanitized
        }
    }

    /// Decode a percent-encoded URL path segment into a UTF-8 string (falls back to the original segment if decoding fails).
    fn percent_decode_segment(segment: &str) -> String {
        let bytes = segment.as_bytes();
        let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let (Some(h1), Some(h2)) = (
                    (bytes[i + 1] as char).to_digit(16),
                    (bytes[i + 2] as char).to_digit(16),
                ) {
                    decoded.push((h1 * 16 + h2) as u8);
                    i += 3;
                    continue;
                }
            }
            decoded.push(bytes[i]);
            i += 1;
        }

        String::from_utf8(decoded).unwrap_or_else(|_| segment.to_string())
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new("./public/data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_data_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fyr-download-tests-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[tokio::test]
    async fn cancel_marks_local_task_as_cancelled() {
        let data_dir = test_data_dir();
        let manager = DownloadManager::new(&data_dir);

        let task_id = manager
            .create_task(DownloadSource::LocalFile {
                path: data_dir.join("local.dat"),
            })
            .await;

        let cancelled = manager.cancel_task(&task_id).await.expect("cancel task");
        assert!(cancelled);

        let task = manager.get_task(&task_id).await.expect("task exists");
        assert!(matches!(
            task.status,
            DownloadStatus::Queued
                | DownloadStatus::Validating
                | DownloadStatus::Downloading
                | DownloadStatus::Cancelled
                | DownloadStatus::Failed
        ));
    }

    fn persisted_task(id: &str, source: DownloadSource, status: DownloadStatus) -> DownloadTask {
        DownloadTask {
            id: id.to_string(),
            source,
            status,
            progress: 40.0,
            bytes_downloaded: 4,
            total_bytes: Some(10),
            error: None,
            content_type: None,
            created_at: "2026-07-19T21:00:00+00:00".to_string(),
            updated_at: "2026-07-19T21:01:00+00:00".to_string(),
            sha256: None,
            etag: None,
            last_modified: None,
            resumable: false,
            resumed_from: None,
        }
    }

    #[tokio::test]
    async fn startup_marks_interrupted_tasks_failed_and_keeps_url_partials() {
        let data_dir = test_data_dir();
        let inbox = data_dir.join("inbox");
        std::fs::create_dir_all(&inbox).expect("create inbox");

        let mut persisted = HashMap::new();
        persisted.insert(
            "url-task".to_string(),
            persisted_task(
                "url-task",
                DownloadSource::Url { url: "https://example.com/atlas.pmtiles".to_string() },
                DownloadStatus::Downloading,
            ),
        );
        persisted.insert(
            "local-task".to_string(),
            persisted_task(
                "local-task",
                DownloadSource::LocalFile { path: inbox.join("novel.epub") },
                DownloadStatus::Validating,
            ),
        );
        persisted.insert(
            "done-task".to_string(),
            persisted_task(
                "done-task",
                DownloadSource::Url { url: "https://example.com/done.epub".to_string() },
                DownloadStatus::Completed,
            ),
        );
        std::fs::write(
            data_dir.join("download_tasks.json"),
            serde_json::to_vec(&persisted).expect("serialize tasks"),
        )
        .expect("write persisted tasks");
        std::fs::write(inbox.join("url-task-atlas.pmtiles.part"), b"partial").expect("write url partial");
        std::fs::write(inbox.join("atlas.pmtiles.part"), b"legacy").expect("write legacy url partial");
        std::fs::write(inbox.join("local-task-novel.epub.part"), b"partial").expect("write local partial");

        let manager = DownloadManager::new(&data_dir);

        for id in ["url-task", "local-task"] {
            let task = manager.get_task(id).await.expect("task exists");
            assert_eq!(task.status, DownloadStatus::Failed);
            assert!(task.error.as_deref().unwrap_or_default().contains("restart"));
        }
        let url_task = manager.get_task("url-task").await.expect("task exists");
        assert!(url_task.resumable);
        assert_eq!(url_task.bytes_downloaded, b"partial".len() as u64);
        assert_eq!(url_task.error.as_deref(), Some(INTERRUPTED_BY_RESTART_MESSAGE));
        assert!(!manager.get_task("local-task").await.expect("task exists").resumable);
        let done = manager.get_task("done-task").await.expect("task exists");
        assert_eq!(done.status, DownloadStatus::Completed);
        assert!(done.error.is_none());

        assert!(inbox.join("url-task-atlas.pmtiles.part").exists(), "URL partial must be kept for resume");
        assert!(!inbox.join("atlas.pmtiles.part").exists());
        assert!(!inbox.join("local-task-novel.epub.part").exists());

        // The recovery must be persisted so a second restart sees terminal states.
        let reloaded = DownloadManager::load_tasks(&data_dir.join("download_tasks.json"))
            .expect("reload tasks");
        assert_eq!(reloaded["url-task"].status, DownloadStatus::Failed);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn move_file_moves_and_replaces_destination() {
        let data_dir = test_data_dir();
        let source = data_dir.join("inbox.epub");
        let destination_dir = data_dir.join("books");
        std::fs::create_dir_all(&destination_dir).expect("create destination dir");
        let destination = destination_dir.join("inbox.epub");
        std::fs::write(&source, b"new").expect("write source");
        std::fs::write(&destination, b"old").expect("write existing destination");

        DownloadManager::move_file(&source, &destination).await.expect("move file");

        assert!(!source.exists());
        assert_eq!(std::fs::read(&destination).expect("read destination"), b"new");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn move_file_reports_missing_source() {
        let data_dir = test_data_dir();
        let result = DownloadManager::move_file(&data_dir.join("absent.epub"), &data_dir.join("x.epub")).await;
        assert!(result.is_err());
        assert!(!data_dir.join("x.epub").exists());
        assert!(!data_dir.join("x.epub.fyr-move").exists());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn retry_status_policy_matches_expected_codes() {
        assert!(DownloadManager::is_retriable_http_status(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(DownloadManager::is_retriable_http_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(DownloadManager::is_retriable_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!DownloadManager::is_retriable_http_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!DownloadManager::is_retriable_http_status(reqwest::StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn list_tasks_returns_newest_updates_first() {
        let data_dir = test_data_dir();
        let manager = DownloadManager::new(&data_dir);

        {
            let mut tasks = manager.tasks.write().await;
            tasks.insert(
                "older".to_string(),
                DownloadTask {
                    id: "older".to_string(),
                    source: DownloadSource::Url {
                        url: "https://example.com/older.epub".to_string(),
                    },
                    status: DownloadStatus::Completed,
                    progress: 100.0,
                    bytes_downloaded: 10,
                    total_bytes: Some(10),
                    error: None,
                    content_type: Some(ContentType::Book),
                    created_at: "2026-07-19T21:00:00+00:00".to_string(),
                    updated_at: "2026-07-19T21:01:00+00:00".to_string(),
                    sha256: None,
                    etag: None,
                    last_modified: None,
                    resumable: false,
                    resumed_from: None,
                },
            );
            tasks.insert(
                "newer".to_string(),
                DownloadTask {
                    id: "newer".to_string(),
                    source: DownloadSource::Url {
                        url: "https://example.com/newer.epub".to_string(),
                    },
                    status: DownloadStatus::Downloading,
                    progress: 50.0,
                    bytes_downloaded: 5,
                    total_bytes: Some(10),
                    error: None,
                    content_type: Some(ContentType::Book),
                    created_at: "2026-07-19T22:00:00+00:00".to_string(),
                    updated_at: "2026-07-19T22:05:00+00:00".to_string(),
                    sha256: None,
                    etag: None,
                    last_modified: None,
                    resumable: false,
                    resumed_from: None,
                },
            );
        }

        let tasks = manager.list_tasks().await;
        let ids: Vec<_> = tasks.into_iter().map(|task| task.id).collect();

        assert_eq!(ids, vec!["newer".to_string(), "older".to_string()]);
    }

    #[test]
    fn percent_decode_segment_decodes_space() {
        assert_eq!(
            DownloadManager::percent_decode_segment("file%20name.epub"),
            "file name.epub"
        );
    }

    #[test]
    fn percent_decode_segment_passes_through_plain() {
        assert_eq!(
            DownloadManager::percent_decode_segment("plain.epub"),
            "plain.epub"
        );
    }

    #[test]
    fn filename_from_url_decodes_percent_encoded_space() {
        let name = DownloadManager::filename_from_url(
            "https://example.com/file%20name.epub",
            "task-id",
        );
        assert_eq!(name, "file_name.epub");
    }

    #[test]
    fn filename_from_url_rejects_dot() {
        let name = DownloadManager::filename_from_url("https://example.com/.", "task-id");
        assert_eq!(name, "task-id.bin");
    }

    #[test]
    fn filename_from_url_rejects_dotdot() {
        let name = DownloadManager::filename_from_url("https://example.com/..", "task-id");
        assert_eq!(name, "task-id.bin");
    }

    #[test]
    fn filename_from_url_rejects_percent_encoded_dotdot() {
        let name =
            DownloadManager::filename_from_url("https://example.com/%2e%2e", "task-id");
        assert_eq!(name, "task-id.bin");
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    use crate::test_server::{Fault, TestServer};

    const ETAG: &str = "\"v1\"";

    fn test_data_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fyr-resume-tests-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("inbox")).expect("create temp test dir");
        dir
    }

    /// Deterministic pseudo-random payload (xorshift).
    fn payload(len: usize, seed: u8) -> Vec<u8> {
        let mut state = seed as u32 | 1;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state & 0xff) as u8
            })
            .collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
    }

    /// Manager whose HTTP client ignores proxy environment variables.
    async fn manager(data_dir: &Path) -> DownloadManager {
        let manager = DownloadManager::new(data_dir);
        *manager.client.write().await = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("client");
        manager
    }

    async fn wait_terminal(manager: &DownloadManager, task_id: &str) -> DownloadTask {
        for _ in 0..400 {
            let task = manager.get_task(task_id).await.expect("task exists");
            if DownloadManager::is_terminal(task.status) && !manager.cancel_flags.read().await.contains_key(task_id) {
                return task;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("task {task_id} did not finish: {:?}", manager.get_task(task_id).await);
    }

    async fn url_task(manager: &DownloadManager, url: String, sha256: Option<String>) -> String {
        manager
            .create_task_with_options(DownloadSource::Url { url }, DownloadOptions { sha256 })
            .await
    }

    fn inbox_parts(data_dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(data_dir.join("inbox"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("part"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Persist a task as if the previous process died mid-download.
    fn seed_interrupted_task(data_dir: &Path, id: &str, url: &str, partial: &[u8], etag: Option<&str>, total: Option<u64>) {
        let mut tasks = HashMap::new();
        tasks.insert(
            id.to_string(),
            DownloadTask {
                id: id.to_string(),
                source: DownloadSource::Url { url: url.to_string() },
                status: DownloadStatus::Downloading,
                progress: 10.0,
                bytes_downloaded: 1,
                total_bytes: total,
                error: None,
                content_type: None,
                created_at: "2026-07-19T21:00:00+00:00".to_string(),
                updated_at: "2026-07-19T21:01:00+00:00".to_string(),
                sha256: None,
                etag: etag.map(ToString::to_string),
                last_modified: None,
                resumable: false,
                resumed_from: None,
            },
        );
        std::fs::write(data_dir.join("download_tasks.json"), serde_json::to_vec(&tasks).unwrap()).unwrap();
        std::fs::write(DownloadManager::url_partial_path(data_dir, id, url), partial).unwrap();
    }

    #[tokio::test]
    async fn mid_stream_failures_resume_to_identical_file() {
        let data_dir = test_data_dir();
        let body = payload(300_000, 7);
        let server = TestServer::start(body.clone(), Some(ETAG), true).await;
        server.push_fault(Fault::DropAfter(100_000));
        server.push_fault(Fault::DropAfter(50_000));
        let manager = manager(&data_dir).await;

        let id = url_task(&manager, server.url("/files/atlas.zip"), Some(sha256_hex(&body).to_uppercase())).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(task.bytes_downloaded, body.len() as u64);
        assert_eq!(task.resumed_from, Some(150_000));
        assert_eq!(std::fs::read(data_dir.join("misc/atlas.zip")).unwrap(), body);
        assert!(inbox_parts(&data_dir).is_empty());

        let requests = server.requests();
        assert_eq!(requests.len(), 3, "{requests:?}");
        assert_eq!(requests[0].range, None);
        assert_eq!(requests[1].range.as_deref(), Some("bytes=100000-"));
        assert_eq!(requests[1].if_range.as_deref(), Some(ETAG));
        assert_eq!(requests[2].range.as_deref(), Some("bytes=150000-"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn server_without_range_support_restarts_cleanly() {
        let data_dir = test_data_dir();
        let body = payload(200_000, 11);
        let server = TestServer::start(body.clone(), Some(ETAG), false).await;
        server.push_fault(Fault::DropAfter(120_000));
        let manager = manager(&data_dir).await;

        let id = url_task(&manager, server.url("/notes.txt"), None).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(task.resumed_from, None);
        assert_eq!(std::fs::read(data_dir.join("misc/notes.txt")).unwrap(), body);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].range.as_deref(), Some("bytes=120000-"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn changed_etag_restarts_instead_of_splicing() {
        let data_dir = test_data_dir();
        let old_body = payload(150_000, 3);
        let new_body = payload(180_000, 5);
        let server = TestServer::start(new_body.clone(), Some("\"v2\""), true).await;
        let url = server.url("/guide.txt");
        seed_interrupted_task(&data_dir, "changed", &url, &old_body[..60_000], Some(ETAG), Some(150_000));
        let manager = manager(&data_dir).await;

        manager.retry_task("changed").await.expect("retry");
        let task = wait_terminal(&manager, "changed").await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(task.etag.as_deref(), Some("\"v2\""));
        assert_eq!(task.total_bytes, Some(new_body.len() as u64));
        assert_eq!(std::fs::read(data_dir.join("misc/guide.txt")).unwrap(), new_body);
        let requests = server.requests();
        assert_eq!(requests[0].range.as_deref(), Some("bytes=60000-"));
        assert_eq!(requests[0].if_range.as_deref(), Some(ETAG));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn sha256_mismatch_fails_and_removes_file() {
        let data_dir = test_data_dir();
        let body = payload(50_000, 9);
        let server = TestServer::start(body, Some(ETAG), true).await;
        let manager = manager(&data_dir).await;

        let id = url_task(&manager, server.url("/bad.zip"), Some("0".repeat(64))).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Failed);
        assert!(task.error.as_deref().unwrap().contains("checksum mismatch"), "{task:?}");
        assert!(!task.resumable);
        assert!(!data_dir.join("misc/bad.zip").exists());
        assert!(!data_dir.join("inbox/bad.zip").exists());
        assert!(inbox_parts(&data_dir).is_empty());

        // A retry starts from scratch (nothing kept) and fails the same way.
        manager.retry_task(&id).await.expect("retry");
        let task = wait_terminal(&manager, &id).await;
        assert_eq!(task.status, DownloadStatus::Failed);
        assert_eq!(server.requests().last().unwrap().range, None);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn sha256_match_completes() {
        let data_dir = test_data_dir();
        let body = payload(70_000, 13);
        let server = TestServer::start(body.clone(), None, true).await;
        let manager = manager(&data_dir).await;

        let id = url_task(&manager, server.url("/good.zip"), Some(sha256_hex(&body))).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(task.sha256, Some(sha256_hex(&body)));
        assert_eq!(std::fs::read(data_dir.join("misc/good.zip")).unwrap(), body);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn restart_recovery_keeps_partial_and_retry_resumes() {
        let data_dir = test_data_dir();
        let body = payload(250_000, 17);
        let server = TestServer::start(body.clone(), Some(ETAG), true).await;
        let url = server.url("/maps/region.txt");
        seed_interrupted_task(&data_dir, "interrupted", &url, &body[..120_000], Some(ETAG), Some(250_000));
        let manager = manager(&data_dir).await;

        let recovered = manager.get_task("interrupted").await.unwrap();
        assert_eq!(recovered.status, DownloadStatus::Failed);
        assert!(recovered.resumable);
        assert_eq!(recovered.bytes_downloaded, 120_000);
        assert_eq!(recovered.error.as_deref(), Some(INTERRUPTED_BY_RESTART_MESSAGE));

        let queued = manager.retry_task("interrupted").await.expect("retry");
        assert_eq!(queued.status, DownloadStatus::Queued);
        assert_eq!(queued.bytes_downloaded, 120_000);

        let task = wait_terminal(&manager, "interrupted").await;
        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(task.resumed_from, Some(120_000));
        assert_eq!(std::fs::read(data_dir.join("misc/region.txt")).unwrap(), body);
        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].range.as_deref(), Some("bytes=120000-"));
        assert_eq!(requests[0].if_range.as_deref(), Some(ETAG));

        // Completed tasks are not retriable.
        assert!(matches!(manager.retry_task("interrupted").await, Err(RetryError::NotRetriable(_))));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn complete_partial_is_accepted_on_416() {
        let data_dir = test_data_dir();
        let body = payload(40_000, 19);
        let server = TestServer::start(body.clone(), Some(ETAG), true).await;
        let url = server.url("/done.txt");
        seed_interrupted_task(&data_dir, "full", &url, &body, Some(ETAG), Some(40_000));
        let manager = manager(&data_dir).await;

        manager.retry_task("full").await.expect("retry");
        let task = wait_terminal(&manager, "full").await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(std::fs::read(data_dir.join("misc/done.txt")).unwrap(), body);
        assert_eq!(server.requests()[0].range.as_deref(), Some("bytes=40000-"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn oversized_partial_restarts_after_416() {
        let data_dir = test_data_dir();
        let body = payload(30_000, 23);
        let server = TestServer::start(body.clone(), Some(ETAG), true).await;
        let url = server.url("/grown.txt");
        let mut stale = body.clone();
        stale.extend_from_slice(b"garbage");
        seed_interrupted_task(&data_dir, "grown", &url, &stale, Some(ETAG), None);
        let manager = manager(&data_dir).await;

        manager.retry_task("grown").await.expect("retry");
        let task = wait_terminal(&manager, "grown").await;

        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(std::fs::read(data_dir.join("misc/grown.txt")).unwrap(), body);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].range, None);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn exhausted_retries_keep_partial_and_mark_resumable() {
        let data_dir = test_data_dir();
        let body = payload(100_000, 29);
        // No range support: every attempt restarts and dies at the same
        // offset, so no progress is made and the attempts run out.
        let server = TestServer::start(body.clone(), Some(ETAG), false).await;
        for _ in 0..MAX_DOWNLOAD_ATTEMPTS {
            server.push_fault(Fault::DropAfter(10_000));
        }
        let manager = manager(&data_dir).await;
        let url = server.url("/flaky.txt");

        let id = url_task(&manager, url.clone(), None).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Failed, "{task:?}");
        assert!(task.resumable);
        assert_eq!(task.bytes_downloaded, 10_000);
        let partial = DownloadManager::url_partial_path(&data_dir, &id, &url);
        assert_eq!(std::fs::metadata(&partial).unwrap().len(), 10_000);
        assert_eq!(server.requests().len(), MAX_DOWNLOAD_ATTEMPTS as usize);

        // The link recovered: retry finishes the download.
        manager.retry_task(&id).await.expect("retry");
        let task = wait_terminal(&manager, &id).await;
        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(std::fs::read(data_dir.join("misc/flaky.txt")).unwrap(), body);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn non_retriable_http_error_fails_without_partial() {
        let data_dir = test_data_dir();
        let server = TestServer::start(Vec::new(), None, true).await;
        let manager = manager(&data_dir).await;

        let id = url_task(&manager, server.url("/missing"), None).await;
        let task = wait_terminal(&manager, &id).await;

        assert_eq!(task.status, DownloadStatus::Failed);
        assert!(task.error.as_deref().unwrap().contains("404"));
        assert!(!task.resumable);
        assert_eq!(server.requests().len(), 1);
        assert!(inbox_parts(&data_dir).is_empty());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn dismiss_deletes_resumable_partial() {
        let data_dir = test_data_dir();
        let url = "https://example.com/keep.zim";
        seed_interrupted_task(&data_dir, "keep", url, b"partial-bytes", Some(ETAG), None);
        let manager = DownloadManager::new(&data_dir);
        let partial = DownloadManager::url_partial_path(&data_dir, "keep", url);
        assert!(partial.exists());

        assert!(manager.dismiss_task("keep").await.unwrap());
        assert!(!partial.exists(), "dismissing a resumable task removes its partial");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn user_cancel_mid_download_deletes_partial() {
        let data_dir = test_data_dir();
        let body = payload(400_000, 37);
        let server = TestServer::start(body, Some(ETAG), true).await;
        // Repeated drops keep the worker cycling through cancel checkpoints.
        for _ in 0..6 {
            server.push_fault(Fault::DropAfter(30_000));
        }
        let manager = manager(&data_dir).await;
        let url = server.url("/cancel.txt");
        let id = url_task(&manager, url.clone(), None).await;

        let partial = DownloadManager::url_partial_path(&data_dir, &id, &url);
        for _ in 0..400 {
            if partial.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        manager.cancel_task(&id).await.unwrap();
        let task = wait_terminal(&manager, &id).await;
        assert_eq!(task.status, DownloadStatus::Cancelled);
        assert!(!task.resumable);
        assert!(!partial.exists());
        assert!(!data_dir.join("misc/cancel.txt").exists());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn retry_rejects_unknown_local_and_active_tasks() {
        let data_dir = test_data_dir();
        let server = TestServer::start(payload(50_000, 41), Some(ETAG), true).await;
        server.push_fault(Fault::StallAfter(10_000));
        let manager = manager(&data_dir).await;

        assert!(matches!(manager.retry_task("nope").await, Err(RetryError::NotFound)));

        let local = manager
            .create_task(DownloadSource::LocalFile { path: data_dir.join("missing.epub") })
            .await;
        wait_terminal(&manager, &local).await;
        assert!(matches!(manager.retry_task(&local).await, Err(RetryError::NotRetriable(_))));

        let active = url_task(&manager, server.url("/stalled.txt"), None).await;
        assert!(matches!(manager.retry_task(&active).await, Err(RetryError::NotRetriable(_))));

        manager.shutdown(Duration::from_secs(5)).await;
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn shutdown_persists_in_flight_task_as_resumable() {
        let data_dir = test_data_dir();
        let body = payload(90_000, 43);
        let server = TestServer::start(body.clone(), Some(ETAG), true).await;
        server.push_fault(Fault::StallAfter(25_000));
        let manager = manager(&data_dir).await;
        let url = server.url("/stalled.txt");
        let id = url_task(&manager, url.clone(), None).await;

        // Wait until the bytes sent before the stall have reached the disk.
        let partial = DownloadManager::url_partial_path(&data_dir, &id, &url);
        for _ in 0..400 {
            if DownloadManager::partial_len(&partial).await >= 25_000 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let started = std::time::Instant::now();
        manager.shutdown(Duration::from_secs(5)).await;
        assert!(started.elapsed() < Duration::from_secs(3), "stalled worker must stop promptly");

        let kept = DownloadManager::partial_len(&partial).await;
        assert_eq!(kept, 25_000);

        let persisted = DownloadManager::load_tasks(&data_dir.join("download_tasks.json")).unwrap();
        let task = &persisted[&id];
        assert_eq!(task.status, DownloadStatus::Failed);
        assert!(task.resumable);
        assert_eq!(task.error.as_deref(), Some(INTERRUPTED_BY_SHUTDOWN_MESSAGE));
        assert_eq!(task.bytes_downloaded, kept);
        assert_eq!(task.etag.as_deref(), Some(ETAG));
        assert!(manager.retry_task(&id).await.is_err(), "no retries while shutting down");
        drop(manager);

        // Next process: the task resumes where it stopped.
        let restarted = self::manager(&data_dir).await;
        restarted.retry_task(&id).await.expect("retry");
        let task = wait_terminal(&restarted, &id).await;
        assert_eq!(task.status, DownloadStatus::Completed, "{task:?}");
        assert_eq!(std::fs::read(data_dir.join("misc/stalled.txt")).unwrap(), body);
        assert_eq!(server.requests().last().unwrap().range.as_deref(), Some("bytes=25000-"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn stale_cleanup_keeps_resumable_partials() {
        let data_dir = test_data_dir();
        let url = "https://example.com/big.zim";
        seed_interrupted_task(&data_dir, "resumable", url, b"keep me", Some(ETAG), None);
        let kept = DownloadManager::url_partial_path(&data_dir, "resumable", url);
        let orphan = data_dir.join("inbox/unknown-task-file.zim.part");
        std::fs::write(&orphan, b"orphan").unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(48 * 60 * 60);
        for path in [&kept, &orphan] {
            std::fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
        }

        let manager = DownloadManager::new(&data_dir);
        assert!(kept.exists(), "resumable partial must survive the 24h cleanup");
        assert!(!orphan.exists(), "orphaned partial is still cleaned up");
        assert!(manager.get_task("resumable").await.unwrap().resumable);

        // A second restart (task already failed + resumable) keeps it too.
        drop(manager);
        let _manager = DownloadManager::new(&data_dir);
        assert!(kept.exists());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn helpers_parse_ranges_and_validate_sha256() {
        assert_eq!(DownloadManager::parse_content_range_value("bytes 100-199/200"), Some((100, Some(200))));
        assert_eq!(DownloadManager::parse_content_range_value("bytes 5-9/*"), Some((5, None)));
        assert_eq!(DownloadManager::parse_content_range_value("items 1-2/3"), None);
        assert_eq!(DownloadManager::if_range_validator(Some("W/\"x\""), Some("date")), Some("date"));
        assert_eq!(DownloadManager::if_range_validator(Some("\"x\""), Some("date")), Some("\"x\""));
        assert_eq!(DownloadManager::if_range_validator(None, None), None);
        assert_eq!(normalize_sha256(&format!(" {} ", "AB".repeat(32))), Some("ab".repeat(32)));
        assert_eq!(normalize_sha256("abc"), None);
        assert_eq!(normalize_sha256(&"g".repeat(64)), None);
    }
}
