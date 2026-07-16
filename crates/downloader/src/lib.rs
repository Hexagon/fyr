//! Nexus Downloader — Content ingestion and file routing
//!
//! Handles:
//! - HTTP/HTTPS downloads
//! - Local file imports
//! - Automatic file routing to appropriate directories
//! - Content validation

pub mod download;
pub mod router;
pub mod manager;

pub use download::{Download, DownloadEngine};
pub use router::ContentRouter;
pub use manager::DownloadManager;
