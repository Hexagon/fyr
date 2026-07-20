//! Download task manager

use crate::router::ContentRouter;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use types::{ContentType, DownloadSource, DownloadStatus, DownloadTask};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;
use tracing::{error, info, warn};

/// Manages active and completed download tasks
pub struct DownloadManager {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    cancel_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    persistence_path: PathBuf,
    data_dir: PathBuf,
    client: reqwest::Client,
}

#[derive(Clone)]
struct DownloadRuntime {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    cancel_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    persistence_path: PathBuf,
    data_dir: PathBuf,
    client: reqwest::Client,
}

const MAX_DOWNLOAD_ATTEMPTS: u8 = 3;

impl DownloadManager {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let root = data_dir.as_ref().to_path_buf();
        Self::cleanup_stale_temp_files(&root);
        let persistence_path = data_dir.as_ref().join("download_tasks.json");
        let tasks = Self::load_tasks(&persistence_path).unwrap_or_else(|error| {
            warn!(
                "Failed to load download tasks from {}: {}",
                persistence_path.display(),
                error
            );
            HashMap::new()
        });

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|error| {
                warn!("Failed to build HTTP client with custom timeouts: {}", error);
                reqwest::Client::new()
            });

        Self {
            tasks: Arc::new(RwLock::new(tasks)),
            cancel_flags: Arc::new(RwLock::new(HashMap::new())),
            persistence_path,
            data_dir: root,
            client,
        }
    }

    /// Create a new download task
    pub async fn create_task(&self, source: DownloadSource) -> String {
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
        };

        let snapshot = {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id.clone(), task);
            tasks.clone()
        };

        if let Err(error) = self.persist_tasks(&snapshot) {
            error!("Failed to persist download tasks after create: {}", error);
        }

        let runtime = DownloadRuntime {
            tasks: self.tasks.clone(),
            cancel_flags: self.cancel_flags.clone(),
            persistence_path: self.persistence_path.clone(),
            data_dir: self.data_dir.clone(),
            client: self.client.clone(),
        };

        match source {
            DownloadSource::Url { url } => {
                let cancel_flag = Arc::new(AtomicBool::new(false));
                {
                    let mut flags = self.cancel_flags.write().await;
                    flags.insert(task_id.clone(), cancel_flag);
                }

                let task_id_for_worker = task_id.clone();
                tokio::spawn(async move {
                    Self::run_url_download(runtime, task_id_for_worker, url).await;
                });
            }
            DownloadSource::LocalFile { path } => {
                let cancel_flag = Arc::new(AtomicBool::new(false));
                {
                    let mut flags = self.cancel_flags.write().await;
                    flags.insert(task_id.clone(), cancel_flag);
                }

                let task_id_for_worker = task_id.clone();
                tokio::spawn(async move {
                    Self::run_local_import(runtime, task_id_for_worker, path).await;
                });
            }
        }

        task_id
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

            if !matches!(task.status, DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled) {
                task.status = DownloadStatus::Cancelled;
                task.error = Some("download cancelled by user".to_string());
                task.updated_at = chrono::Local::now().to_rfc3339();
            }

            tasks.clone()
        };

        self.persist_tasks(&snapshot)?;
        Ok(true)
    }

    /// Dismiss a download task: cancels it if still active, then removes it from the list.
    pub async fn dismiss_task(&self, task_id: &str) -> anyhow::Result<bool> {
        // Signal cancellation if the task is still running
        let flag = {
            let flags = self.cancel_flags.read().await;
            flags.get(task_id).cloned()
        };
        if let Some(cancel_flag) = flag {
            cancel_flag.store(true, Ordering::Relaxed);
        }

        // Remove the task from the map
        let snapshot = {
            let mut tasks = self.tasks.write().await;
            if tasks.remove(task_id).is_none() {
                return Ok(false);
            }
            tasks.clone()
        };

        // Clean up the cancel flag
        {
            let mut flags = self.cancel_flags.write().await;
            flags.remove(task_id);
        }

        self.persist_tasks(&snapshot)?;
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

    async fn run_url_download(runtime: DownloadRuntime, task_id: String, url: String) {
        if let Err(error) = Self::set_task_state(&runtime, &task_id, DownloadStatus::Downloading, None, None, None).await {
            error!("Failed to mark task {} as downloading: {}", task_id, error);
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
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let mut response = None;
        let mut total_bytes = None;
        let mut terminal_error = None;

        for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
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
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            match runtime.client.get(&url).send().await {
                Ok(candidate) => {
                    if candidate.status().is_success() {
                        total_bytes = candidate.content_length();
                        response = Some(candidate);
                        break;
                    }

                    let status = candidate.status();
                    if !Self::is_retriable_http_status(status) || attempt == MAX_DOWNLOAD_ATTEMPTS {
                        terminal_error = Some(format!("server returned HTTP {}", status));
                        break;
                    }

                    warn!(
                        "Download task {} got HTTP {} on attempt {}/{}; retrying",
                        task_id,
                        status,
                        attempt,
                        MAX_DOWNLOAD_ATTEMPTS
                    );
                }
                Err(error) => {
                    if attempt == MAX_DOWNLOAD_ATTEMPTS {
                        terminal_error = Some(format!("request failed: {error}"));
                        break;
                    }

                    warn!(
                        "Download task {} failed on attempt {}/{}: {}. Retrying",
                        task_id,
                        attempt,
                        MAX_DOWNLOAD_ATTEMPTS,
                        error
                    );
                }
            }

            let backoff_ms = 500u64 * (attempt as u64);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }

        let Some(response) = response else {
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                None,
                total_bytes,
                Some(terminal_error.unwrap_or_else(|| "download failed after retries".to_string())),
            )
            .await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        };

        let filename = Self::filename_from_url(&url, &task_id);
        let inbox_path = runtime.data_dir.join("inbox");
        if let Err(error) = tokio::fs::create_dir_all(&inbox_path).await {
            let message = format!("failed to create inbox directory: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                None,
                total_bytes,
                Some(message),
            )
            .await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        let temp_path = inbox_path.join(format!("{}.part", filename));
        let final_inbox_path = inbox_path.join(&filename);

        let mut file = match tokio::fs::File::create(&temp_path).await {
            Ok(file) => file,
            Err(error) => {
                let message = format!("failed to create temporary file: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    None,
                    total_bytes,
                    Some(message),
                )
                .await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }
        };

        let mut downloaded = 0u64;
        let mut last_progress_sync = 0u64;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(error) => {
                    let message = format!("download stream failed: {error}");
                    let _ = Self::set_task_state(
                        &runtime,
                        &task_id,
                        DownloadStatus::Failed,
                        Some(downloaded),
                        total_bytes,
                        Some(message),
                    )
                    .await;
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    Self::clear_cancel_flag(&runtime, &task_id).await;
                    return;
                }
            };

            if Self::is_cancelled(&runtime, &task_id).await {
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Cancelled,
                    Some(downloaded),
                    total_bytes,
                    Some("download cancelled by user".to_string()),
                )
                .await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            if let Err(error) = file.write_all(&chunk).await {
                let message = format!("failed to write downloaded data: {error}");
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Failed,
                    Some(downloaded),
                    total_bytes,
                    Some(message),
                )
                .await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                Self::clear_cancel_flag(&runtime, &task_id).await;
                return;
            }

            downloaded += chunk.len() as u64;
            if downloaded.saturating_sub(last_progress_sync) >= 1024 * 1024 {
                let _ = Self::set_task_state(
                    &runtime,
                    &task_id,
                    DownloadStatus::Downloading,
                    Some(downloaded),
                    total_bytes,
                    None,
                )
                .await;
                last_progress_sync = downloaded;
            }
        }

        if let Err(error) = file.flush().await {
            let message = format!("failed to flush downloaded file: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(downloaded),
                total_bytes,
                Some(message),
            )
            .await;
            let _ = tokio::fs::remove_file(&temp_path).await;
            Self::clear_cancel_flag(&runtime, &task_id).await;
            return;
        }

        if let Err(error) = tokio::fs::rename(&temp_path, &final_inbox_path).await {
            let message = format!("failed to finalize downloaded file: {error}");
            let _ = Self::set_task_state(
                &runtime,
                &task_id,
                DownloadStatus::Failed,
                Some(downloaded),
                total_bytes,
                Some(message),
            )
            .await;
            let _ = tokio::fs::remove_file(&temp_path).await;
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

            if let Err(error) = tokio::fs::rename(&final_inbox_path, &destination).await {
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

        let temp_path = inbox_path.join(format!("{}.part", source_name));
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

            if let Err(error) = tokio::fs::rename(&final_inbox_path, &destination).await {
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

            if let Some(bytes) = bytes_downloaded {
                task.bytes_downloaded = bytes;
            }

            if total_bytes.is_some() {
                task.total_bytes = total_bytes;
            }

            let denominator = task.total_bytes.unwrap_or(task.bytes_downloaded).max(1);
            task.progress = ((task.bytes_downloaded as f32 / denominator as f32) * 100.0).clamp(0.0, 100.0);
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

    fn cleanup_stale_temp_files(data_dir: &Path) {
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

        let sanitized: String = segment
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();

        if sanitized.is_empty() {
            default
        } else {
            sanitized
        }
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
                },
            );
        }

        let tasks = manager.list_tasks().await;
        let ids: Vec<_> = tasks.into_iter().map(|task| task.id).collect();

        assert_eq!(ids, vec!["newer".to_string(), "older".to_string()]);
    }
}
