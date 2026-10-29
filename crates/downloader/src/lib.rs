//! Fyr Downloader — Content ingestion and file routing
//!
//! Handles:
//! - HTTP/HTTPS downloads
//! - Local file imports
//! - Automatic file routing to appropriate directories

pub mod router;
pub mod manager;
#[cfg(test)]
mod test_server;

pub use router::ContentRouter;
pub use manager::DownloadManager;
pub use manager::{
	normalize_sha256,
	DownloadOptions,
	RetryError,
	DEFAULT_REQUEST_TIMEOUT_SECS,
	MAX_REQUEST_TIMEOUT_SECS,
	MIN_REQUEST_TIMEOUT_SECS,
};
