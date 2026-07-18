//! Core types and data structures for Fyr

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Content types supported by Fyr
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    /// Map tiles in PMTiles format
    Map,
    /// Books in EPUB, MOBI, PDF, or ZIM format
    Book,
    /// Point of Interest data in FlatGeoBuf or GeoJSON format
    Poi,
    /// Local GGUF model files for the offline assistant
    Model,
    /// Generic files that do not belong to a specialized category
    Misc,
}

impl ContentType {
    pub fn directory_name(&self) -> &'static str {
        match self {
            ContentType::Map => "maps",
            ContentType::Book => "books",
            ContentType::Poi => "poi",
            ContentType::Model => "models",
            ContentType::Misc => "misc",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "pmtiles" => Some(ContentType::Map),
            "epub" | "pdf" | "mobi" | "zim" => Some(ContentType::Book),
            "fgb" | "geojson" | "json" => Some(ContentType::Poi),
            "gguf" => Some(ContentType::Model),
            "txt" | "md" | "csv" | "zip" | "7z" | "log" => Some(ContentType::Misc),
            _ => None,
        }
    }
}

/// Metadata for a downloadable content item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub id: String,
    pub name: String,
    pub content_type: ContentType,
    pub file_path: PathBuf,
    pub file_size: u64,
    pub checksum: Option<String>,
    pub created_at: String,
}

/// Status of a download task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Validating,
    Routing,
    Completed,
    Failed,
    Cancelled,
}

/// Download task representing a content ingestion job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub source: DownloadSource,
    pub status: DownloadStatus,
    pub progress: f32,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub content_type: Option<ContentType>,
    pub created_at: String,
    pub updated_at: String,
}

/// Source of content being downloaded
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DownloadSource {
    Url { url: String },
    LocalFile { path: PathBuf },
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
}

/// Validation result for content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub detected_type: Option<ContentType>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            detected_type: None,
        }
    }
}
