//! Shared application state

use nexus_core::Config;
use nexus_downloader::DownloadManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub download_manager: Arc<DownloadManager>,
}
