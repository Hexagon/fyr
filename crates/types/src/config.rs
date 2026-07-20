//! Configuration management for Fyr

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub data_dir: PathBuf,
    pub auth: AuthConfig,
}

/// Authentication and access-control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// When set, mutating API endpoints require a valid session.
    /// Visitors without a session can only use read-only endpoints.
    pub admin_password: Option<String>,
    /// When true, all mutating endpoints are permanently disabled regardless
    /// of any session state. Takes precedence over `admin_password`.
    pub readonly: bool,
}

impl AuthConfig {
    /// Returns true when the system is in any restricted mode (read-only or
    /// password-protected).
    pub fn is_restricted(&self) -> bool {
        self.readonly || self.admin_password.is_some()
    }
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
            auth: AuthConfig {
                admin_password: None,
                readonly: false,
            },
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

    /// Get path to local model directory
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// Get path to generic misc directory
    pub fn misc_dir(&self) -> PathBuf {
        self.data_dir.join("misc")
    }

    /// Initialize data directory structure
    pub fn initialize_directories(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.maps_dir())?;
        std::fs::create_dir_all(self.books_dir())?;
        std::fs::create_dir_all(self.poi_dir())?;
        std::fs::create_dir_all(self.inbox_dir())?;
        std::fs::create_dir_all(self.models_dir())?;
        std::fs::create_dir_all(self.misc_dir())?;
        Ok(())
    }

    /// Verify that runtime data paths are writable.
    pub fn validate_writable(&self) -> Result<()> {
        let probe = self.data_dir.join(".fyr-write-test");

        std::fs::write(&probe, b"fyr")
            .with_context(|| format!("Data directory is not writable: {}", self.data_dir.display()))?;

        std::fs::remove_file(&probe)
            .with_context(|| format!("Failed to clean write probe file: {}", probe.display()))?;

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = std::env::var("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let public_data = PathBuf::from("./public/data");
                let legacy_data = PathBuf::from("./data");
                if public_data.exists() {
                    public_data
                } else if legacy_data.exists() {
                    legacy_data
                } else {
                    PathBuf::from("./public/data")
                }
            });

        let host = std::env::var("FYR_HOST")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = std::env::var("FYR_PORT")
            .or_else(|_| std::env::var("PORT"))
            .ok()
            .and_then(|raw| raw.parse::<u16>().ok())
            .unwrap_or(8080);

        let mut config = Self::default_with_data_dir(data_dir);
        config.server.host = host;
        config.server.port = port;

        let admin_password = std::env::var("FYR_ADMIN_PASSWORD").ok().filter(|s| !s.is_empty());
        let readonly = std::env::var("FYR_READONLY")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        config.auth = AuthConfig {
            admin_password,
            readonly,
        };

        config
    }
}
