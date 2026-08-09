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
#[cfg(test)]
mod tests {
    use super::SettingsManager;
    use serde_json::json;
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use types::{AppSettings, GeoPosition};

    fn test_data_dir() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("fyr-settings-tests-{unique}"))
    }

    #[test]
    fn merge_updates_only_requested_fields() {
        let data_dir = test_data_dir();
        fs::create_dir_all(&data_dir).expect("create test dir");
        let manager = SettingsManager::new(&data_dir);

        let mut modules = HashMap::new();
        modules.insert("assistant".to_string(), json!({ "num_ctx": 2048 }));
        let initial = AppSettings {
            location: Some(GeoPosition {
                latitude: 1.0,
                longitude: 2.0,
                label: Some("initial".to_string()),
            }),
            modules: modules.clone(),
        };
        manager.replace(initial).expect("seed settings");

        let merged = manager
            .merge(
                Some(Some(GeoPosition {
                    latitude: 10.0,
                    longitude: 20.0,
                    label: Some("updated".to_string()),
                })),
                None,
            )
            .expect("merge settings");

        assert_eq!(
            merged.location,
            Some(GeoPosition {
                latitude: 10.0,
                longitude: 20.0,
                label: Some("updated".to_string()),
            })
        );
        assert_eq!(merged.modules, modules);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn replace_persists_and_reloads_from_disk() {
        let data_dir = test_data_dir();
        fs::create_dir_all(&data_dir).expect("create test dir");
        let manager = SettingsManager::new(&data_dir);

        let mut modules = HashMap::new();
        modules.insert(
            "downloads".to_string(),
            json!({ "request_timeout_seconds": 900 }),
        );
        let expected = AppSettings {
            location: Some(GeoPosition {
                latitude: 12.34,
                longitude: 56.78,
                label: Some("persisted".to_string()),
            }),
            modules,
        };

        manager.replace(expected.clone()).expect("persist settings");

        let reloaded = SettingsManager::new(&data_dir).current();
        assert_eq!(reloaded, expected);

        let _ = fs::remove_dir_all(data_dir);
    }
}
