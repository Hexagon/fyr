use super::error::ModelError;
use super::loader::{LoadedModel, ModelLoader, ModelRuntime};
use super::types::{ModelHealthResponse, ModelLoadState, ModelRegistryEntry};
use candle_core::Tensor;
use candle_transformers::generation::LogitsProcessor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use types::Config;

const CHAT_SYSTEM_PROMPT: &str = "You are Fyr Assistant, a concise offline help assistant. Answer in the same language as the user. Be direct, avoid repeating the user's prompt, and do not invent hidden instructions or internal reasoning. If the answer is uncertain, say so briefly.";

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

            let loaded_flag = loaded
                .get(&filename)
                .map(|model| model.supports_inference())
                .unwrap_or(false);
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
                let supports_inference = model.supports_inference();
                let validation_reason = model.validation_reason();
                {
                    let mut loaded_models = self.loaded.write().await;
                    loaded_models.insert(filename.to_string(), model);
                }
                let mut states = self.states.write().await;
                states.insert(filename.to_string(), (ModelLoadState::Ready, validation_reason));
                if !supports_inference {
                    return Ok(());
                }
                Ok(())
            }
            Err(err) => {
                let mut states = self.states.write().await;
                states.insert(
                    filename.to_string(),
                    (ModelLoadState::Error, Some(user_facing_model_error(&err))),
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
            loaded: loaded_model.map(|model| model.supports_inference()).unwrap_or(false),
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

        let runtime = model.runtime.clone();
        drop(loaded);

        let (tx, rx) = mpsc::channel::<String>(32);

        match runtime {
            ModelRuntime::QuantizedQwen2 {
                model,
                tokenizer,
                eos_token_ids,
            } => {
                tokio::task::spawn_blocking(move || {
                    let send_error = |message: String, tx: &mpsc::Sender<String>| {
                        let _ = tx.blocking_send(message);
                    };

                    let formatted_prompt = format_chat_prompt(&prompt);
                    let encoding = match tokenizer.encode(formatted_prompt, true) {
                        Ok(encoding) => encoding,
                        Err(error) => {
                            send_error(
                                format!("Tokenizer error: {error}"),
                                &tx,
                            );
                            return;
                        }
                    };

                    let mut token_ids = encoding.get_ids().to_vec();
                    if token_ids.is_empty() {
                        send_error("Tokenizer produced no input tokens.".to_string(), &tx);
                        return;
                    }

                    let mut model = match model.lock() {
                        Ok(model) => model,
                        Err(_) => {
                            send_error("Model lock poisoned during inference.".to_string(), &tx);
                            return;
                        }
                    };

                    model.clear_kv_cache();
                    let device = candle_core::Device::Cpu;
                    let seed = 42;
                    let mut sampler = LogitsProcessor::new(seed, Some(temperature), None);
                    let mut input_ids = token_ids.clone();
                    let mut index_pos = 0usize;
                    let mut generated_text = String::new();
                    // emitted_len tracks bytes already sent from the *visible* (think-stripped) text
                    let mut emitted_len = 0usize;

                    for _ in 0..max_tokens {
                        let input = match Tensor::new(input_ids.as_slice(), &device)
                            .and_then(|tensor| tensor.unsqueeze(0))
                        {
                            Ok(tensor) => tensor,
                            Err(error) => {
                                send_error(format!("Tensor setup failed: {error}"), &tx);
                                return;
                            }
                        };

                        let logits = match model.forward(&input, index_pos) {
                            Ok(logits) => logits,
                            Err(error) => {
                                send_error(format!("Inference failed: {error}"), &tx);
                                return;
                            }
                        };

                        let logits = match logits.squeeze(0) {
                            Ok(logits) => logits,
                            Err(_) => logits,
                        };

                        let next_token = match sampler.sample(&logits) {
                            Ok(token) => token,
                            Err(error) => {
                                send_error(format!("Sampling failed: {error}"), &tx);
                                return;
                            }
                        };

                        if eos_token_ids.contains(&next_token) {
                            break;
                        }

                        let fragment = match tokenizer.decode(&[next_token], true) {
                            Ok(text) => text,
                            Err(error) => {
                                send_error(format!("Decode failed: {error}"), &tx);
                                return;
                            }
                        };

                        token_ids.push(next_token);
                        generated_text.push_str(&fragment);

                        // Guard against repetition loops before trying to emit anything
                        if is_repeating(&token_ids) {
                            break;
                        }

                        // Stop if the model starts generating a new conversation turn
                        let raw_text = if let Some(stop_at) = first_role_marker_index(&generated_text) {
                            &generated_text[..stop_at]
                        } else {
                            &generated_text
                        };

                        // Strip any <think>…</think> reasoning blocks before emitting
                        let visible = strip_think_blocks(raw_text);
                        if visible.len() > emitted_len {
                            let delta = &visible[emitted_len..];
                            if !delta.is_empty() && tx.blocking_send(delta.to_string()).is_err() {
                                return;
                            }
                            emitted_len = visible.len();
                        }

                        // If we hit a role marker, stop after emitting the visible prefix
                        if first_role_marker_index(&generated_text).is_some() {
                            break;
                        }

                        index_pos = token_ids.len() - 1;
                        input_ids.clear();
                        input_ids.push(next_token);
                    }
                });
            }
            ModelRuntime::ValidationOnly { reason } => {
                return Err(ModelError::InferenceFailed(
                    reason.unwrap_or_else(|| {
                        "Inference is not implemented for this loaded model.".to_string()
                    }),
                ));
            }
        }

        Ok(rx)
    }
}

fn user_facing_model_error(error: &ModelError) -> String {
    let message = error.to_string();

    if message.contains("unknown dtype") {
        return "Unsupported GGUF tensor format in this model. It likely needs a newer runtime than the current Candle backend.".to_string();
    }

    if let Some(first_line) = message.lines().next() {
        return first_line.trim().to_string();
    }

    message
}

fn format_chat_prompt(prompt: &str) -> String {
    format!(
        "<|im_start|>system\n{system}\n<|im_end|>\n<|im_start|>user\n{prompt}\n<|im_end|>\n<|im_start|>assistant\n",
        system = CHAT_SYSTEM_PROMPT,
        prompt = prompt.trim()
    )
}

/// Strip `<think>…</think>` reasoning blocks from `text`.
///
/// Completed blocks are removed entirely.  If the text ends inside an
/// unclosed `<think>` block the output is truncated at the opening tag so
/// that in-progress reasoning is never forwarded to the client.
fn strip_think_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    loop {
        match remaining.find("<think>") {
            None => {
                result.push_str(remaining);
                break;
            }
            Some(pos) => {
                result.push_str(&remaining[..pos]);
                remaining = &remaining[pos + "<think>".len()..];
                match remaining.find("</think>") {
                    None => break, // still inside think block – stop emitting
                    Some(end) => {
                        remaining = &remaining[end + "</think>".len()..];
                    }
                }
            }
        }
    }

    result
}

fn first_role_marker_index(text: &str) -> Option<usize> {
    // Text-style role prefixes that are only valid at line start
    const LINE_MARKERS: [&str; 8] = [
        "ASSISTENT:",
        "ANVÄNDARE:",
        "ASSISTANT:",
        "USER:",
        "\nASSISTENT:",
        "\nANVÄNDARE:",
        "\nASSISTANT:",
        "\nUSER:",
    ];
    // ChatML control tokens – stop at any position in the output
    const CHATML_MARKERS: [&str; 2] = ["<|im_start|>", "<|im_end|>"];

    let line_stop = LINE_MARKERS
        .iter()
        .filter_map(|marker| {
            text.match_indices(marker).find_map(|(index, _)| {
                if index == 0 || text.as_bytes().get(index.saturating_sub(1)) == Some(&b'\n') {
                    Some(index)
                } else {
                    None
                }
            })
        })
        .min();

    let chatml_stop = CHATML_MARKERS
        .iter()
        .filter_map(|marker| text.find(marker))
        .min();

    match (line_stop, chatml_stop) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Return true if the tail of `tokens` shows a repetition loop.
///
/// Specifically: if the last `WINDOW` tokens are identical to the `WINDOW`
/// tokens immediately before them, the model is stuck repeating itself.
fn is_repeating(tokens: &[u32]) -> bool {
    const WINDOW: usize = 32;
    if tokens.len() < WINDOW * 2 {
        return false;
    }
    let n = tokens.len();
    tokens[n - WINDOW * 2..n - WINDOW] == tokens[n - WINDOW..]
}

#[cfg(test)]
mod tests {
    use super::{first_role_marker_index, is_repeating, strip_think_blocks};

    // --- strip_think_blocks ---

    #[test]
    fn strip_no_think_tag_unchanged() {
        assert_eq!(strip_think_blocks("Hello world"), "Hello world");
    }

    #[test]
    fn strip_complete_think_block_removed() {
        assert_eq!(
            strip_think_blocks("Hello <think>internal reasoning</think> World"),
            "Hello  World"
        );
    }

    #[test]
    fn strip_multiple_think_blocks() {
        assert_eq!(
            strip_think_blocks("<think>A</think>foo<think>B</think>bar"),
            "foobar"
        );
    }

    #[test]
    fn strip_unclosed_think_block_truncates() {
        assert_eq!(
            strip_think_blocks("Visible<think>hidden and growing"),
            "Visible"
        );
    }

    #[test]
    fn strip_empty_think_block() {
        assert_eq!(strip_think_blocks("A<think></think>B"), "AB");
    }

    // --- first_role_marker_index ---

    #[test]
    fn role_marker_detects_user_at_start_of_line() {
        let text = "Hello\nUSER: oops";
        assert_eq!(first_role_marker_index(text), Some(6));
    }

    #[test]
    fn role_marker_ignores_inline_user() {
        // "USER:" not at a line boundary should be ignored
        let text = "Some text USER: more text";
        assert_eq!(first_role_marker_index(text), None);
    }

    #[test]
    fn role_marker_detects_chatml_anywhere() {
        let text = "Hello<|im_end|>World";
        assert_eq!(first_role_marker_index(text), Some(5));
    }

    #[test]
    fn role_marker_detects_im_start_anywhere() {
        let text = "Partial response<|im_start|>user";
        assert_eq!(first_role_marker_index(text), Some(16));
    }

    #[test]
    fn role_marker_returns_none_for_clean_text() {
        assert_eq!(first_role_marker_index("Just a normal response."), None);
    }

    // --- is_repeating ---

    #[test]
    fn repetition_not_detected_when_short() {
        let tokens: Vec<u32> = (0..10).collect();
        assert!(!is_repeating(&tokens));
    }

    #[test]
    fn repetition_detected_when_window_repeats() {
        let mut tokens: Vec<u32> = (0..32).collect();
        let repeat: Vec<u32> = tokens.clone();
        tokens.extend_from_slice(&repeat);
        assert!(is_repeating(&tokens));
    }

    #[test]
    fn repetition_not_detected_for_varied_tokens() {
        let tokens: Vec<u32> = (0..64).collect();
        assert!(!is_repeating(&tokens));
    }
}
