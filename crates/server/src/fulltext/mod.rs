//! On-disk full-text index for ZIM archives.
//!
//! Global search and assistant retrieval can only afford title lookups in
//! large ZIM archives at request time. This module builds, in the
//! background, one SQLite FTS5 database per archive with every HTML
//! article's path, title and plain body text, so both can match words that
//! only occur in article bodies.
//!
//! * [`store`]: the index database (schema, writer, read-only queries).
//! * [`worker`]: the single low-priority background indexer and its
//!   admin controls / status.
//! * [`routes`]: `GET /api/index/status` (public) and the protected
//!   `POST /api/index/:filename/build` / `DELETE /api/index/:filename`.
//!
//! Index files live in `DATA_DIR/index/` (never served over HTTP) and are
//! named `<archive-stem>-<hash of filename>.fts.sqlite`. Each records the
//! archive's size and mtime; an index whose archive changed is ignored by
//! readers and rebuilt by the worker. Only filenames and archive-internal
//! paths leave this module.

pub mod routes;
pub mod store;
pub mod worker;

#[cfg(test)]
mod tests;

pub use store::{ArchiveStamp, IndexReader, IndexState, Interrupt};
pub use worker::FulltextIndexer;

use std::path::{Path, PathBuf};

/// Highlight delimiters emitted by `snippet()`. Private-use code points that
/// are stripped from indexed text, so they can only come from FTS5.
pub const HIGHLIGHT_START: char = '\u{E000}';
pub const HIGHLIGHT_END: char = '\u{E001}';

/// Most query terms turned into an FTS5 expression.
const MAX_QUERY_TERMS: usize = 12;
/// Longest single query term (characters).
const MAX_TERM_CHARS: usize = 64;
/// Shortest final term that is matched as a prefix ("zuri" → "zuri*").
const MIN_PREFIX_CHARS: usize = 3;

/// HTML articles larger than this are indexed by title only.
pub const MAX_ARTICLE_HTML_BYTES: usize = 8 * 1024 * 1024;
/// Plain body text stored per article is capped at this many bytes.
pub const MAX_BODY_BYTES: usize = 256 * 1024;

/// File name of an archive's index inside the index directory:
/// `<sanitised stem>-<12 hex chars of SHA-256(filename)>.fts.sqlite`.
/// The hash keeps names unique when two filenames sanitise to the same stem.
pub fn index_file_name(archive: &str) -> String {
    let stem = Path::new(archive)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(archive);
    let safe: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(60)
        .collect();
    let digest = ring::digest::digest(&ring::digest::SHA256, archive.as_bytes());
    format!("{safe}-{}.fts.sqlite", hex::encode(&digest.as_ref()[..6]))
}

pub fn index_path(index_dir: &Path, archive: &str) -> PathBuf {
    index_dir.join(index_file_name(archive))
}

/// The index database plus its WAL side files.
pub fn index_files(path: &Path) -> [PathBuf; 3] {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let shm = PathBuf::from(format!("{}-shm", path.display()));
    [path.to_path_buf(), wal, shm]
}

/// Remove an index and its side files. Returns false when a file that
/// exists could not be removed.
pub fn remove_index_files(path: &Path) -> bool {
    let mut ok = true;
    for file in index_files(path) {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => ok = false,
        }
    }
    ok
}

/// Total on-disk size of an index (database + WAL + shared memory).
pub fn index_size(path: &Path) -> u64 {
    index_files(path)
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum()
}

/// Whether `name` is an acceptable bare archive filename for the index API.
pub fn is_valid_archive_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
}

// ---------------------------------------------------------------------------
// Query building (pure). Every term is emitted as a double-quoted FTS5
// string, so user input can never introduce operators, column filters,
// NEAR groups or syntax errors.
// ---------------------------------------------------------------------------

/// Case- and accent-folded word terms of `text`, in order, without
/// duplicates (split on anything that is not a letter or digit, which is
/// also where the `unicode61` tokenizer splits).
pub fn query_terms(text: &str) -> Vec<String> {
    let folded = crate::search::fold(text);
    let mut out: Vec<String> = Vec::new();
    for term in folded.split(|c: char| !c.is_alphanumeric()) {
        if term.is_empty() {
            continue;
        }
        let term: String = term.chars().take(MAX_TERM_CHARS).collect();
        if !out.contains(&term) {
            out.push(term);
        }
        if out.len() >= MAX_QUERY_TERMS {
            break;
        }
    }
    out
}

fn quote_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Expression matching documents that contain every term of `text`; the
/// last term also matches as a prefix (search-as-you-type) when it has at
/// least three characters. `None` when `text` has no terms.
pub fn all_terms_expr(text: &str) -> Option<String> {
    let terms = query_terms(text);
    let last = terms.len().checked_sub(1)?;
    let parts: Vec<String> = terms
        .iter()
        .enumerate()
        .map(|(i, term)| {
            if i == last && term.chars().count() >= MIN_PREFIX_CHARS {
                format!("{} *", quote_term(term))
            } else {
                quote_term(term)
            }
        })
        .collect();
    Some(parts.join(" "))
}

/// Expression matching documents that contain every term of `text` as a
/// whole word (no prefix matching). `None` when `text` has no terms.
pub fn exact_terms_expr(text: &str) -> Option<String> {
    let terms = query_terms(text);
    if terms.is_empty() {
        return None;
    }
    Some(terms.iter().map(|t| quote_term(t)).collect::<Vec<_>>().join(" "))
}

/// Expression matching documents that contain any of `keywords`; BM25 then
/// ranks documents matching more (and rarer) keywords higher.
pub fn any_terms_expr(keywords: &[String]) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for keyword in keywords {
        for term in query_terms(keyword) {
            if !terms.contains(&term) && terms.len() < MAX_QUERY_TERMS {
                terms.push(term);
            }
        }
    }
    if terms.is_empty() {
        return None;
    }
    Some(terms.iter().map(|t| quote_term(t)).collect::<Vec<_>>().join(" OR "))
}

// ---------------------------------------------------------------------------
// Text extraction
// ---------------------------------------------------------------------------

/// Plain text of an HTML article for indexing: block-level paragraphs (see
/// [`crate::ai::retrieval::html_to_text_blocks`]) joined by newlines, with
/// control and highlight-marker characters removed, capped at
/// [`MAX_BODY_BYTES`] on a character boundary.
pub fn article_text(html: &str) -> String {
    let mut out = String::new();
    for block in crate::ai::retrieval::html_to_text_blocks(html, 1) {
        if out.len() >= MAX_BODY_BYTES {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        for ch in block.chars() {
            if ch.is_control() || ch == HIGHLIGHT_START || ch == HIGHLIGHT_END {
                continue;
            }
            if out.len() + ch.len_utf8() > MAX_BODY_BYTES {
                return out;
            }
            out.push(ch);
        }
    }
    out
}

/// One piece of a highlighted snippet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SnippetSegment {
    pub text: String,
    #[serde(rename = "match")]
    pub matched: bool,
}

/// Split an FTS5 `snippet()` result into plain/highlighted segments.
/// Adjacent segments never share the same flag; empty text is dropped.
pub fn snippet_segments(snippet: &str) -> Vec<SnippetSegment> {
    let mut segments: Vec<SnippetSegment> = Vec::new();
    let mut matched = false;
    let mut current = String::new();
    let flush = |text: &mut String, matched: bool, segments: &mut Vec<SnippetSegment>| {
        if text.is_empty() {
            return;
        }
        match segments.last_mut() {
            Some(last) if last.matched == matched => last.text.push_str(text),
            _ => segments.push(SnippetSegment { text: text.clone(), matched }),
        }
        text.clear();
    };
    for ch in snippet.chars() {
        match ch {
            HIGHLIGHT_START => {
                flush(&mut current, matched, &mut segments);
                matched = true;
            }
            HIGHLIGHT_END => {
                flush(&mut current, matched, &mut segments);
                matched = false;
            }
            '\n' | '\r' | '\t' => current.push(' '),
            c if c.is_control() => {}
            c => current.push(c),
        }
    }
    flush(&mut current, matched, &mut segments);
    segments
}

/// Open the index of `archive` (stored at `archive_path`) for queries, if it
/// exists, belongs to the archive's current version and holds any rows
/// (ready, or still being built: partial recall beats none).
pub fn open_reader(index_dir: &Path, archive: &str, archive_path: &Path) -> Option<IndexReader> {
    let stamp = ArchiveStamp::of(archive_path)?;
    let path = index_path(index_dir, archive);
    if !path.is_file() {
        return None;
    }
    let reader = IndexReader::open(&path).ok()?;
    let usable = reader.meta.matches(archive, &stamp)
        && matches!(reader.meta.state, IndexState::Ready | IndexState::Building)
        && reader.meta.articles > 0;
    usable.then_some(reader)
}
