//! "Ask your library": lightweight keyword retrieval over the offline book
//! library, used to ground assistant answers in local content.
//!
//! The pipeline is deliberately simple and language-agnostic:
//!
//! 1. [`extract_keywords`] lowercases the prompt, splits on non-alphanumeric
//!    characters and drops short tokens and English/Swedish stop words.
//! 2. Candidate documents are gathered from `books/`:
//!    * ZIM archives: articles whose *title* matches the prompt, found with
//!      binary-search path lookups and a title-index prefix scan (plus a
//!      bounded linear scan for small archives only), so multi-gigabyte
//!      archives are never walked end to end.
//!    * Markdown files: split into heading sections and paragraphs.
//!    * EPUB files: spine documents split into block-level paragraphs.
//! 3. Paragraph-sized chunks are scored with a BM25-style term-frequency score
//!    (plus a small title bonus) and grouped per document; the best documents
//!    become numbered sources.
//!
//! Every phase checks a [`Budget`] (wall-clock deadline and bytes read) and
//! hard caps on archives, articles and files, so a huge library degrades to
//! fewer sources instead of stalling the request. Callers must run
//! [`retrieve`] on the blocking pool (`spawn_blocking`).
//!
//! Only bare filenames and archive-internal paths ever leave this module;
//! absolute host paths are never part of a [`Passage`] or [`SourceRef`].

use crate::library;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Default number of sources injected into the prompt.
pub const DEFAULT_SOURCES_LIMIT: usize = 4;
/// Upper bound for the `sources_limit` query parameter.
pub const MAX_SOURCES_LIMIT: usize = 8;

const MAX_KEYWORDS: usize = 10;
const MAX_TITLE_CANDIDATES: usize = 24;
const MIN_BLOCK_CHARS: usize = 25;
const MAX_EPUB_SPINE_ITEMS: usize = 400;
const MAX_TITLE_PREFIX_MATCHES: usize = 4;
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;
const TITLE_BONUS: f64 = 0.6;

/// English and Swedish stop words (lowercase). Kept small on purpose: the goal
/// is to drop glue words, not to do linguistics.
const STOP_WORDS: &[&str] = &[
    // English
    "the", "and", "for", "are", "but", "not", "you", "your", "yours", "all", "any", "can", "had",
    "has", "have", "her", "hers", "him", "his", "how", "its", "may", "our", "out", "she", "they",
    "them", "their", "theirs", "then", "than", "that", "this", "these", "those", "was", "were",
    "what", "when", "where", "which", "who", "whom", "whose", "why", "will", "with", "would",
    "could", "should", "shall", "about", "above", "after", "again", "against", "also", "been",
    "before", "being", "below", "between", "both", "does", "did", "doing", "done", "down",
    "during", "each", "few", "from", "further", "here", "into", "just", "more", "most", "much",
    "must", "only", "other", "over", "own", "same", "some", "such", "there", "through", "too",
    "under", "until", "very", "while", "yes", "yet", "one", "get", "got", "let", "make", "please",
    "tell", "explain", "describe", "know", "like", "give", "show", "want", "need", "use", "used",
    "using", "many", "way", "ways", "thing", "things", "something", "anything", "everything",
    "is", "am", "be", "do", "of", "to", "in", "on", "at", "by", "it", "an", "as", "or", "if", "me",
    "my", "we", "us", "he", "so", "no", "up",
    // Swedish
    "och", "att", "det", "som", "för", "med", "den", "har", "inte", "var", "ett", "till", "från",
    "kan", "jag", "han", "hon", "men", "vid", "nu", "när", "där", "här", "vad", "vem", "vilka",
    "vilken", "vilket", "hur", "varför", "eller", "också", "även", "efter", "under", "över",
    "mellan", "utan", "genom", "sig", "sin", "sitt", "sina", "dem", "deras", "vår", "våra", "vårt",
    "min", "mitt", "mina", "din", "ditt", "dina", "hans", "hennes", "dess", "denna", "detta",
    "dessa", "något", "några", "någon", "inga", "ingen", "inget", "alla", "allt", "mycket",
    "mer", "mest", "bara", "skulle", "ska", "kunde", "vara", "blir", "blev", "varit", "hade",
    "får", "fick", "gör", "göra", "gjorde", "berätta", "förklara", "beskriv", "beskriva", "snälla",
    "tack", "finns", "fanns", "om", "på", "av", "en", "är", "de", "så", "då", "ju", "nog", "mig",
    "dig", "oss", "er", "ni", "vi", "du", "ja", "nej",
];

/// Hard limits for one retrieval run.
#[derive(Debug, Clone)]
pub struct RetrievalLimits {
    /// Wall-clock budget for the whole run.
    pub time_budget: Duration,
    /// ZIM archives probed per run.
    pub max_zim_archives: usize,
    /// Articles fetched per ZIM archive.
    pub max_zim_articles_per_archive: usize,
    /// Archives with at most this many entries may be scanned linearly for
    /// title matches; larger archives only use indexed lookups.
    pub max_zim_linear_scan_entries: u32,
    /// Use the global search's cached ZIM title index for keyword-in-title
    /// candidates (and schedule its background build). Off only in tests
    /// that pin down the lookup stages without the shared process-wide cache.
    pub use_title_index: bool,
    /// Markdown/EPUB files opened per run.
    pub max_text_files: usize,
    /// Largest single file, ZIM article or EPUB chapter read, in bytes.
    pub max_item_bytes: u64,
    /// Total bytes read per run across all files and articles.
    pub max_total_bytes: u64,
    /// Character budget for one source's text.
    pub max_passage_chars: usize,
    /// Paragraphs combined into one document's source text.
    pub max_paragraphs_per_source: usize,
}

impl Default for RetrievalLimits {
    fn default() -> Self {
        Self {
            time_budget: Duration::from_millis(2500),
            max_zim_archives: 8,
            max_zim_articles_per_archive: 6,
            max_zim_linear_scan_entries: 50_000,
            use_title_index: true,
            max_text_files: 64,
            max_item_bytes: 4 * 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_passage_chars: 900,
            max_paragraphs_per_source: 2,
        }
    }
}

/// Deadline plus a byte allowance, shared by every retrieval phase.
#[derive(Debug)]
pub struct Budget {
    deadline: Instant,
    bytes_left: u64,
}

impl Budget {
    pub fn new(time: Duration, bytes: u64) -> Self {
        Self {
            deadline: Instant::now() + time,
            bytes_left: bytes,
        }
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// When the time budget runs out.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Reserve `bytes` from the allowance. Returns false (and reserves
    /// nothing) when the read would exceed the budget.
    pub fn take_bytes(&mut self, bytes: u64) -> bool {
        if bytes > self.bytes_left {
            return false;
        }
        self.bytes_left -= bytes;
        true
    }
}

/// A retrieved, ranked excerpt from one library document.
#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    /// Human-readable title (article title, book title, or book › section).
    pub title: String,
    /// Bare filename inside `books/` (never a host path).
    pub file: String,
    /// Article path inside a ZIM archive, used for deep links.
    pub zim_path: Option<String>,
    /// Search phrase for non-ZIM deep links; guaranteed to occur in `text`
    /// when it was derived from the passage.
    pub query: String,
    /// Plain-text excerpt.
    pub text: String,
    pub score: f64,
}

impl Passage {
    /// Books deep link: `/books?file=<f>&path=<zim path>` for ZIM articles,
    /// `/books?file=<f>&q=<keywords>` otherwise.
    pub fn link(&self) -> String {
        let mut link = format!("/books?file={}", encode_query_component(&self.file));
        match &self.zim_path {
            Some(path) => {
                link.push_str("&path=");
                link.push_str(&encode_query_component(path));
            }
            None if !self.query.is_empty() => {
                link.push_str("&q=");
                link.push_str(&encode_query_component(&self.query));
            }
            None => {}
        }
        link
    }

    pub fn source_ref(&self, n: usize) -> SourceRef {
        SourceRef {
            n,
            title: self.title.clone(),
            source: self.file.clone(),
            link: self.link(),
        }
    }
}

/// Payload of the `sources` SSE event.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SourceRef {
    pub n: usize,
    pub title: String,
    /// Bare filename of the book or archive.
    pub source: String,
    pub link: String,
}

// ---------------------------------------------------------------------------
// Keywords
// ---------------------------------------------------------------------------

fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.contains(&word)
}

fn is_keyword(lower: &str) -> bool {
    let chars = lower.chars().count();
    let numeric = lower.chars().all(|c| c.is_ascii_digit());
    (chars >= 3 || (numeric && chars >= 2)) && !is_stop_word(lower)
}

/// Split into alphanumeric word tokens, preserving case.
fn word_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty())
}

/// Lowercase keyword terms from a prompt, in first-occurrence order, without
/// duplicates, stop words or tokens shorter than three characters.
pub fn extract_keywords(prompt: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in word_tokens(prompt) {
        let lower = token.to_lowercase();
        if is_keyword(&lower) && seen.insert(lower.clone()) {
            out.push(lower);
            if out.len() >= MAX_KEYWORDS {
                break;
            }
        }
    }
    out
}

/// Runs of adjacent keyword tokens (original case), e.g. "Who was Gustav
/// Vasa?" yields `[["Gustav", "Vasa"]]`.
fn keyword_runs(prompt: &str) -> Vec<Vec<String>> {
    let mut runs: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for token in word_tokens(prompt) {
        if is_keyword(&token.to_lowercase()) {
            current.push(token.to_string());
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
        None => String::new(),
    }
}

fn capitalize_first_only(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Candidate ZIM article paths (underscore-separated, Wikipedia style) for the
/// prompt: longer phrases first, each in as-typed, Title_Case and
/// Sentence_case variants.
pub fn title_candidates(prompt: &str) -> Vec<String> {
    let runs = keyword_runs(prompt);
    let mut phrases: Vec<Vec<String>> = Vec::new();
    for size in (1..=3).rev() {
        for run in &runs {
            if run.len() < size {
                continue;
            }
            for window in run.windows(size) {
                phrases.push(window.to_vec());
            }
        }
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for words in phrases {
        let as_typed = words.join("_");
        let title_case = words.iter().map(|w| capitalize(w)).collect::<Vec<_>>().join("_");
        let lower = words.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>();
        let sentence_case = capitalize_first_only(&lower.join("_"));
        for candidate in [as_typed, title_case, sentence_case] {
            if seen.insert(candidate.clone()) {
                out.push(candidate);
            }
        }
        if out.len() >= MAX_TITLE_CANDIDATES {
            out.truncate(MAX_TITLE_CANDIDATES);
            break;
        }
    }
    out
}

/// Adjacent keyword pairs as lowercase "a b" phrases, used to pick a deep-link
/// search phrase that actually appears in a passage.
fn keyword_bigrams(prompt: &str) -> Vec<String> {
    keyword_runs(prompt)
        .iter()
        .flat_map(|run| {
            run.windows(2)
                .map(|w| format!("{} {}", w[0].to_lowercase(), w[1].to_lowercase()))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Text extraction
// ---------------------------------------------------------------------------

/// Decode the handful of HTML entities that matter for plain text.
pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let Some(semi) = after.find(';').filter(|i| *i <= 10) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let name = &after[1..semi];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if name.starts_with("#x") || name.starts_with("#X") => {
                u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32)
            }
            _ if name.starts_with('#') => name[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remove bracketed numeric reference markers ("[1]", "[ 12 ]", "[note 3]")
/// so they cannot be confused with the assistant's own `[n]` citations.
pub fn strip_reference_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let close = after.find(']').filter(|i| *i <= 16);
        let is_marker = close.is_some_and(|close| {
            let inner = after[1..close].trim();
            let inner = inner
                .strip_prefix("note")
                .or_else(|| inner.strip_prefix("citation needed"))
                .map(str::trim)
                .unwrap_or(inner);
            inner.is_empty() || inner.chars().all(|c| c.is_ascii_digit() || c == ',' || c == ' ')
        });
        match (is_marker, close) {
            (true, Some(close)) => rest = &after[close + 1..],
            _ => {
                out.push('[');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

const SKIPPED_HTML_ELEMENTS: &[&str] = &["script", "style", "head", "noscript", "svg", "math", "template"];
const BLOCK_HTML_ELEMENTS: &[&str] = &[
    "p", "div", "li", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "br", "tr", "td", "th",
    "table", "section", "article", "blockquote", "dd", "dt", "dl", "pre", "header", "footer",
    "figcaption", "aside", "nav", "body",
];

fn tag_name(tag_inner: &str) -> String {
    tag_inner
        .trim_start_matches('/')
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Byte offset of the first ASCII-case-insensitive occurrence of `needle`
/// (ASCII) in `haystack`, without allocating a lowercased copy.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

/// Split HTML into plain-text blocks at block-level elements, dropping
/// scripts, styles and other non-content elements. Each block goes through
/// [`library::strip_html_tags`], entity decoding and reference-marker removal.
/// Blocks shorter than 25 characters are dropped (see
/// [`html_to_text_blocks`] to keep them).
pub fn html_to_blocks(html: &str) -> Vec<String> {
    html_to_text_blocks(html, MIN_BLOCK_CHARS)
}

/// [`html_to_blocks`] keeping blocks of at least `min_chars` characters
/// (the full-text indexer keeps short blocks such as list items).
pub fn html_to_text_blocks(html: &str, min_chars: usize) -> Vec<String> {
    let mut segments: Vec<&str> = Vec::new();
    let mut seg_start = 0usize;
    let mut pos = 0usize;
    let bytes = html.as_bytes();

    while let Some(rel) = html[pos..].find('<') {
        let tag_start = pos + rel;
        let Some(rel_end) = html[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + rel_end + 1;
        let inner = &html[tag_start + 1..tag_end - 1];
        let name = tag_name(inner);
        let is_closing = inner.trim_start().starts_with('/');

        if !is_closing && SKIPPED_HTML_ELEMENTS.contains(&name.as_str()) && !inner.trim_end().ends_with('/') {
            segments.push(&html[seg_start..tag_start]);
            let close = format!("</{name}");
            let resume = match find_ascii_ci(&html[tag_end..], &close) {
                Some(close_rel) => {
                    let close_start = tag_end + close_rel;
                    html[close_start..].find('>').map(|i| close_start + i + 1).unwrap_or(html.len())
                }
                None => html.len(),
            };
            seg_start = resume;
            pos = resume;
            continue;
        }

        if BLOCK_HTML_ELEMENTS.contains(&name.as_str()) {
            segments.push(&html[seg_start..tag_start]);
            seg_start = tag_end;
        }
        pos = tag_end;
        if pos >= bytes.len() {
            break;
        }
    }
    if seg_start < html.len() {
        segments.push(&html[seg_start..]);
    }

    segments
        .into_iter()
        .map(|segment| {
            let text = decode_entities(&library::strip_html_tags(segment));
            collapse_whitespace(&strip_reference_markers(&text))
        })
        .filter(|block| !block.is_empty() && block.chars().count() >= min_chars)
        .collect()
}

/// A Markdown section: optional heading plus its paragraphs.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownSection {
    pub heading: Option<String>,
    pub paragraphs: Vec<String>,
}

/// Split Markdown into heading sections and blank-line separated paragraphs.
/// Fenced code blocks are kept as paragraphs but their fences are dropped.
pub fn split_markdown_sections(markdown: &str) -> Vec<MarkdownSection> {
    let mut sections = vec![MarkdownSection { heading: None, paragraphs: Vec::new() }];
    let mut paragraph: Vec<&str> = Vec::new();

    fn flush(paragraph: &mut Vec<&str>, sections: &mut [MarkdownSection]) {
        if paragraph.is_empty() {
            return;
        }
        let text = collapse_whitespace(&paragraph.join(" "));
        paragraph.clear();
        if text.chars().count() >= MIN_BLOCK_CHARS {
            if let Some(section) = sections.last_mut() {
                section.paragraphs.push(text);
            }
        }
    }

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush(&mut paragraph, &mut sections);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut paragraph, &mut sections);
            let heading = trimmed[hashes..].trim().trim_end_matches('#').trim().to_string();
            sections.push(MarkdownSection {
                heading: (!heading.is_empty()).then_some(heading),
                paragraphs: Vec::new(),
            });
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut sections);
        } else {
            paragraph.push(trimmed);
        }
    }
    flush(&mut paragraph, &mut sections);
    sections.retain(|s| !s.paragraphs.is_empty());
    sections
}

/// Trim `text` to at most `max_chars` characters, cutting at a word boundary
/// and appending an ellipsis when anything was removed.
pub fn trim_to_chars(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let cut = text
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    let head = &text[..cut];
    let head = match head.rfind(char::is_whitespace) {
        Some(space) if space > cut / 2 => &head[..space],
        _ => head,
    };
    format!("{}…", head.trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ';'))
}

/// Split an over-long paragraph into chunks of roughly `max_chars`, preferring
/// sentence boundaries, so each chunk can be scored on its own.
pub fn chunk_paragraph(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in text.split_inclusive(['.', '!', '?']) {
        let sentence_len = sentence.chars().count();
        if !current.is_empty() && current.chars().count() + sentence_len > max_chars {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if sentence_len > max_chars {
            for word in sentence.split_whitespace() {
                if !current.is_empty() && current.chars().count() + word.chars().count() + 1 > max_chars {
                    chunks.push(current.trim().to_string());
                    current.clear();
                }
                current.push_str(word);
                current.push(' ');
            }
        } else {
            current.push_str(sentence);
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// One scorable chunk of a document.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Groups chunks of the same document (file + section/article).
    pub doc_key: String,
    pub title: String,
    pub file: String,
    pub zim_path: Option<String>,
    /// Position inside the document, to keep combined excerpts in order.
    pub order: usize,
    pub text: String,
}

/// Lowercase word-token counts for `text`.
pub fn term_counts(text: &str) -> (HashMap<String, u32>, usize) {
    let mut counts = HashMap::new();
    let mut len = 0usize;
    for token in word_tokens(text) {
        len += 1;
        *counts.entry(token.to_lowercase()).or_insert(0) += 1;
    }
    (counts, len)
}

/// BM25-style scores for `candidates` against `keywords` (corpus statistics
/// are computed over the candidate set itself), with a bonus for keywords in
/// the document title. Returned in the same order as `candidates`.
pub fn score_candidates(candidates: &[Candidate], keywords: &[String]) -> Vec<f64> {
    if candidates.is_empty() || keywords.is_empty() {
        return vec![0.0; candidates.len()];
    }
    let stats: Vec<(HashMap<String, u32>, usize)> =
        candidates.iter().map(|c| term_counts(&c.text)).collect();
    let n = candidates.len() as f64;
    let avg_len = (stats.iter().map(|(_, len)| *len).sum::<usize>() as f64 / n).max(1.0);

    let idf: Vec<f64> = keywords
        .iter()
        .map(|term| {
            let df = stats.iter().filter(|(counts, _)| counts.contains_key(term)).count() as f64;
            (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
        })
        .collect();

    candidates
        .iter()
        .zip(&stats)
        .map(|(candidate, (counts, len))| {
            let mut score = 0.0;
            let mut matched_any = false;
            for (term, idf) in keywords.iter().zip(&idf) {
                let tf = f64::from(counts.get(term).copied().unwrap_or(0));
                if tf > 0.0 {
                    matched_any = true;
                    let norm = BM25_K1 * (1.0 - BM25_B + BM25_B * (*len as f64) / avg_len);
                    score += idf * tf * (BM25_K1 + 1.0) / (tf + norm);
                }
            }
            if !matched_any {
                return 0.0;
            }
            let (title_counts, _) = term_counts(&candidate.title);
            for (term, idf) in keywords.iter().zip(&idf) {
                if title_counts.contains_key(term) {
                    score += TITLE_BONUS * idf;
                }
            }
            score
        })
        .collect()
}

/// Pick a deep-link search phrase that occurs in `text`: the first keyword
/// bigram present, otherwise the rarest-looking (longest) keyword present.
fn pick_link_query(text: &str, keywords: &[String], bigrams: &[String]) -> String {
    let lower = collapse_whitespace(&text.to_lowercase());
    if let Some(phrase) = bigrams.iter().find(|b| lower.contains(b.as_str())) {
        return phrase.clone();
    }
    keywords
        .iter()
        .filter(|k| lower.contains(k.as_str()))
        .max_by_key(|k| k.chars().count())
        .cloned()
        .unwrap_or_default()
}

/// Rank candidates, group them per document and build the top `limit`
/// sources. Each source combines up to `max_paragraphs_per_source` of the
/// document's best chunks (in document order), trimmed to `max_passage_chars`.
pub fn select_passages(
    candidates: Vec<Candidate>,
    keywords: &[String],
    bigrams: &[String],
    limit: usize,
    limits: &RetrievalLimits,
) -> Vec<Passage> {
    let scores = score_candidates(&candidates, keywords);
    let mut by_doc: HashMap<String, Vec<(f64, Candidate)>> = HashMap::new();
    let mut doc_order: Vec<String> = Vec::new();
    for (candidate, score) in candidates.into_iter().zip(scores) {
        if score <= 0.0 {
            continue;
        }
        if !by_doc.contains_key(&candidate.doc_key) {
            doc_order.push(candidate.doc_key.clone());
        }
        by_doc.entry(candidate.doc_key.clone()).or_default().push((score, candidate));
    }

    let mut docs: Vec<(f64, usize, Vec<(f64, Candidate)>)> = doc_order
        .into_iter()
        .enumerate()
        .filter_map(|(index, key)| by_doc.remove(&key).map(|chunks| (index, chunks)))
        .map(|(index, mut chunks)| {
            chunks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            chunks.truncate(limits.max_paragraphs_per_source.max(1));
            // Best chunk dominates; a second strong chunk adds a little.
            let doc_score = chunks.first().map(|c| c.0).unwrap_or(0.0)
                + chunks.iter().skip(1).map(|c| c.0 * 0.25).sum::<f64>();
            (doc_score, index, chunks)
        })
        .collect();
    // Highest score first; ties keep discovery order for determinism.
    docs.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });

    docs.into_iter()
        .take(limit)
        .map(|(score, _, mut chunks)| {
            chunks.sort_by_key(|(_, c)| c.order);
            let first = chunks[0].1.clone();
            let joined = chunks
                .iter()
                .map(|(_, c)| c.text.trim())
                .collect::<Vec<_>>()
                .join(" … ");
            let text = trim_to_chars(&joined, limits.max_passage_chars);
            let query = if first.zim_path.is_some() {
                String::new()
            } else {
                pick_link_query(&text, keywords, bigrams)
            };
            Passage {
                title: first.title,
                file: first.file,
                zim_path: first.zim_path,
                query,
                text,
                score,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Candidate collection
// ---------------------------------------------------------------------------

fn contains_any_keyword(text_lower: &str, keywords: &[String]) -> bool {
    keywords.iter().any(|k| text_lower.contains(k.as_str()))
}

#[allow(clippy::too_many_arguments)]
fn push_chunks(
    out: &mut Vec<Candidate>,
    doc_key: &str,
    title: &str,
    file: &str,
    zim_path: Option<&str>,
    blocks: impl IntoIterator<Item = String>,
    keywords: &[String],
    max_chars: usize,
) {
    let mut order = 0usize;
    for block in blocks {
        for chunk in chunk_paragraph(&block, max_chars) {
            order += 1;
            // Only chunks mentioning a keyword can score; skip the rest early
            // to keep the candidate set (and BM25 statistics) small.
            if !contains_any_keyword(&chunk.to_lowercase(), keywords) {
                continue;
            }
            out.push(Candidate {
                doc_key: doc_key.to_string(),
                title: title.to_string(),
                file: file.to_string(),
                zim_path: zim_path.map(str::to_string),
                order,
                text: chunk,
            });
        }
    }
}

/// Read at most `cap` bytes from `reader` after reserving them in `budget`.
fn read_capped(reader: impl Read, declared_len: u64, cap: u64, budget: &mut Budget) -> Option<Vec<u8>> {
    if declared_len > cap || !budget.take_bytes(declared_len) {
        return None;
    }
    let mut out = Vec::with_capacity(declared_len as usize);
    reader.take(cap).read_to_end(&mut out).ok()?;
    Some(out)
}

fn collect_markdown(
    path: &Path,
    file: &str,
    keywords: &[String],
    limits: &RetrievalLimits,
    budget: &mut Budget,
    out: &mut Vec<Candidate>,
) {
    let Ok(handle) = std::fs::File::open(path) else { return };
    let len = handle.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
    let Some(bytes) = read_capped(handle, len, limits.max_item_bytes, budget) else { return };
    let content = String::from_utf8_lossy(&bytes);
    if !contains_any_keyword(&content.to_lowercase(), keywords) {
        return;
    }
    let book_title = library::extract_title(path, "md").unwrap_or_else(|| file_stem(file));
    for (index, section) in split_markdown_sections(&content).into_iter().enumerate() {
        let title = match &section.heading {
            Some(heading) if *heading != book_title => format!("{book_title} › {heading}"),
            _ => book_title.clone(),
        };
        let doc_key = format!("{file}#{index}");
        push_chunks(out, &doc_key, &title, file, None, section.paragraphs, keywords, limits.max_passage_chars);
    }
}

fn read_zip_text(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
    limits: &RetrievalLimits,
    budget: &mut Budget,
) -> Option<String> {
    let entry = archive.by_name(name).ok()?;
    let size = entry.size();
    let bytes = read_capped(entry, size, limits.max_item_bytes, budget)?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn collect_epub(
    path: &Path,
    file: &str,
    keywords: &[String],
    limits: &RetrievalLimits,
    budget: &mut Budget,
    out: &mut Vec<Candidate>,
) {
    let Ok(handle) = std::fs::File::open(path) else { return };
    let Ok(mut archive) = zip::ZipArchive::new(handle) else { return };
    let Some(container) = read_zip_text(&mut archive, "META-INF/container.xml", limits, budget) else { return };
    let Some(opf_path) = library::extract_xml_attr(&container, "full-path") else { return };
    let Some(opf) = read_zip_text(&mut archive, &opf_path, limits, budget) else { return };
    let opf_dir = Path::new(&opf_path).parent().unwrap_or(Path::new(""));
    let title = library::extract_xml_text_content(&opf, "dc:title")
        .map(|t| decode_entities(&t))
        .unwrap_or_else(|| file_stem(file));

    for (index, item) in library::collect_epub_spine_items(&opf)
        .into_iter()
        .take(MAX_EPUB_SPINE_ITEMS)
        .enumerate()
    {
        if budget.expired() {
            break;
        }
        let full = opf_dir.join(&item).to_string_lossy().replace('\\', "/");
        let Some(html) = read_zip_text(&mut archive, &full, limits, budget) else { continue };
        if !contains_any_keyword(&html.to_lowercase(), keywords) {
            continue;
        }
        let doc_key = format!("{file}#{index}");
        push_chunks(out, &doc_key, &title, file, None, html_to_blocks(&html), keywords, limits.max_passage_chars);
    }
}

/// Index of the first element for which `is_less` is false, assuming the
/// sequence is partitioned (all "less" elements first). Used for prefix
/// lookups in a ZIM title listing without reading the whole listing.
pub fn lower_bound(len: usize, mut is_less: impl FnMut(usize) -> bool) -> usize {
    let (mut low, mut high) = (0usize, len);
    while low < high {
        let mid = low + (high - low) / 2;
        if is_less(mid) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn is_html_article(entry: &zim::DirectoryEntry) -> bool {
    matches!(&entry.mime_type, zim::MimeType::Type(m) if m.starts_with("text/html") || m.starts_with("application/xhtml+xml"))
}

fn entry_title(entry: &zim::DirectoryEntry) -> String {
    if entry.title.is_empty() {
        entry.url.replace('_', " ")
    } else {
        entry.title.clone()
    }
}

/// Stage 0 for archives with a full-text index: the best BM25 matches for
/// the keywords (any keyword, ranked by how many and how rare), read from
/// the index's stored article text instead of the archive. Returns the
/// article paths taken and whether the index is complete.
#[allow(clippy::too_many_arguments)]
fn collect_zim_fulltext(
    index_dir: &Path,
    path: &Path,
    file: &str,
    keywords: &[String],
    limits: &RetrievalLimits,
    budget: &mut Budget,
    out: &mut Vec<Candidate>,
) -> (Vec<String>, bool) {
    let Some(reader) = crate::fulltext::open_reader(index_dir, file, path) else {
        return (Vec::new(), false);
    };
    let complete = reader.meta.state == crate::fulltext::IndexState::Ready;
    let Some(expr) = crate::fulltext::any_terms_expr(keywords) else {
        return (Vec::new(), complete);
    };
    let interrupt = crate::fulltext::Interrupt::at(budget.deadline());
    let docs = match reader.documents(&expr, limits.max_zim_articles_per_archive, &interrupt) {
        Ok(docs) => docs,
        Err(error) => {
            debug!("Full-text retrieval skipped {}: {}", file, error);
            return (Vec::new(), complete);
        }
    };
    let mut taken = Vec::new();
    for doc in docs {
        if budget.expired() {
            break;
        }
        let size = doc.body.len() as u64;
        if size > limits.max_item_bytes || !budget.take_bytes(size) {
            continue;
        }
        let blocks = doc
            .body
            .split('\n')
            .filter(|block| block.chars().count() >= MIN_BLOCK_CHARS)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let doc_key = format!("{file}#{}", doc.path);
        let before = out.len();
        push_chunks(out, &doc_key, &doc.title, file, Some(&doc.path), blocks, keywords, limits.max_passage_chars);
        // Passage scoring is accent-sensitive; a document whose match was
        // only accent-folded yields no chunk and must not use up a slot.
        if out.len() > before {
            taken.push(doc.path);
        }
    }
    (taken, complete)
}

#[allow(clippy::too_many_arguments)]
fn collect_zim(
    path: &Path,
    file: &str,
    prompt: &str,
    keywords: &[String],
    index_dir: Option<&Path>,
    limits: &RetrievalLimits,
    budget: &mut Budget,
    out: &mut Vec<Candidate>,
) -> Result<(), String> {
    // 0. Full-text index (when one exists for this archive version).
    let (fulltext_paths, index_complete) = match index_dir {
        Some(dir) => collect_zim_fulltext(dir, path, file, keywords, limits, budget, out),
        None => (Vec::new(), false),
    };
    let max_articles = limits.max_zim_articles_per_archive.saturating_sub(fulltext_paths.len());
    if max_articles == 0 || budget.expired() {
        return Ok(());
    }

    let zim = zim::Zim::new(path).map_err(|e| format!("cannot open ZIM: {e}"))?;
    let mut found: Vec<zim::DirectoryEntry> = Vec::new();
    let mut seen: HashSet<String> = fulltext_paths.into_iter().collect();

    let mut accept = |entry: zim::DirectoryEntry, found: &mut Vec<zim::DirectoryEntry>| {
        let Ok(resolved) = zim.resolve(entry) else { return };
        if !is_html_article(&resolved) {
            return;
        }
        let path = library::normalize_zim_url(&resolved.url);
        if path.is_empty() || path.starts_with("_assets_/") || !seen.insert(path) {
            return;
        }
        found.push(resolved);
    };

    // 1. Exact path lookups (binary search over the URL pointer list).
    let candidates = title_candidates(prompt);
    'exact: for candidate in &candidates {
        for namespace in [zim::Namespace::UserContent, zim::Namespace::Articles] {
            if found.len() >= max_articles || budget.expired() {
                break 'exact;
            }
            if let Ok(Some(entry)) = zim.get_by_path(namespace, candidate) {
                accept(entry, &mut found);
            }
        }
    }

    // 2. Title prefix lookups in the article title listing (format 6.1+).
    if found.len() < max_articles && !budget.expired() {
        if let Ok(Some(listing)) = zim.article_list_by_title() {
            let len = listing.len().unwrap_or(0);
            let title_at = |pos: usize| -> Option<zim::DirectoryEntry> {
                let idx = listing.get(pos).ok()??;
                zim.get_by_url_index(idx).ok()
            };
            'prefix: for candidate in &candidates {
                let prefix = candidate.replace('_', " ");
                let start = lower_bound(len, |pos| {
                    title_at(pos).map(|e| entry_title(&e) < prefix).unwrap_or(false)
                });
                for pos in start..len.min(start + MAX_TITLE_PREFIX_MATCHES) {
                    if found.len() >= max_articles || budget.expired() {
                        break 'prefix;
                    }
                    let Some(entry) = title_at(pos) else { break };
                    if !entry_title(&entry).starts_with(&prefix) {
                        break;
                    }
                    accept(entry, &mut found);
                }
            }
        }
    }

    // 3. Titles containing keywords: use the global search's cached title
    //    index when one exists for this archive (built in the background
    //    for archives up to its size limit); otherwise fall back to a linear
    //    scan, for small archives only.
    // A complete full-text index already covered every title.
    let indexed = if !index_complete && limits.use_title_index && found.len() < max_articles && !budget.expired() {
        crate::search::cached_title_keyword_matches(
            path,
            zim.header.article_count as usize,
            keywords,
            max_articles * 3,
        )
    } else {
        None
    };
    if let Some(indices) = indexed {
        for idx in indices {
            if found.len() >= max_articles || budget.expired() {
                break;
            }
            if let Ok(entry) = zim.get_by_url_index(idx) {
                accept(entry, &mut found);
            }
        }
    } else if !index_complete
        && found.len() < max_articles
        && !budget.expired()
        && zim.header.article_count <= limits.max_zim_linear_scan_entries
    {
        let mut scored: Vec<(usize, zim::DirectoryEntry)> = Vec::new();
        for (index, entry) in zim.iterate_by_urls().enumerate() {
            if index % 1024 == 0 && budget.expired() {
                break;
            }
            let Ok(entry) = entry else { continue };
            if !matches!(entry.namespace, zim::Namespace::Articles | zim::Namespace::UserContent) {
                continue;
            }
            let title_lower = entry_title(&entry).to_lowercase();
            let (title_terms, _) = term_counts(&title_lower);
            let hits = keywords.iter().filter(|k| title_terms.contains_key(k.as_str())).count();
            if hits > 0 {
                scored.push((hits, entry));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, entry) in scored {
            if found.len() >= max_articles {
                break;
            }
            accept(entry, &mut found);
        }
    }

    for entry in found {
        if budget.expired() {
            break;
        }
        let Ok(Some(content)) = zim.entry_content(&entry) else { continue };
        let size = content.len().unwrap_or(usize::MAX) as u64;
        if size > limits.max_item_bytes || !budget.take_bytes(size) {
            continue;
        }
        let Ok(bytes) = content.to_vec() else { continue };
        let html = String::from_utf8_lossy(&bytes);
        let article_path = library::normalize_zim_url(&entry.url);
        let title = entry_title(&entry);
        let doc_key = format!("{file}#{article_path}");
        push_chunks(
            out,
            &doc_key,
            &title,
            file,
            Some(&article_path),
            html_to_blocks(&html),
            keywords,
            limits.max_passage_chars,
        );
    }
    Ok(())
}

fn file_stem(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file)
        .to_string()
}

/// Retrieve up to `limit` ranked passages for `prompt` from the books
/// directory. Blocking; run via `spawn_blocking`.
#[cfg(test)]
pub fn retrieve(books_dir: &Path, prompt: &str, limit: usize, limits: &RetrievalLimits) -> Vec<Passage> {
    retrieve_with_index(books_dir, None, prompt, limit, limits)
}

/// [`retrieve`] that also uses the full-text indexes in `index_dir` (see
/// [`crate::fulltext`]) for ZIM archives that have one.
pub fn retrieve_with_index(
    books_dir: &Path,
    index_dir: Option<&Path>,
    prompt: &str,
    limit: usize,
    limits: &RetrievalLimits,
) -> Vec<Passage> {
    let keywords = extract_keywords(prompt);
    if keywords.is_empty() || limit == 0 {
        return Vec::new();
    }
    let started = Instant::now();
    let mut budget = Budget::new(limits.time_budget, limits.max_total_bytes);

    let mut files: Vec<(String, &'static str)> = std::fs::read_dir(books_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let format = library::detect_format(&name)?;
            Some((name, format))
        })
        .collect();
    files.sort();

    let mut candidates: Vec<Candidate> = Vec::new();

    // ZIM title lookups first: cheap and usually the most relevant hits.
    for (file, _) in files.iter().filter(|(_, f)| *f == "zim").take(limits.max_zim_archives) {
        if budget.expired() {
            break;
        }
        let path = books_dir.join(file);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            collect_zim(&path, file, prompt, &keywords, index_dir, limits, &mut budget, &mut candidates)
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!("Library retrieval skipped {}: {}", file, error),
            Err(_) => warn!("Library retrieval skipped {}: ZIM parser panicked", file),
        }
    }

    for (file, format) in files
        .iter()
        .filter(|(_, f)| matches!(*f, "md" | "epub"))
        .take(limits.max_text_files)
    {
        if budget.expired() {
            break;
        }
        let path = books_dir.join(file);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| match *format {
            "md" => collect_markdown(&path, file, &keywords, limits, &mut budget, &mut candidates),
            _ => collect_epub(&path, file, &keywords, limits, &mut budget, &mut candidates),
        }));
        if result.is_err() {
            warn!("Library retrieval skipped {}: parser panicked", file);
        }
    }

    let bigrams = keyword_bigrams(prompt);
    let passages = select_passages(candidates, &keywords, &bigrams, limit, limits);
    debug!(
        "Library retrieval: {} keyword(s), {} source(s) in {:.2?}",
        keywords.len(),
        passages.len(),
        started.elapsed()
    );
    passages
}

/// Percent-encode a query-string component (RFC 3986 unreserved set kept).
pub fn encode_query_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fyr-retrieval-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn candidate(doc: &str, title: &str, order: usize, text: &str) -> Candidate {
        Candidate {
            doc_key: doc.to_string(),
            title: title.to_string(),
            file: format!("{doc}.md"),
            zim_path: None,
            order,
            text: text.to_string(),
        }
    }

    fn write_epub(path: &Path, title: &str, chapters: &[(&str, &str)]) {
        let file = std::fs::File::create(path).expect("create epub");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        zip.start_file("mimetype", options).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        )
        .unwrap();
        let manifest: String = chapters
            .iter()
            .enumerate()
            .map(|(i, (name, _))| format!(r#"<item id="ch{i}" href="{name}" media-type="application/xhtml+xml"/>"#))
            .collect();
        let spine: String = (0..chapters.len()).map(|i| format!(r#"<itemref idref="ch{i}"/>"#)).collect();
        let opf = format!(
            r#"<?xml version="1.0"?><package><metadata><dc:title>{title}</dc:title></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
        );
        zip.start_file("OEBPS/content.opf", options).unwrap();
        zip.write_all(opf.as_bytes()).unwrap();
        for (name, body) in chapters {
            zip.start_file(format!("OEBPS/{name}"), options).unwrap();
            zip.write_all(format!("<html><head><title>x</title><style>p{{}}</style></head><body>{body}</body></html>").as_bytes())
                .unwrap();
        }
        zip.finish().unwrap();
    }

    // --- keywords ---

    #[test]
    fn keywords_drop_stop_words_short_tokens_and_punctuation() {
        assert_eq!(
            extract_keywords("What is the boiling point of water, at sea-level?"),
            vec!["boiling", "point", "water", "sea", "level"]
        );
    }

    #[test]
    fn keywords_handle_swedish_and_deduplicate() {
        assert_eq!(
            extract_keywords("Hur renar jag vatten? Vatten från sjön, alltså!"),
            vec!["renar", "vatten", "sjön", "alltså"]
        );
    }

    #[test]
    fn keywords_keep_numbers_and_cap_count() {
        assert_eq!(extract_keywords("Events in 1945 and WW2"), vec!["events", "1945", "ww2"]);
        let long = (0..30).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        assert_eq!(extract_keywords(&long).len(), MAX_KEYWORDS);
        assert!(extract_keywords("is it ok?").is_empty());
    }

    #[test]
    fn title_candidates_prefer_phrases_and_case_variants() {
        let candidates = title_candidates("who was gustav vasa?");
        assert_eq!(candidates[0], "gustav_vasa");
        assert!(candidates.contains(&"Gustav_Vasa".to_string()));
        assert!(candidates.contains(&"Gustav_vasa".to_string()));
        assert!(candidates.contains(&"Gustav".to_string()));
        let gv = candidates.iter().position(|c| c == "Gustav_Vasa").unwrap();
        let g = candidates.iter().position(|c| c == "Gustav").unwrap();
        assert!(gv < g, "phrases should come before single words: {candidates:?}");
    }

    // --- text extraction ---

    #[test]
    fn html_blocks_split_paragraphs_and_drop_scripts_and_markers() {
        let html = r#"<html><head><title>T</title></head><body>
            <script>var secret = "should not appear anywhere";</script>
            <h1>Water purification</h1>
            <p>Boiling water for one minute kills most pathogens.<sup>[12]</sup> It is &amp; remains simple.</p>
            <p>Short</p>
            <div>Chemical treatment with iodine or chlorine also works&nbsp;well.</div>
            </body></html>"#;
        let blocks = html_to_blocks(html);
        assert_eq!(
            blocks,
            vec![
                "Boiling water for one minute kills most pathogens. It is & remains simple.".to_string(),
                "Chemical treatment with iodine or chlorine also works well.".to_string(),
            ]
        );
    }

    #[test]
    fn decodes_numeric_and_named_entities() {
        assert_eq!(decode_entities("a &lt;b&gt; &#229;&#x00E4; &unknown; & c"), "a <b> åä &unknown; & c");
    }

    #[test]
    fn reference_markers_are_removed_but_other_brackets_kept() {
        assert_eq!(
            strip_reference_markers("Fact.[1][ 2 ] More[note 3] [citation needed] see [Appendix A]."),
            "Fact. More  see [Appendix A]."
        );
    }

    #[test]
    fn markdown_sections_follow_headings() {
        let md = "# Guide\n\nIntro paragraph that is long enough to keep.\n\n## Water\nBoil water for at least one minute\nbefore drinking it.\n\n```\ncode block line that is long enough\n```\n\n## Empty\n\nshort\n";
        let sections = split_markdown_sections(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading.as_deref(), Some("Guide"));
        assert_eq!(sections[1].heading.as_deref(), Some("Water"));
        assert_eq!(
            sections[1].paragraphs,
            vec![
                "Boil water for at least one minute before drinking it.".to_string(),
                "code block line that is long enough".to_string()
            ]
        );
    }

    #[test]
    fn trim_to_chars_cuts_at_word_boundary() {
        assert_eq!(trim_to_chars("short", 10), "short");
        let trimmed = trim_to_chars("alpha beta gamma delta epsilon", 14);
        assert_eq!(trimmed, "alpha beta…");
        assert!(trimmed.chars().count() <= 14);
        // Multi-byte text never panics on char boundaries.
        let swedish = trim_to_chars("åäö åäö åäö åäö", 6);
        assert!(swedish.ends_with('…'));
    }

    #[test]
    fn chunk_paragraph_respects_limit() {
        let text = "One sentence here. Another sentence there. A third one follows. And a fourth.";
        let chunks = chunk_paragraph(text, 40);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 40), "{chunks:?}");
        assert_eq!(chunks.join(" "), text);
    }

    // --- scoring / selection ---

    #[test]
    fn bm25_prefers_rare_terms_and_repeated_matches() {
        let keywords = vec!["water".to_string(), "iodine".to_string()];
        let candidates = vec![
            candidate("a", "Notes", 1, "water water water is everywhere in this text about water"),
            candidate("b", "Notes", 1, "iodine tablets purify water quickly"),
            candidate("c", "Notes", 1, "nothing relevant here at all"),
        ];
        let scores = score_candidates(&candidates, &keywords);
        assert!(scores[1] > scores[0], "rare term match should win: {scores:?}");
        assert_eq!(scores[2], 0.0);
    }

    #[test]
    fn title_matches_get_a_bonus() {
        let keywords = vec!["vasa".to_string()];
        let candidates = vec![
            candidate("a", "Gustav Vasa", 1, "He was king of Sweden and Vasa ruled"),
            candidate("b", "Kings", 1, "He was king of Sweden and Vasa ruled"),
        ];
        let scores = score_candidates(&candidates, &keywords);
        assert!(scores[0] > scores[1]);
    }

    #[test]
    fn select_groups_per_document_limits_and_orders() {
        let limits = RetrievalLimits { max_passage_chars: 200, max_paragraphs_per_source: 2, ..Default::default() };
        let keywords = vec!["water".to_string(), "boil".to_string()];
        let candidates = vec![
            candidate("a", "Survival", 2, "boil water for one minute"),
            candidate("a", "Survival", 1, "water is essential"),
            candidate("a", "Survival", 3, "water again and water"),
            candidate("b", "Cooking", 1, "boil pasta in salted water"),
            candidate("c", "Unrelated", 1, "maps and tiles"),
        ];
        let passages = select_passages(candidates, &keywords, &["boil water".to_string()], 5, &limits);
        assert_eq!(passages.len(), 2, "unmatched docs are dropped");
        assert_eq!(passages[0].file, "a.md");
        // Two best chunks of doc a, recombined in document order.
        assert!(passages[0].text.contains(" … "));
        assert!(passages[0].text.find("boil water").unwrap() < passages[0].text.find("water again").unwrap_or(usize::MAX));
        assert_eq!(passages[0].query, "boil water");
        assert!(passages[0].score >= passages[1].score);

        let one = select_passages(
            vec![candidate("a", "S", 1, "water"), candidate("b", "S", 1, "water")],
            &keywords,
            &[],
            1,
            &limits,
        );
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn selected_text_is_trimmed_to_char_budget() {
        let limits = RetrievalLimits { max_passage_chars: 50, ..Default::default() };
        let long = "water ".repeat(40);
        let passages = select_passages(vec![candidate("a", "T", 1, &long)], &["water".to_string()], &[], 3, &limits);
        assert!(passages[0].text.chars().count() <= 50);
    }

    #[test]
    fn links_use_books_deep_link_contract_without_host_paths() {
        let mut passage = Passage {
            title: "Gustav Vasa".into(),
            file: "wikipedia sv.zim".into(),
            zim_path: Some("Gustav_Vasa/Ä".into()),
            query: String::new(),
            text: "x".into(),
            score: 1.0,
        };
        assert_eq!(passage.link(), "/books?file=wikipedia%20sv.zim&path=Gustav_Vasa%2F%C3%84");
        passage.zim_path = None;
        passage.query = "boil water".into();
        passage.file = "guide.md".into();
        let source = passage.source_ref(2);
        assert_eq!(source.link, "/books?file=guide.md&q=boil%20water");
        assert_eq!(source.n, 2);
        assert_eq!(source.source, "guide.md");
    }

    // --- budget ---

    #[test]
    fn budget_enforces_bytes_and_deadline() {
        let mut budget = Budget::new(Duration::from_secs(60), 100);
        assert!(budget.take_bytes(60));
        assert!(!budget.take_bytes(41));
        assert!(budget.take_bytes(40));
        assert!(!budget.expired());
        assert!(Budget::new(Duration::ZERO, 10).expired());
    }

    #[test]
    fn retrieval_stops_when_time_budget_is_exhausted() {
        let dir = temp_dir("deadline");
        std::fs::write(dir.join("guide.md"), "# Guide\n\nBoil water for one minute before drinking.\n").unwrap();
        let limits = RetrievalLimits { time_budget: Duration::ZERO, ..Default::default() };
        assert!(retrieve(&dir, "boil water", 4, &limits).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn retrieval_respects_byte_and_file_caps() {
        let dir = temp_dir("caps");
        std::fs::write(dir.join("a.md"), "# A\n\nBoil water for one minute before drinking.\n").unwrap();
        std::fs::write(dir.join("b.md"), "# B\n\nWater should boil vigorously to be safe to drink.\n").unwrap();

        let tiny_item = RetrievalLimits { max_item_bytes: 10, ..Default::default() };
        assert!(retrieve(&dir, "boil water", 4, &tiny_item).is_empty());

        let one_file = RetrievalLimits { max_text_files: 1, ..Default::default() };
        let passages = retrieve(&dir, "boil water", 4, &one_file);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].file, "a.md");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lower_bound_finds_prefix_start() {
        let titles = ["Apple", "Gustav I", "Gustav Vasa", "Gustavus", "Vasa"];
        let start = lower_bound(titles.len(), |i| titles[i] < "Gustav Vasa");
        assert_eq!(start, 2);
        assert_eq!(lower_bound(titles.len(), |i| titles[i] < "Zebra"), titles.len());
        assert_eq!(lower_bound(0, |_| true), 0);
    }

    // --- retrieval over real files ---

    #[test]
    fn retrieves_from_markdown_and_epub_files() {
        let dir = temp_dir("files");
        std::fs::write(
            dir.join("survival.md"),
            "# Survival Guide\n\nGeneral advice about staying calm.\n\n## Water\n\nBoil water for at least one minute to purify it before drinking.\n\n## Fire\n\nUse dry tinder and build the fire gradually from small sticks.\n",
        )
        .unwrap();
        write_epub(
            &dir.join("handbook.epub"),
            "Field Handbook",
            &[
                ("ch1.xhtml", "<h1>Shelter</h1><p>Build a shelter out of the wind before nightfall.</p>"),
                ("ch2.xhtml", "<h1>Purify</h1><p>Iodine tablets purify water when boiling is not possible.</p>"),
            ],
        );
        std::fs::write(dir.join("notes.pdf"), "%PDF-1.4 water water water").unwrap();
        std::fs::write(dir.join("ignored.txt"), "water water").unwrap();

        let passages = retrieve(&dir, "How do I purify water?", 4, &RetrievalLimits::default());
        assert_eq!(passages.len(), 2, "{passages:#?}");
        let files: HashSet<&str> = passages.iter().map(|p| p.file.as_str()).collect();
        assert_eq!(files, HashSet::from(["survival.md", "handbook.epub"]));

        let md = passages.iter().find(|p| p.file == "survival.md").unwrap();
        assert_eq!(md.title, "Survival Guide › Water");
        assert!(md.text.contains("Boil water"));
        assert!(md.link().starts_with("/books?file=survival.md&q="));

        let epub = passages.iter().find(|p| p.file == "handbook.epub").unwrap();
        assert_eq!(epub.title, "Field Handbook");
        assert!(epub.text.contains("Iodine tablets purify water"));
        assert!(!epub.text.contains("p{}"), "styles must be stripped");
        assert_eq!(epub.query, "purify water");

        let dir_str = dir.display().to_string();
        for passage in &passages {
            let source = serde_json::to_string(&passage.source_ref(1)).unwrap();
            assert!(!source.contains(&dir_str), "host path leaked: {source}");
        }

        assert!(retrieve(&dir, "the and of", 4, &RetrievalLimits::default()).is_empty());
        assert!(retrieve(&dir, "volcano", 4, &RetrievalLimits::default()).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The shared sample archive (two articles plus a "Vasa" redirect).
    fn write_test_zim(path: &Path) {
        crate::test_zim::write_sample_zim(path);
    }

    /// Limits for tests that pin down the exact/prefix/linear-scan stages.
    /// The shared title index is process-wide and built in the background,
    /// so leaving it on would let a finished build from another call or test
    /// add hits nondeterministically.
    fn zim_stage_limits() -> RetrievalLimits {
        RetrievalLimits { use_title_index: false, ..Default::default() }
    }

    #[test]
    fn retrieves_zim_articles_by_exact_title_and_redirect() {
        let dir = temp_dir("zim");
        write_test_zim(&dir.join("wiki.zim"));
        let no_scan = RetrievalLimits { max_zim_linear_scan_entries: 0, ..zim_stage_limits() };

        let passages = retrieve(&dir, "Who was Gustav Vasa?", 4, &no_scan);
        assert_eq!(passages[0].title, "Gustav Vasa");
        assert_eq!(passages[0].zim_path.as_deref(), Some("Gustav_Vasa"));
        assert_eq!(passages[0].link(), "/books?file=wiki.zim&path=Gustav_Vasa");
        assert!(passages[0].text.contains("King of Sweden from 1523"));
        assert!(!passages[0].text.contains("[1]"), "reference markers are stripped");

        // Lowercase single word resolves through the "Vasa" redirect.
        let redirect = retrieve(&dir, "vasa", 4, &no_scan);
        assert_eq!(redirect.len(), 1);
        assert_eq!(redirect[0].zim_path.as_deref(), Some("Gustav_Vasa"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn retrieves_zim_articles_by_title_prefix_and_small_archive_scan() {
        let dir = temp_dir("zim-prefix");
        write_test_zim(&dir.join("wiki.zim"));

        // "Kings" is only a title prefix of "Kings of Sweden".
        let no_scan = RetrievalLimits { max_zim_linear_scan_entries: 0, ..zim_stage_limits() };
        let prefix = retrieve(&dir, "list the kings please", 4, &no_scan);
        assert_eq!(prefix.len(), 1, "{prefix:#?}");
        assert_eq!(prefix[0].zim_path.as_deref(), Some("Kings_of_Sweden"));

        // "sweden" matches no path or prefix, only the small-archive title scan.
        assert!(retrieve(&dir, "sweden", 4, &no_scan).is_empty());
        let scanned = retrieve(&dir, "sweden", 4, &zim_stage_limits());
        assert_eq!(scanned.len(), 1, "{scanned:#?}");
        assert_eq!(scanned[0].title, "Kings of Sweden");

        // A zero article budget fetches nothing at all.
        let none = RetrievalLimits { max_zim_articles_per_archive: 0, ..zim_stage_limits() };
        assert!(retrieve(&dir, "Gustav Vasa", 4, &none).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_archives_are_skipped() {
        let dir = temp_dir("broken");
        std::fs::write(dir.join("broken.zim"), b"not a zim archive at all").unwrap();
        std::fs::write(dir.join("broken.epub"), b"PK not really a zip").unwrap();
        std::fs::write(dir.join("ok.md"), "# Ok\n\nWater is wet and good to drink when clean.\n").unwrap();
        let passages = retrieve(&dir, "water", 4, &RetrievalLimits::default());
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].file, "ok.md");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_books_dir_yields_nothing() {
        let dir = std::env::temp_dir().join(format!("fyr-retrieval-missing-{}", uuid::Uuid::new_v4()));
        assert!(retrieve(&dir, "water", 4, &RetrievalLimits::default()).is_empty());
    }
}
