//! Prompt budgeting: fit the system block, library sources and conversation
//! history into the model's context window.
//!
//! Everything here is pure and model-agnostic: callers pass a `format`
//! closure (the model's chat template) and a `count` closure (the loaded
//! tokenizer), so the logic is unit-testable without a GGUF model.

use super::retrieval::Passage;

/// A source must keep at least this many tokens of text to be worth citing.
pub const MIN_SOURCE_TOKENS: usize = 32;

const LIBRARY_HEADER: &str = "Library sources (excerpts from the user's offline library, numbered):";
const LIBRARY_INSTRUCTIONS: &str = "Answer using the library sources above when they are relevant. Cite the sources you use inline with their number in square brackets, for example [1] or [2]; only cite numbers listed above. If the sources do not contain the answer, say clearly that the library does not cover it before answering from general knowledge.";
const LIBRARY_EMPTY: &str = "Library search: no passages from the user's offline library could be included for this question. If the user expects an answer from their library, say that no matching sources were found.";

/// Tokens available for the prompt once room for `max_tokens` of output is
/// reserved. At most half of the window is reserved, so a large `max_tokens`
/// on a small window still leaves space for the question.
pub fn input_token_budget(num_ctx: usize, max_tokens: usize) -> usize {
    let reserve = max_tokens.min(num_ctx / 2);
    num_ctx.saturating_sub(reserve).max(1)
}

/// One numbered source as it appears in the system block.
pub fn format_source_entry(n: usize, passage: &Passage) -> String {
    format!("[{}] {} — {}\n{}", n, passage.title, passage.file, passage.text.trim())
}

/// The library part of the system block: numbered sources plus citation
/// instructions, or a short note when nothing could be included.
pub fn library_block(sources: &[Passage]) -> String {
    if sources.is_empty() {
        return LIBRARY_EMPTY.to_string();
    }
    let entries = sources
        .iter()
        .enumerate()
        .map(|(i, passage)| format_source_entry(i + 1, passage))
        .collect::<Vec<_>>()
        .join("\n\n");
    library_frame(&entries)
}

fn library_frame(entries: &str) -> String {
    format!("{LIBRARY_HEADER}\n\n{entries}\n\n{LIBRARY_INSTRUCTIONS}")
}

/// Extra system context: the app catalog summary plus, when the library was
/// requested, the library block.
pub fn compose_system_context(app_context: &str, library: Option<&[Passage]>) -> String {
    join_context(app_context, library.map(library_block))
}

fn join_context(app_context: &str, library: Option<String>) -> String {
    match (app_context.trim().is_empty(), library) {
        (true, None) => String::new(),
        (false, None) => app_context.to_string(),
        (true, Some(block)) => block,
        (false, Some(block)) => format!("{app_context}\n\n{block}"),
    }
}

/// Trim `text` so that it encodes to at most `max_tokens` tokens.
/// Returns `None` when no meaningful excerpt fits.
pub fn trim_text_to_tokens(text: &str, max_tokens: usize, count: &impl Fn(&str) -> usize) -> Option<String> {
    let tokens = count(text);
    if tokens <= max_tokens {
        return Some(text.to_string());
    }
    if max_tokens == 0 || tokens == 0 {
        return None;
    }
    let chars = text.chars().count();
    let mut target = chars * max_tokens / tokens;
    for _ in 0..6 {
        if target < 16 {
            return None;
        }
        let candidate = super::retrieval::trim_to_chars(text, target);
        if count(&candidate) <= max_tokens {
            return Some(candidate);
        }
        target = target * 3 / 4;
    }
    None
}

/// Result of [`fit_prompt`].
#[derive(Debug, Clone, PartialEq)]
pub struct FittedPrompt {
    pub text: String,
    /// Sources actually included (possibly trimmed), in citation order.
    /// `None` when the library was not requested.
    pub sources: Option<Vec<Passage>>,
    /// Number of history messages kept (the most recent ones).
    pub history_used: usize,
}

/// Fit the prompt into `budget` tokens.
///
/// Priority: system prompt and the user's question, then library sources
/// (up to two thirds of the remaining room when there is history), then as
/// much recent history as fits. Oldest history goes first; a kept history
/// never starts with an orphaned assistant reply. If even the bare prompt
/// does not fit, the bare prompt is returned and the caller's tail trimming
/// is the last resort (the pre-existing behaviour).
pub fn fit_prompt<F, C>(
    format: F,
    count: C,
    app_context: &str,
    history: &[(String, String)],
    library: Option<&[Passage]>,
    budget: usize,
) -> FittedPrompt
where
    F: Fn(&[(String, String)], &str) -> String,
    C: Fn(&str) -> usize,
{
    let render = |hist: &[(String, String)], sources: Option<&[Passage]>| {
        format(hist, &compose_system_context(app_context, sources))
    };

    let full = render(history, library);
    if count(&full) <= budget {
        return FittedPrompt {
            text: full,
            sources: library.map(<[Passage]>::to_vec),
            history_used: history.len(),
        };
    }

    let empty: &[Passage] = &[];
    let base_text = render(&[], library.map(|_| empty));
    // Estimate the fixed cost with the sources frame (header + citation
    // instructions) when there are sources to place, since that is what the
    // final prompt will carry.
    let base = match library {
        Some(sources) if !sources.is_empty() => {
            count(&format(&[], &join_context(app_context, Some(library_frame("")))))
        }
        _ => count(&base_text),
    };
    if base >= budget {
        return FittedPrompt {
            text: base_text,
            sources: library.map(|_| Vec::new()),
            history_used: 0,
        };
    }
    let remaining = budget - base;

    let mut kept: Vec<Passage> = Vec::new();
    if let Some(sources) = library {
        let source_budget = if history.is_empty() { remaining } else { remaining * 2 / 3 };
        let mut used = 0usize;
        for passage in sources {
            let n = kept.len() + 1;
            // +2 for the blank-line separator between entries.
            let cost = count(&format_source_entry(n, passage)) + 2;
            if used + cost <= source_budget {
                kept.push(passage.clone());
                used += cost;
                continue;
            }
            let header = Passage { text: String::new(), ..passage.clone() };
            let header_cost = count(&format_source_entry(n, &header)) + 2;
            let left = source_budget.saturating_sub(used + header_cost);
            if left >= MIN_SOURCE_TOKENS {
                if let Some(text) = trim_text_to_tokens(&passage.text, left, &count) {
                    kept.push(Passage { text, ..passage.clone() });
                }
            }
            break;
        }
    }

    let mut start = 0usize;
    loop {
        let hist = &history[start..];
        let text = render(hist, library.map(|_| kept.as_slice()));
        if count(&text) <= budget {
            return FittedPrompt {
                text,
                sources: library.map(|_| kept),
                history_used: hist.len(),
            };
        }
        if start < history.len() {
            start += 1;
            while start < history.len() && history[start].0 == "assistant" {
                start += 1;
            }
            continue;
        }
        if kept.pop().is_some() {
            continue;
        }
        return FittedPrompt {
            text,
            sources: library.map(|_| kept),
            history_used: 0,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whitespace word count stands in for a real tokenizer.
    fn words(text: &str) -> usize {
        text.split_whitespace().count()
    }

    fn fake_format(history: &[(String, String)], system: &str) -> String {
        let mut out = format!("SYSTEM base prompt {system}\n");
        for (role, text) in history {
            out.push_str(&format!("{role}: {text}\n"));
        }
        out.push_str("user: question here\nassistant:");
        out
    }

    fn passage(title: &str, words_count: usize) -> Passage {
        Passage {
            title: title.to_string(),
            file: format!("{}.md", title.to_lowercase()),
            zim_path: None,
            query: "x".into(),
            text: (0..words_count).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" "),
            score: 1.0,
        }
    }

    fn turn(role: &str, n: usize) -> (String, String) {
        (role.to_string(), (0..n).map(|_| "h").collect::<Vec<_>>().join(" "))
    }

    #[test]
    fn budget_reserves_output_tokens_but_at_most_half() {
        assert_eq!(input_token_budget(2048, 512), 1536);
        assert_eq!(input_token_budget(1024, 4096), 512);
        assert_eq!(input_token_budget(0, 10), 1);
    }

    #[test]
    fn library_block_numbers_sources_and_instructs_citations() {
        let block = library_block(&[passage("Water", 3), passage("Fire", 2)]);
        assert!(block.contains("[1] Water — water.md\nw0 w1 w2"));
        assert!(block.contains("[2] Fire — fire.md"));
        assert!(block.contains("[1] or [2]"));
        assert!(block.contains("do not contain the answer"));
        assert!(library_block(&[]).contains("no passages"));
    }

    #[test]
    fn system_context_combines_catalog_and_library() {
        assert_eq!(compose_system_context("", None), "");
        assert_eq!(compose_system_context("Catalog", None), "Catalog");
        let combined = compose_system_context("Catalog", Some(&[passage("Water", 1)]));
        assert!(combined.starts_with("Catalog\n\nLibrary sources"));
    }

    #[test]
    fn everything_fits_unchanged() {
        let history = vec![turn("user", 3), turn("assistant", 3)];
        let sources = vec![passage("Water", 10)];
        let fitted = fit_prompt(fake_format, words, "Catalog", &history, Some(&sources), 1000);
        assert_eq!(fitted.history_used, 2);
        assert_eq!(fitted.sources.as_ref().unwrap(), &sources);
        assert!(fitted.text.contains("[1] Water"));
        assert!(fitted.text.contains("user: h h h"));
    }

    #[test]
    fn without_library_old_path_only_trims_history() {
        let history = vec![turn("user", 50), turn("assistant", 50), turn("user", 5), turn("assistant", 5)];
        let fitted = fit_prompt(fake_format, words, "", &history, None, 40);
        assert_eq!(fitted.sources, None);
        assert_eq!(fitted.history_used, 2, "oldest turns dropped first");
        assert!(words(&fitted.text) <= 40);
        assert!(!fitted.text.contains("Library"));
    }

    #[test]
    fn kept_history_never_starts_with_assistant_reply() {
        let history = vec![turn("user", 30), turn("assistant", 3), turn("assistant", 3), turn("user", 3)];
        let fitted = fit_prompt(fake_format, words, "", &history, None, 30);
        assert_eq!(fitted.history_used, 1);
        assert!(fitted.text.contains("user: h h h\nuser: question"));
    }

    #[test]
    fn sources_are_trimmed_then_dropped_to_fit() {
        let sources = vec![passage("Alpha", 60), passage("Beta", 400), passage("Gamma", 10)];
        let budget = 200;
        let fitted = fit_prompt(fake_format, words, "", &[], Some(&sources), budget);
        let kept = fitted.sources.unwrap();
        assert!(words(&fitted.text) <= budget, "{}", words(&fitted.text));
        assert_eq!(kept[0], sources[0], "first source fits untouched");
        assert_eq!(kept.len(), 2, "second is trimmed, later ones are not added");
        assert!(kept[1].text.ends_with('…'));
        assert!(words(&kept[1].text) < 400);
        assert!(!fitted.text.contains("Gamma"));
    }

    #[test]
    fn sources_leave_room_for_history() {
        let sources = vec![passage("Alpha", 500)];
        let history = vec![turn("user", 20), turn("assistant", 20)];
        let fitted = fit_prompt(fake_format, words, "", &history, Some(&sources), 300);
        assert_eq!(fitted.history_used, 2, "{}", fitted.text);
        assert_eq!(fitted.sources.unwrap().len(), 1);
        assert!(words(&fitted.text) <= 300);
    }

    #[test]
    fn tiny_window_yields_bare_prompt_with_empty_sources() {
        let sources = vec![passage("Alpha", 50)];
        let fitted = fit_prompt(fake_format, words, "", &[turn("user", 5)], Some(&sources), 10);
        assert_eq!(fitted.sources, Some(Vec::new()));
        assert_eq!(fitted.history_used, 0);
        assert!(fitted.text.contains("no passages"));
    }

    #[test]
    fn trim_text_to_tokens_shrinks_or_gives_up() {
        let text = (0..100).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let trimmed = trim_text_to_tokens(&text, 20, &words).unwrap();
        assert!(words(&trimmed) <= 20);
        assert_eq!(trim_text_to_tokens("a b c", 5, &words).as_deref(), Some("a b c"));
        assert_eq!(trim_text_to_tokens(&text, 1, &words), None);
    }
}
