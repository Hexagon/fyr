//! Shared application state

use types::Config;
use downloader::DownloadManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub download_manager: Arc<DownloadManager>,
}
