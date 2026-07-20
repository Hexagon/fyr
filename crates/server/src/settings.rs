use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
use types::{AppSettings, GeoPosition};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone)]
pub struct SettingsManager {
    path: PathBuf,
    settings: Arc<RwLock<AppSettings>>,
}

impl SettingsManager {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let path = data_dir.as_ref().join("settings.json");
        let settings = Self::load_from_disk(&path).unwrap_or_else(|error| {
            if path.exists() {
                warn!("Failed to load settings from {}: {}", path.display(), error);
            }
            AppSettings::default()
        });

        Self {
            path,
            settings: Arc::new(RwLock::new(settings)),
        }
    }

    pub fn current(&self) -> AppSettings {
        match self.settings.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                warn!("Settings lock was poisoned while reading; returning last known value");
                poisoned.into_inner().clone()
            }
        }
    }

    pub fn replace(&self, next: AppSettings) -> Result<AppSettings> {
        self.persist(&next)?;

        match self.settings.write() {
            Ok(mut guard) => {
                *guard = next.clone();
            }
            Err(poisoned) => {
                warn!("Settings lock was poisoned while writing; forcing update");
                let mut guard = poisoned.into_inner();
                *guard = next.clone();
            }
        }

        Ok(next)
    }

    pub fn merge(
        &self,
        location: Option<Option<GeoPosition>>,
        modules: Option<HashMap<String, Value>>,
    ) -> Result<AppSettings> {
        let mut current = self.current();

        if let Some(next_location) = location {
            current.location = next_location;
        }

        if let Some(next_modules) = modules {
            current.modules = next_modules;
        }

        self.replace(current)
    }

    fn persist(&self, settings: &AppSettings) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(settings)?;
        fs::write(&tmp_path, json)?;

        if self.path.exists() {
            let _ = fs::remove_file(&self.path);
        }

        fs::rename(&tmp_path, &self.path)?;
        info!("Saved settings to {}", self.path.display());
        Ok(())
    }

    fn load_from_disk(path: &Path) -> Result<AppSettings> {
        if !path.exists() {
            return Ok(AppSettings::default());
        }

        let raw = fs::read_to_string(path)?;
        let settings = serde_json::from_str(&raw)?;
        Ok(settings)
    }
}