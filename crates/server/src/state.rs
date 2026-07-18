//! Shared application state

use crate::ai::ModelManager;
use crate::settings::SettingsManager;
use types::Config;
use downloader::DownloadManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub download_manager: Arc<DownloadManager>,
    pub model_manager: Arc<ModelManager>,
    pub settings_manager: Arc<SettingsManager>,
}
