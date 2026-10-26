//! Configuration management for Fyr

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CURATED_CONTENT_FILENAME: &str = "curated-content.json";
const DEFAULT_CURATED_CONTENT: &str = include_str!("../../../public/data/curated-content.json");

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub data_dir: PathBuf,
    pub auth: AuthConfig,
    pub ai: AiConfig,
    pub messaging: MessagingConfig,
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
    /// When true (`FYR_GUEST_MESSAGING`), visitors without an admin session
    /// may post messages and create channels even when `admin_password` is
    /// set. Has no effect in read-only mode or in open mode (where everyone
    /// may already post).
    #[serde(default)]
    pub guest_messaging: bool,
}

impl AuthConfig {
    /// Returns true when the system is in any restricted mode (read-only or
    /// password-protected).
    pub fn is_restricted(&self) -> bool {
        self.readonly || self.admin_password.is_some()
    }
}

/// Messages (public board channel + encrypted channels) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingConfig {
    /// Master switch (`FYR_MESSAGING`, default on).
    pub enabled: bool,
    /// Newest messages kept per channel (`FYR_MESSAGES_MAX_PER_CHANNEL`).
    pub max_messages_per_channel: u32,
    /// Maximum age of encrypted-channel messages in days
    /// (`FYR_MESSAGES_MAX_AGE_DAYS`).
    pub max_age_days: u32,
}

impl Default for MessagingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_messages_per_channel: 1000,
            max_age_days: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Reverse proxies whose `X-Forwarded-For` is honoured for client IPs
    /// (`FYR_TRUSTED_PROXIES`). Empty by default: trust nobody.
    #[serde(default)]
    pub trusted_proxies: crate::proxy::TrustedProxies,
    /// `FYR_TRUSTED_PROXIES` entries that could not be parsed; logged as a
    /// warning at startup.
    #[serde(skip)]
    pub trusted_proxies_rejected: Vec<String>,
    /// Built-in HTTPS (`FYR_TLS`, `FYR_TLS_CERT`/`FYR_TLS_KEY`,
    /// `FYR_TLS_SANS`, `FYR_HTTP_REDIRECT_PORT`). Off by default.
    #[serde(default)]
    pub tls: crate::tls::TlsConfig,
    /// Invalid TLS environment combination; the server refuses to start
    /// with this message (see [`Config::validate`]).
    #[serde(skip)]
    pub tls_error: Option<String>,
}

/// Local AI inference runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Optional operator override for the Candle CPU inference thread pool
    /// size (`FYR_AI_THREADS`). When unset, a container-aware default is
    /// computed at startup from `std::thread::available_parallelism()`.
    pub threads: Option<usize>,
}

impl Config {
    /// Create default configuration
    pub fn default_with_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                trusted_proxies: crate::proxy::TrustedProxies::default(),
                trusted_proxies_rejected: Vec::new(),
                tls: crate::tls::TlsConfig::default(),
                tls_error: None,
            },
            data_dir,
            auth: AuthConfig {
                admin_password: None,
                readonly: false,
                guest_messaging: false,
            },
            ai: AiConfig { threads: None },
            messaging: MessagingConfig::default(),
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

    /// Get path to the messages database. Lives directly in the data
    /// directory, which is never served as a whole (only the public content
    /// folders under `/data/*` are).
    pub fn messages_db_path(&self) -> PathBuf {
        self.data_dir.join("messages.sqlite")
    }

    /// Directory holding the generated self-signed certificate
    /// (`FYR_TLS=self-signed`). Never served over HTTP.
    pub fn tls_dir(&self) -> PathBuf {
        self.data_dir.join("tls")
    }

    /// Reject configuration errors detected while reading the environment
    /// (currently: invalid TLS combinations). Call before serving.
    pub fn validate(&self) -> Result<()> {
        if let Some(error) = &self.server.tls_error {
            anyhow::bail!("Invalid HTTPS configuration: {error}");
        }
        Ok(())
    }

    /// Get path to the curated content catalog
    pub fn curated_content_path(&self) -> PathBuf {
        self.data_dir.join(CURATED_CONTENT_FILENAME)
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

        let curated_content_path = self.curated_content_path();
        if !curated_content_path.exists() {
            std::fs::write(&curated_content_path, DEFAULT_CURATED_CONTENT)?;
        }

        Ok(())
    }

    /// Verify that runtime data paths are writable.
    pub fn validate_writable(&self) -> Result<()> {
        let probe = self.data_dir.join(".fyr-write-test");

        std::fs::write(&probe, b"fyr").with_context(|| {
            format!(
                "Data directory is not writable: {}",
                self.data_dir.display()
            )
        })?;

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
        if let Ok(raw) = std::env::var("FYR_TRUSTED_PROXIES") {
            let (trusted, rejected) = crate::proxy::TrustedProxies::parse(&raw);
            config.server.trusted_proxies = trusted;
            config.server.trusted_proxies_rejected = rejected;
        }
        match crate::tls::TlsConfig::from_lookup(|name| std::env::var(name).ok(), port) {
            Ok(tls) => config.server.tls = tls,
            Err(error) => config.server.tls_error = Some(error),
        }

        let admin_password = std::env::var("FYR_ADMIN_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());
        let readonly = std::env::var("FYR_READONLY")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let guest_messaging = env_flag("FYR_GUEST_MESSAGING").unwrap_or(false);

        config.auth = AuthConfig {
            admin_password,
            readonly,
            guest_messaging,
        };

        let defaults = MessagingConfig::default();
        config.messaging = MessagingConfig {
            enabled: env_flag("FYR_MESSAGING").unwrap_or(defaults.enabled),
            max_messages_per_channel: env_u32("FYR_MESSAGES_MAX_PER_CHANNEL")
                .map(|v| v.clamp(10, 100_000))
                .unwrap_or(defaults.max_messages_per_channel),
            max_age_days: env_u32("FYR_MESSAGES_MAX_AGE_DAYS")
                .map(|v| v.clamp(1, 3650))
                .unwrap_or(defaults.max_age_days),
        };

        let ai_threads = std::env::var("FYR_AI_THREADS")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .filter(|threads| *threads > 0);
        config.ai = AiConfig { threads: ai_threads };

        config
    }
}

/// Parse a boolean environment flag (`1|true|yes|on` / `0|false|no|off`).
fn env_flag(name: &str) -> Option<bool> {
    parse_flag(&std::env::var(name).ok()?)
}

fn parse_flag(raw: &str) -> Option<bool> {
    match raw.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.trim().parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_flag, Config};

    #[test]
    fn parse_flag_accepts_common_spellings() {
        for raw in ["1", "true", "YES", " on "] {
            assert_eq!(parse_flag(raw), Some(true), "{raw}");
        }
        for raw in ["0", "false", "No", "off"] {
            assert_eq!(parse_flag(raw), Some(false), "{raw}");
        }
        assert_eq!(parse_flag("maybe"), None);
    }

    #[test]
    fn tls_is_off_by_default_and_validate_reports_errors() {
        let mut config = Config::default_with_data_dir("/tmp/x");
        assert!(!config.server.tls.is_enabled());
        assert!(config.validate().is_ok());
        assert!(config.tls_dir().ends_with("tls"));
        config.server.tls_error = Some("boom".into());
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("HTTPS") && error.contains("boom"), "{error}");
    }

    #[test]
    fn messaging_defaults_are_safe() {
        let config = Config::default_with_data_dir("/tmp/x");
        assert!(config.messaging.enabled);
        assert!(!config.auth.guest_messaging);
        assert_eq!(config.messaging.max_messages_per_channel, 1000);
        assert_eq!(config.messaging.max_age_days, 90);
        assert!(config.messages_db_path().ends_with("messages.sqlite"));
    }
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_data_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("fyr-types-tests-{unique}"))
    }

    #[test]
    fn initialize_directories_seeds_curated_catalog_when_missing() {
        let data_dir = test_data_dir();
        let config = Config::default_with_data_dir(&data_dir);

        config
            .initialize_directories()
            .expect("initialize directories");

        assert!(config.maps_dir().is_dir());
        assert!(config.books_dir().is_dir());
        assert!(config.poi_dir().is_dir());
        assert!(config.inbox_dir().is_dir());
        assert!(config.models_dir().is_dir());
        assert!(config.misc_dir().is_dir());

        let catalog =
            fs::read_to_string(config.curated_content_path()).expect("read curated catalog");
        assert!(catalog.contains("\"schema_version\": 1"));
        assert!(catalog.contains("\"assistant\""));
        assert!(catalog.contains("\"models\""));

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn initialize_directories_preserves_existing_curated_catalog() {
        let data_dir = test_data_dir();
        fs::create_dir_all(&data_dir).expect("create test data dir");

        let config = Config::default_with_data_dir(&data_dir);
        let existing = "{\n  \"custom\": true\n}\n";
        fs::write(config.curated_content_path(), existing).expect("write existing catalog");

        config
            .initialize_directories()
            .expect("initialize directories");

        let catalog =
            fs::read_to_string(config.curated_content_path()).expect("read curated catalog");
        assert_eq!(catalog, existing);

        let _ = fs::remove_dir_all(data_dir);
    }
}
