use super::error::ModelError;
use super::loader::{LoadedModel, ModelLoader};
use super::types::{ModelHealthResponse, ModelLoadState, ModelRegistryEntry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use types::Config;

#[derive(Clone)]
pub struct ModelManager {
    config: Arc<Config>,
    loaded: Arc<RwLock<HashMap<String, LoadedModel>>>,
    states: Arc<RwLock<HashMap<String, (ModelLoadState, Option<String>)>>>,
}

impl ModelManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            loaded: Arc::new(RwLock::new(HashMap::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn list_models(&self) -> Result<Vec<ModelRegistryEntry>, ModelError> {
        let mut entries = Vec::new();
        let models_dir = self.config.models_dir();

        if !models_dir.exists() {
            return Ok(entries);
        }

        let loaded = self.loaded.read().await;
        let states = self.states.read().await;

        for entry in std::fs::read_dir(&models_dir)
            .map_err(|e| ModelError::Internal(format!("read_dir failed: {}", e)))?
        {
            let entry = entry.map_err(|e| ModelError::Internal(e.to_string()))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
            if !ext.eq_ignore_ascii_case("gguf") {
                continue;
            }

            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

            let loaded_flag = loaded.contains_key(&filename);
            let (state, error) = states
                .get(&filename)
                .cloned()
                .unwrap_or((ModelLoadState::Unloaded, None));

            entries.push(ModelRegistryEntry {
                filename,
                size_bytes,
                loaded: loaded_flag,
                state,
                error,
            });
        }

        entries.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(entries)
    }

    pub async fn import_model(&self, filename: &str, source: &str) -> Result<PathBuf, ModelError> {
        if !filename.to_ascii_lowercase().ends_with(".gguf") {
            return Err(ModelError::InvalidExtension(filename.to_string()));
        }

        let source_dir = match source {
            "inbox" => self.config.inbox_dir(),
            "misc" => self.config.misc_dir(),
            other => {
                return Err(ModelError::ImportFailed(format!(
                    "unsupported source directory: {}",
                    other
                )))
            }
        };

        let source_path = source_dir.join(filename);
        if !source_path.exists() {
            return Err(ModelError::NotFound(source_path.display().to_string()));
        }

        let mut file = std::fs::File::open(&source_path)
            .map_err(|e| ModelError::ImportFailed(e.to_string()))?;
        let mut magic = [0u8; 4];
        use std::io::Read;
        file.read_exact(&mut magic)
            .map_err(|e| ModelError::InvalidMagic(e.to_string()))?;
        if magic != *b"GGUF" {
            return Err(ModelError::InvalidMagic(source_path.display().to_string()));
        }

        let target_path = self.config.models_dir().join(filename);
        if source_path == target_path {
            return Ok(target_path);
        }

        std::fs::rename(&source_path, &target_path).or_else(|_| {
            std::fs::copy(&source_path, &target_path)
                .map(|_| ())
                .and_then(|_| std::fs::remove_file(&source_path))
        }).map_err(|e| ModelError::ImportFailed(e.to_string()))?;

        Ok(target_path)
    }

    pub async fn load_model(&self, filename: &str) -> Result<(), ModelError> {
        let model_path = self.config.models_dir().join(filename);
        {
            let mut states = self.states.write().await;
            states.insert(filename.to_string(), (ModelLoadState::Loading, None));
        }

        let loaded = ModelLoader::load(&model_path);

        match loaded {
            Ok(model) => {
                {
                    let mut loaded_models = self.loaded.write().await;
                    loaded_models.insert(filename.to_string(), model);
                }
                let mut states = self.states.write().await;
                states.insert(filename.to_string(), (ModelLoadState::Ready, None));
                Ok(())
            }
            Err(err) => {
                let mut states = self.states.write().await;
                states.insert(
                    filename.to_string(),
                    (ModelLoadState::Error, Some(err.to_string())),
                );
                Err(err)
            }
        }
    }

    pub async fn unload_model(&self, filename: &str) {
        let mut loaded = self.loaded.write().await;
        loaded.remove(filename);
        let mut states = self.states.write().await;
        states.insert(filename.to_string(), (ModelLoadState::Unloaded, None));
    }

    pub async fn health(&self, filename: &str) -> ModelHealthResponse {
        let loaded = self.loaded.read().await;
        let states = self.states.read().await;

        let loaded_model = loaded.get(filename);
        let (state, error) = states
            .get(filename)
            .cloned()
            .unwrap_or((ModelLoadState::Unloaded, None));

        ModelHealthResponse {
            filename: filename.to_string(),
            loaded: loaded_model.is_some(),
            state,
            architecture: loaded_model.and_then(|m| m.metadata.architecture.clone()),
            has_tokenizer_metadata: loaded_model
                .map(|m| m.metadata.has_tokenizer_metadata)
                .unwrap_or(false),
            error,
        }
    }

    pub async fn infer_stream(
        &self,
        filename: &str,
        prompt: String,
        temperature: f64,
        max_tokens: usize,
    ) -> Result<mpsc::Receiver<String>, ModelError> {
        let loaded = self.loaded.read().await;
        let Some(model) = loaded.get(filename) else {
            return Err(ModelError::NotLoaded(filename.to_string()));
        };

        let model_name = model.metadata.filename.clone();
        drop(loaded);

        let (tx, rx) = mpsc::channel::<String>(32);

        tokio::spawn(async move {
            let intro = format!(
                "[offline:{} temp={:.1}] ",
                model_name,
                temperature
            );

            let mut words: Vec<String> = prompt
                .split_whitespace()
                .map(|w| w.to_string())
                .collect();

            if words.is_empty() {
                words.push("(empty)".to_string());
            }

            let mut generated = Vec::new();
            generated.push("Local".to_string());
            generated.push("response:".to_string());
            generated.extend(words);

            let mut count = 0usize;
            if tx.send(intro).await.is_err() {
                return;
            }

            for token in generated {
                if count >= max_tokens {
                    break;
                }
                let payload = format!("{} ", token);
                if tx.send(payload).await.is_err() {
                    break;
                }
                count += 1;
                tokio::time::sleep(std::time::Duration::from_millis(18)).await;
            }
        });

        Ok(rx)
    }
}
