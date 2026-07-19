//! Fyr Downloader — Content ingestion and file routing
//!
//! Handles:
//! - HTTP/HTTPS downloads
//! - Local file imports
//! - Automatic file routing to appropriate directories

pub mod router;
pub mod manager;

pub use router::ContentRouter;
pub use manager::DownloadManager;
