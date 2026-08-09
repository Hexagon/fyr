//! Fyr Downloader — Content ingestion and file routing
//!
//! Handles:
//! - HTTP/HTTPS downloads
//! - Local file imports
//! - Automatic file routing to appropriate directories

pub mod manager;
pub mod router;

pub use manager::DownloadManager;
pub use manager::{
    DEFAULT_REQUEST_TIMEOUT_SECS, MAX_REQUEST_TIMEOUT_SECS, MIN_REQUEST_TIMEOUT_SECS,
};
pub use router::ContentRouter;
