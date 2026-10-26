//! Shared application state

use crate::ai::ModelManager;
use crate::auth::AuthManager;
use crate::mbtiles::MbtilesPool;
use crate::messaging::Messaging;
use crate::settings::SettingsManager;
use types::Config;
use downloader::DownloadManager;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub static_dir: PathBuf,
    pub download_manager: Arc<DownloadManager>,
    pub model_manager: Arc<ModelManager>,
    pub settings_manager: Arc<SettingsManager>,
    pub auth_manager: Arc<AuthManager>,
    /// Reusable read-only SQLite connections and cached tile format per MBTiles archive.
    pub mbtiles_pool: Arc<MbtilesPool>,
    /// Public board channel + encrypted channels (SQLite-backed).
    pub messaging: Arc<Messaging>,
}
