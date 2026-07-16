//! Configuration management for Offline Nexus

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::Result;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Config {
    /// Create default configuration
    pub fn default_with_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            data_dir,
        }
    }

    /// Get path to maps directory
    pub fn maps_dir(&self) -> PathBuf {
        self.data_dir.join("maps")
    }

    /// Get path to books directory
    pub fn books_dir(&self) -> PathBuf {
        self.data_dir.join("books")
    }

    /// Get path to POI directory
    pub fn poi_dir(&self) -> PathBuf {
        self.data_dir.join("poi")
    }

    /// Get path to downloads directory (inbox)
    pub fn inbox_dir(&self) -> PathBuf {
        self.data_dir.join("inbox")
    }

    /// Initialize data directory structure
    pub fn initialize_directories(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.maps_dir())?;
        std::fs::create_dir_all(self.books_dir())?;
        std::fs::create_dir_all(self.poi_dir())?;
        std::fs::create_dir_all(self.inbox_dir())?;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = PathBuf::from("./data");
        Self::default_with_data_dir(data_dir)
    }
}
