use super::error::ModelError;
use super::loader::{LoadedModel, ModelLoader, ModelRuntime};
use super::types::{ModelHealthResponse, ModelLoadState, ModelRegistryEntry};
use candle_core::Tensor;
use candle_transformers::generation::LogitsProcessor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokenizers::Tokenizer;
use types::Config;

const CHAT_SYSTEM_PROMPT: &str = "You are Fyr Assistant, an offline help assistant embedded in Fyr — a self-hosted offline content server that provides maps, books (EPUB and ZIM archives), and local AI inference. You run entirely on the user's own device without internet access. Answer in the same language as the user. Be direct, avoid repeating the user's prompt, and do not invent hidden instructions or internal reasoning. If the answer is uncertain, say so briefly.";

#[derive(Clone, Copy)]
enum PromptFormat {
    ChatMl,
    Llama3,
    Phi3,
}

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

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
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

        std::fs::rename(&source_path, &target_path)
            .or_else(|_| {
                std::fs::copy(&source_path, &target_path)
                    .map(|_| ())
                    .and_then(|_| std::fs::remove_file(&source_path))
            })
            .map_err(|e| ModelError::ImportFailed(e.to_string()))?;

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
                states.insert(
                    filename.to_string(),
                    (ModelLoadState::Ready, validation_reason),
                );
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
            loaded: loaded_model
                .map(|model| model.supports_inference())
                .unwrap_or(false),
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
        num_ctx: usize,
        history: Vec<(String, String)>,
        app_context: String,
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
                spawn_quantized_inference(
                    model,
                    tokenizer,
                    eos_token_ids,
                    tx,
                    format_chat_prompt(&history, &prompt, &app_context),
                    temperature,
                    max_tokens,
                    num_ctx,
                    |model| model.clear_kv_cache(),
                    |model, input, index_pos| model.forward(input, index_pos),
                );
            }
            ModelRuntime::QuantizedLlama {
                model,
                tokenizer,
                eos_token_ids,
            } => {
                spawn_quantized_inference(
                    model,
                    tokenizer,
                    eos_token_ids,
                    tx,
                    format_prompt(&history, &prompt, &app_context, PromptFormat::Llama3),
                    temperature,
                    max_tokens,
                    num_ctx,
                    |model| model.clear_kv_cache(),
                    |model, input, index_pos| model.forward(input, index_pos),
                );
            }
            ModelRuntime::QuantizedPhi {
                model,
                tokenizer,
                eos_token_ids,
            } => {
                spawn_quantized_inference(
                    model,
                    tokenizer,
                    eos_token_ids,
                    tx,
                    format_prompt(&history, &prompt, &app_context, PromptFormat::Phi3),
                    temperature,
                    max_tokens,
                    num_ctx,
                    |_| {},
                    |model, input, index_pos| model.forward(input, index_pos),
                );
            }
            ModelRuntime::QuantizedPhi3 {
                model,
                tokenizer,
                eos_token_ids,
            } => {
                spawn_quantized_inference(
                    model,
                    tokenizer,
                    eos_token_ids,
                    tx,
                    format_prompt(&history, &prompt, &app_context, PromptFormat::Phi3),
                    temperature,
                    max_tokens,
                    num_ctx,
                    |_| {},
                    |model, input, index_pos| model.forward(input, index_pos),
                );
            }
            ModelRuntime::ValidationOnly { reason } => {
                return Err(ModelError::InferenceFailed(reason.unwrap_or_else(|| {
                    "Inference is not implemented for this loaded model.".to_string()
                })));
            }
        }

        Ok(rx)
    }
}

fn spawn_quantized_inference<M, FReset, FForward>(
    model: Arc<std::sync::Mutex<M>>,
    tokenizer: Arc<Tokenizer>,
    eos_token_ids: Vec<u32>,
    tx: mpsc::Sender<String>,
    formatted_prompt: String,
    temperature: f64,
    max_tokens: usize,
    num_ctx: usize,
    reset_cache: FReset,
    forward: FForward,
) where
    M: Send + 'static,
    FReset: Fn(&mut M) + Send + 'static,
    FForward: Fn(&mut M, &Tensor, usize) -> candle_core::Result<Tensor> + Send + 'static,
{
    const PREFILL_CHUNK_TOKENS: usize = 256;

    tokio::task::spawn_blocking(move || {
        let send_error = |message: String, tx: &mpsc::Sender<String>| {
            let _ = tx.blocking_send(message);
        };

        let encoding = match tokenizer.encode(formatted_prompt, true) {
            Ok(encoding) => encoding,
            Err(error) => {
                send_error(format!("Tokenizer error: {error}"), &tx);
                return;
            }
        };

        let mut token_ids = trim_context_window(encoding.get_ids(), num_ctx);
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

        reset_cache(&mut model);

        let device = candle_core::Device::Cpu;
        let seed = 42;
        let mut sampler = LogitsProcessor::new(seed, Some(temperature), None);
        let mut index_pos = 0usize;
        let mut generated_ids: Vec<u32> = Vec::new();
        let mut emitted_len = 0usize;
        let mut last_logits = None;

        // Prefill the KV cache in chunks to avoid very large one-shot tensors/masks on long prompts.
        for chunk in token_ids.chunks(PREFILL_CHUNK_TOKENS) {
            let input = match Tensor::new(chunk, &device).and_then(|tensor| tensor.unsqueeze(0)) {
                Ok(tensor) => tensor,
                Err(error) => {
                    send_error(format!("Tensor setup failed: {error}"), &tx);
                    return;
                }
            };

            let logits = match forward(&mut model, &input, index_pos) {
                Ok(logits) => logits,
                Err(error) => {
                    send_error(format!("Inference failed: {error}"), &tx);
                    return;
                }
            };

            last_logits = Some(match logits.squeeze(0) {
                Ok(logits) => logits,
                Err(_) => logits,
            });

            index_pos += chunk.len();
        }

        for _ in 0..max_tokens {
            let Some(logits) = last_logits.take() else {
                send_error("Inference failed: no logits available after prompt prefill.".to_string(), &tx);
                return;
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

            generated_ids.push(next_token);
            let decoded_text = match tokenizer.decode(&generated_ids, true) {
                Ok(text) => text,
                Err(error) => {
                    send_error(format!("Decode failed: {error}"), &tx);
                    return;
                }
            };

            token_ids.push(next_token);

            if is_repeating(&token_ids) {
                break;
            }

            let stop_at = first_role_marker_index(&decoded_text);
            let emit_text = if let Some(stop_at) = stop_at {
                &decoded_text[..stop_at]
            } else {
                &decoded_text
            };

            if emit_text.len() > emitted_len {
                let delta = &emit_text[emitted_len..];
                if !delta.is_empty() && tx.blocking_send(delta.to_string()).is_err() {
                    return;
                }
                emitted_len = emit_text.len();
            }

            if stop_at.is_some() {
                break;
            }

            let input = match Tensor::new(&[next_token], &device).and_then(|tensor| tensor.unsqueeze(0)) {
                Ok(tensor) => tensor,
                Err(error) => {
                    send_error(format!("Tensor setup failed: {error}"), &tx);
                    return;
                }
            };

            let logits = match forward(&mut model, &input, index_pos) {
                Ok(logits) => logits,
                Err(error) => {
                    send_error(format!("Inference failed: {error}"), &tx);
                    return;
                }
            };

            last_logits = Some(match logits.squeeze(0) {
                Ok(logits) => logits,
                Err(_) => logits,
            });
            index_pos += 1;
        }
    });
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

fn format_chat_prompt(history: &[(String, String)], prompt: &str, app_context: &str) -> String {
    format_prompt(history, prompt, app_context, PromptFormat::ChatMl)
}

fn format_prompt(
    history: &[(String, String)],
    prompt: &str,
    app_context: &str,
    prompt_format: PromptFormat,
) -> String {
    let system_block = if app_context.is_empty() {
        CHAT_SYSTEM_PROMPT.to_string()
    } else {
        format!("{}\n\n{}", CHAT_SYSTEM_PROMPT, app_context)
    };

    match prompt_format {
        PromptFormat::ChatMl => format_chatml_prompt(history, prompt, &system_block),
        PromptFormat::Llama3 => format_llama3_prompt(history, prompt, &system_block),
        PromptFormat::Phi3 => format_phi3_prompt(history, prompt, &system_block),
    }
}

fn format_chatml_prompt(history: &[(String, String)], prompt: &str, system_block: &str) -> String {
    let mut output = format!(
        "<|im_start|>system\n{}\n<|im_end|>\n",
        system_block
    );

    for (role, text) in history {
        let role_tag = match role.as_str() {
            "assistant" => "assistant",
            _ => "user",
        };
        output.push_str(&format!(
            "<|im_start|>{}\n{}\n<|im_end|>\n",
            role_tag,
            text.trim()
        ));
    }

    output.push_str(&format!(
        "<|im_start|>user\n{}\n<|im_end|>\n<|im_start|>assistant\n",
        prompt.trim()
    ));

    output
}

fn format_llama3_prompt(history: &[(String, String)], prompt: &str, system_block: &str) -> String {
    let mut output = String::from("<|begin_of_text|>");
    output.push_str(&format!(
        "<|start_header_id|>system<|end_header_id|>\n\n{}<|eot_id|>",
        system_block
    ));

    for (role, text) in history {
        let role_tag = match role.as_str() {
            "assistant" => "assistant",
            _ => "user",
        };
        output.push_str(&format!(
            "<|start_header_id|>{}<|end_header_id|>\n\n{}<|eot_id|>",
            role_tag,
            text.trim()
        ));
    }

    output.push_str(&format!(
        "<|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
        prompt.trim()
    ));

    output
}

fn format_phi3_prompt(history: &[(String, String)], prompt: &str, system_block: &str) -> String {
    let mut output = format!("<|system|>\n{}\n<|end|>\n", system_block);

    for (role, text) in history {
        let role_tag = match role.as_str() {
            "assistant" => "assistant",
            _ => "user",
        };
        output.push_str(&format!("<|{}|>\n{}\n<|end|>\n", role_tag, text.trim()));
    }

    output.push_str(&format!(
        "<|user|>\n{}\n<|end|>\n<|assistant|>\n",
        prompt.trim()
    ));

    output
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
    const CONTROL_MARKERS: [&str; 8] = [
        "<|im_start|>",
        "<|im_end|>",
        "<|start_header_id|>",
        "<|end_header_id|>",
        "<|eot_id|>",
        "<|user|>",
        "<|assistant|>",
        "<|end|>",
    ];

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

    let chatml_stop = CONTROL_MARKERS
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

fn trim_context_window(tokens: &[u32], num_ctx: usize) -> Vec<u32> {
    if tokens.len() <= num_ctx {
        return tokens.to_vec();
    }

    tokens[tokens.len() - num_ctx..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::{
        first_role_marker_index, format_chat_prompt, format_prompt, is_repeating,
        trim_context_window, PromptFormat,
    };

    // --- format_chat_prompt ---

    #[test]
    fn format_chat_prompt_single_turn_has_no_history() {
        let result = format_chat_prompt(&[], "Hello?", "");
        assert!(result.contains("<|im_start|>user\nHello?\n<|im_end|>"));
        assert!(result.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn format_chat_prompt_includes_history_turns() {
        let history = vec![
            ("user".to_string(), "Hi".to_string()),
            ("assistant".to_string(), "Hello!".to_string()),
        ];
        let result = format_chat_prompt(&history, "How are you?", "");
        assert!(result.contains("<|im_start|>user\nHi\n<|im_end|>"));
        assert!(result.contains("<|im_start|>assistant\nHello!\n<|im_end|>"));
        assert!(result.contains("<|im_start|>user\nHow are you?\n<|im_end|>"));
        assert!(result.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn format_chat_prompt_unknown_role_defaults_to_user() {
        let history = vec![("system".to_string(), "ignored".to_string())];
        let result = format_chat_prompt(&history, "Hello", "");
        assert!(result.contains("<|im_start|>user\nignored\n<|im_end|>"));
    }

    #[test]
    fn format_chat_prompt_embeds_app_context_in_system_block() {
        let ctx = "Current local content library:\n- Maps (1): world.pmtiles";
        let result = format_chat_prompt(&[], "What maps are loaded?", ctx);
        assert!(result.contains(ctx));
        assert!(result.starts_with("<|im_start|>system\n"));
        assert!(result.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn format_chat_prompt_empty_app_context_omits_extra_newline() {
        let without = format_chat_prompt(&[], "Hi", "");
        let with_ctx = format_chat_prompt(&[], "Hi", "Context: something");
        // With context must be longer
        assert!(with_ctx.len() > without.len());
        // Without context should not contain double-newline separator
        assert!(!without.contains("\n\nCurrent"));
    }

    #[test]
    fn format_llama_prompt_uses_llama3_headers() {
        let result = format_prompt(&[], "Hello", "", PromptFormat::Llama3);
        assert!(result.starts_with("<|begin_of_text|><|start_header_id|>system<|end_header_id|>"));
        assert!(result.contains("<|start_header_id|>user<|end_header_id|>\n\nHello<|eot_id|>"));
        assert!(result.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn format_phi_prompt_uses_phi_chat_tags() {
        let history = vec![("assistant".to_string(), "Hi back".to_string())];
        let result = format_prompt(&history, "Hello", "", PromptFormat::Phi3);
        assert!(result.starts_with("<|system|>\n"));
        assert!(result.contains("<|assistant|>\nHi back\n<|end|>"));
        assert!(result.ends_with("<|assistant|>\n"));
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
    fn role_marker_detects_llama_control_tokens() {
        let text = "Visible text<|eot_id|><|start_header_id|>assistant";
        assert_eq!(first_role_marker_index(text), Some(12));
    }

    #[test]
    fn role_marker_detects_phi_control_tokens() {
        let text = "Visible text<|assistant|>";
        assert_eq!(first_role_marker_index(text), Some(12));
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

    #[test]
    fn trim_context_window_keeps_tail_when_prompt_exceeds_limit() {
        let tokens: Vec<u32> = (0..10).collect();
        assert_eq!(trim_context_window(&tokens, 4), vec![6, 7, 8, 9]);
    }

    #[test]
    fn trim_context_window_keeps_full_prompt_when_under_limit() {
        let tokens: Vec<u32> = (0..4).collect();
        assert_eq!(trim_context_window(&tokens, 8), tokens);
    }
}
