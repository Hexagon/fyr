//! Download task manager

use nexus_core::{DownloadTask, DownloadStatus, DownloadSource};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Manages active and completed download tasks
pub struct DownloadManager {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new download task
    pub async fn create_task(&self, source: DownloadSource) -> String {
        let task_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().to_rfc3339();

        let task = DownloadTask {
            id: task_id.clone(),
            source,
            status: DownloadStatus::Queued,
            progress: 0.0,
            bytes_downloaded: 0,
            total_bytes: None,
            error: None,
            content_type: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut tasks = self.tasks.write().await;
        tasks.insert(task_id.clone(), task);
        task_id
    }

    /// Get task by ID
    pub async fn get_task(&self, task_id: &str) -> Option<DownloadTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Update task status
    pub async fn update_status(&self, task_id: &str, status: DownloadStatus) -> anyhow::Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = status;
            task.updated_at = chrono::Local::now().to_rfc3339();
        }
        Ok(())
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Vec<DownloadTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

// Add chrono dependency for timestamp
use chrono;
