//! Global offline search — `GET /api/search?q=&limit=&kinds=`.
//!
//! One query fans out over every local content source and returns a single
//! ranked, de-duplicated list:
//!
//! | kind      | source                                            |
//! |-----------|---------------------------------------------------|
//! | `book`    | book titles (extracted metadata) and filenames     |
//! | `article` | ZIM article titles across every archive in books/  |
//! | `content` | full-text hits inside Markdown / EPUB books        |
//! | `map`     | map filenames in maps/                             |
//! | `poi`     | GeoJSON feature names/descriptions in poi/         |
//!
//! The endpoint is read-only and public. Results carry only *filenames* and
//! frontend route descriptors (`link`), never host paths, so they are safe in
//! every access mode.
//!
//! Work is bounded on several axes so a huge archive (full Wikipedia) cannot
//! stall the server: an overall time budget with a per-archive share, a
//! per-archive and total result cap, size caps for full-text and GeoJSON
//! parsing, and a limit on concurrently running searches. Ranking, merging
//! and limits are pure functions (see the `tests` module).

use crate::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, warn};
use zim::{DirectoryEntry, MimeType, Namespace, Zim};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// Default number of results returned when `limit` is omitted.
pub const DEFAULT_LIMIT: usize = 40;
/// Hard ceiling for `limit`.
pub const MAX_LIMIT: usize = 200;
/// Queries are truncated to this many characters.
pub const MAX_QUERY_CHARS: usize = 200;
/// Minimum query length (characters) before full-text and description
/// matching is attempted. Shorter queries only match titles/names.
pub const MIN_CONTENT_QUERY_CHARS: usize = 3;
/// Every kind keeps at least this many of its best results before the
/// remaining slots are filled purely by score, so one large source (a big
/// ZIM archive) cannot crowd out an exact book or map match.
pub const MIN_PER_KIND: usize = 5;
/// Maximum article results taken from a single ZIM archive.
pub const PER_ARCHIVE_LIMIT: usize = 15;
/// Maximum POI results taken from a single GeoJSON file.
pub const PER_POI_FILE_LIMIT: usize = 25;
/// Overall wall-clock budget for one search request.
pub const SEARCH_BUDGET: Duration = Duration::from_millis(1500);
/// Smallest slice of the budget handed to one ZIM archive.
const MIN_ARCHIVE_BUDGET: Duration = Duration::from_millis(150);

/// ZIM archives with at most this many title entries get an in-memory title
/// index built synchronously on first search.
const SYNC_INDEX_MAX_ENTRIES: usize = 60_000;
/// Archives up to this size get their title index built on a background
/// thread; until it is ready (and above this size, always) the archive is
/// searched with a prefix lookup on its title-ordered listing.
const ASYNC_INDEX_MAX_ENTRIES: usize = 250_000;
/// Upper bound on title entries held across all cached indices
/// (~40 bytes each, so roughly 20 MB).
const ZIM_INDEX_CACHE_MAX_ENTRIES: usize = 500_000;
/// Time allowed for a background index build.
const BACKGROUND_INDEX_BUDGET: Duration = Duration::from_secs(20);

/// Markdown books larger than this are not full-text searched.
const MAX_MARKDOWN_CONTENT_BYTES: u64 = 8 * 1024 * 1024;
/// EPUB books larger than this are not full-text searched.
const MAX_EPUB_CONTENT_BYTES: u64 = 32 * 1024 * 1024;
/// At most this many books are full-text searched per request.
const MAX_CONTENT_BOOKS: usize = 40;
/// Full-text hits collected per book (the best one becomes the result).
const CONTENT_HITS_PER_BOOK: usize = 3;
/// PDFs larger than this use their filename instead of parsing for /Title.
const MAX_PDF_TITLE_BYTES: u64 = 16 * 1024 * 1024;
/// GeoJSON files larger than this are skipped.
const MAX_POI_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// Upper bound on POI records held across all cached files.
const POI_CACHE_MAX_RECORDS: usize = 300_000;
/// Records kept per GeoJSON file (so any single file stays cacheable).
const MAX_POI_RECORDS_PER_FILE: usize = 150_000;
/// Upper bound on cached book titles.
const BOOK_TITLE_CACHE_MAX: usize = 4096;
/// Concurrent searches allowed; extra requests queue (and are dropped for
/// free when the client aborts while queued).
const MAX_CONCURRENT_SEARCHES: usize = 2;

// Score tiers. Tiers are 200 apart and the length penalty is capped at 100,
// so a better tier always outranks a worse one.
const SCORE_EXACT: f64 = 1000.0;
const SCORE_PREFIX: f64 = 800.0;
const SCORE_WORD_PREFIX: f64 = 600.0;
const SCORE_ALL_TOKENS: f64 = 450.0;
const SCORE_SUBSTRING: f64 = 300.0;
const SCORE_CONTENT: f64 = 250.0;
const MAX_LENGTH_PENALTY: f64 = 100.0;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Book,
    Map,
    Poi,
    Article,
    Content,
}

impl SearchKind {
    pub const ALL: [SearchKind; 5] = [
        SearchKind::Book,
        SearchKind::Map,
        SearchKind::Poi,
        SearchKind::Article,
        SearchKind::Content,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "book" | "books" => Some(SearchKind::Book),
            "map" | "maps" => Some(SearchKind::Map),
            "poi" | "pois" => Some(SearchKind::Poi),
            "article" | "articles" | "zim" => Some(SearchKind::Article),
            "content" | "text" | "fulltext" => Some(SearchKind::Content),
            _ => None,
        }
    }

    /// Small bonus so that, at equal match quality, whole items (a book, a
    /// map) rank above parts of items (an article, a text hit).
    fn bonus(self) -> f64 {
        match self {
            SearchKind::Book | SearchKind::Map => 40.0,
            SearchKind::Poi => 20.0,
            SearchKind::Article => 0.0,
            SearchKind::Content => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Filename (never a path) of the file the hit came from.
    pub source: String,
    pub score: f64,
    /// Frontend route, e.g. `/books?file=x.zim&path=A%2FFoo`.
    pub link: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lng: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GlobalSearchParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub kinds: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GlobalSearchResponse {
    pub query: String,
    pub results: Vec<SearchHit>,
    /// Matches found before the result limit was applied.
    pub total: usize,
    /// `true` when matches were dropped by the limit.
    pub truncated: bool,
    /// `true` when the time budget ran out before every source was searched,
    /// so more matches may exist.
    pub partial: bool,
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Sets a shared flag when the request future is dropped (client aborted),
/// so blocking work in progress can stop early.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, AtomicOrdering::Relaxed);
    }
}

fn search_permits() -> &'static tokio::sync::Semaphore {
    static PERMITS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    PERMITS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_SEARCHES))
}

/// GET /api/search?q=&limit=&kinds=book,map,poi,article,content
pub async fn global_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GlobalSearchParams>,
) -> Result<Json<GlobalSearchResponse>, StatusCode> {
    let started = Instant::now();
    let query = SearchQuery::new(params.q.as_deref().unwrap_or(""));
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let kinds = parse_kinds(params.kinds.as_deref());

    let Some(query) = query else {
        return Ok(Json(empty_response(String::new(), started)));
    };
    if kinds.is_empty() {
        return Ok(Json(empty_response(query.display.clone(), started)));
    }

    let _permit = search_permits()
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel_guard = CancelOnDrop(Arc::clone(&cancelled));
    let roots = SearchRoots::from_config(&state.config);
    let display = query.display.clone();

    let outcome = crate::handlers::run_blocking(move || {
        let deadline = Deadline::new(SEARCH_BUDGET, cancelled);
        Ok(run_search(&roots, &query, &kinds, &deadline))
    })
    .await?;

    let (results, total, truncated) = merge_results(outcome.hits, limit, MIN_PER_KIND);

    Ok(Json(GlobalSearchResponse {
        query: display,
        results,
        total,
        truncated,
        partial: outcome.partial,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }))
}

fn empty_response(query: String, started: Instant) -> GlobalSearchResponse {
    GlobalSearchResponse {
        query,
        results: Vec::new(),
        total: 0,
        truncated: false,
        partial: false,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

/// Parse the `kinds` filter. Absent or blank means every kind; unknown names
/// are ignored (so a filter of only unknown names matches nothing).
pub fn parse_kinds(raw: Option<&str>) -> HashSet<SearchKind> {
    match raw.map(str::trim) {
        None | Some("") => SearchKind::ALL.into_iter().collect(),
        Some(list) => list.split(',').filter_map(SearchKind::parse).collect(),
    }
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Directories searched. Kept separate from `Config` so tests can point it
/// at temp dirs.
#[derive(Debug, Clone)]
pub struct SearchRoots {
    pub books: PathBuf,
    pub maps: PathBuf,
    pub poi: PathBuf,
}

impl SearchRoots {
    pub fn from_config(config: &types::Config) -> Self {
        Self {
            books: config.books_dir(),
            maps: config.maps_dir(),
            poi: config.poi_dir(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub partial: bool,
}

/// Wall-clock budget plus cooperative cancellation.
#[derive(Debug, Clone)]
pub struct Deadline {
    end: Instant,
    cancelled: Arc<AtomicBool>,
}

impl Deadline {
    pub fn new(budget: Duration, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            end: Instant::now() + budget,
            cancelled,
        }
    }

    pub fn after(budget: Duration) -> Self {
        Self::new(budget, Arc::new(AtomicBool::new(false)))
    }

    pub fn expired(&self) -> bool {
        self.cancelled.load(AtomicOrdering::Relaxed) || Instant::now() >= self.end
    }

    pub fn remaining(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }

    /// A sub-deadline that ends no later than this one.
    fn child(&self, budget: Duration) -> Deadline {
        let end = (Instant::now() + budget).min(self.end);
        Deadline {
            end,
            cancelled: Arc::clone(&self.cancelled),
        }
    }
}

/// Run every enabled source, cheapest first, until the budget runs out.
pub fn run_search(
    roots: &SearchRoots,
    query: &SearchQuery,
    kinds: &HashSet<SearchKind>,
    deadline: &Deadline,
) -> SearchOutcome {
    let mut outcome = SearchOutcome::default();

    if kinds.contains(&SearchKind::Map) {
        outcome.hits.extend(search_maps(&roots.maps, query));
    }

    // Book titles are needed for book hits and as archive labels on
    // article/content hits.
    let needs_books = kinds.contains(&SearchKind::Book)
        || kinds.contains(&SearchKind::Article)
        || kinds.contains(&SearchKind::Content);
    let books = if needs_books {
        list_books(&roots.books, deadline)
    } else {
        Vec::new()
    };

    if kinds.contains(&SearchKind::Book) {
        outcome.hits.extend(search_book_titles(&books, query));
    }

    if kinds.contains(&SearchKind::Poi) {
        let (hits, partial) = search_poi_dir(&roots.poi, query, deadline);
        outcome.hits.extend(hits);
        outcome.partial |= partial;
    }

    if kinds.contains(&SearchKind::Article) {
        let archives: Vec<&BookFile> = books.iter().filter(|b| b.format == "zim").collect();
        for (i, archive) in archives.iter().enumerate() {
            if deadline.expired() {
                outcome.partial = true;
                break;
            }
            let left = (archives.len() - i) as u32;
            let share = (deadline.remaining() / left).max(MIN_ARCHIVE_BUDGET);
            let archive_deadline = deadline.child(share);
            let result = search_zim_archive(archive, query, PER_ARCHIVE_LIMIT, &archive_deadline);
            outcome.hits.extend(result.hits);
            outcome.partial |= result.partial;
        }
    }

    if kinds.contains(&SearchKind::Content) && query.char_len >= MIN_CONTENT_QUERY_CHARS {
        let (hits, partial) = search_book_contents(&roots.books, &books, query, deadline);
        outcome.hits.extend(hits);
        outcome.partial |= partial;
    }

    outcome
}

// ---------------------------------------------------------------------------
// Query normalisation and scoring (pure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Trimmed, whitespace-collapsed, length-capped query as typed.
    pub display: String,
    /// Case-folded form used for matching.
    pub folded: String,
    /// Folded whitespace-separated tokens.
    pub tokens: Vec<String>,
    pub char_len: usize,
}

impl SearchQuery {
    /// Returns `None` for an empty / whitespace-only query.
    pub fn new(raw: &str) -> Option<Self> {
        let collapsed: String = raw
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_QUERY_CHARS)
            .collect();
        let display = collapsed.trim().to_string();
        if display.is_empty() {
            return None;
        }
        let folded = fold(&display);
        if folded.is_empty() {
            return None;
        }
        let tokens = folded.split(' ').filter(|t| !t.is_empty()).map(str::to_string).collect();
        let char_len = folded.chars().count();
        Some(Self {
            display,
            folded,
            tokens,
            char_len,
        })
    }
}

/// Case-fold for matching: Unicode lowercase, `_` treated as a space (ZIM
/// paths and filenames), whitespace collapsed.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = true;
    for ch in text.chars() {
        let ch = if ch == '_' { ' ' } else { ch };
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
            continue;
        }
        prev_space = false;
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Turn a filename into a readable, matchable label:
/// `alps_hiking-guide.v2.pmtiles` → `alps hiking guide v2`.
pub fn humanize_filename(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    stem.chars()
        .map(|c| if c == '_' || c == '-' || c == '.' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Score `text` against the query; `None` means no match.
pub fn match_score(text: &str, query: &SearchQuery) -> Option<f64> {
    score_folded(&fold(text), query)
}

/// Like [`match_score`] for text that is already [`fold`]ed.
pub fn score_folded(text: &str, query: &SearchQuery) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let q = query.folded.as_str();

    let tier = if text == q {
        SCORE_EXACT
    } else if text.starts_with(q) {
        SCORE_PREFIX
    } else if contains_at_word_start(text, q) {
        SCORE_WORD_PREFIX
    } else if query.tokens.len() > 1
        && query.tokens.iter().all(|t| contains_at_word_start(text, t))
    {
        SCORE_ALL_TOKENS
    } else if query.char_len >= 2 && text.contains(q) {
        SCORE_SUBSTRING
    } else {
        return None;
    };

    let extra = text.chars().count().saturating_sub(query.char_len) as f64;
    Some(tier - (extra * 0.5).min(MAX_LENGTH_PENALTY))
}

/// `true` when `needle` occurs in `haystack` at the start of a word.
pub fn contains_at_word_start(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(idx, _)| {
        haystack[..idx]
            .chars()
            .next_back()
            .map_or(true, |prev| !prev.is_alphanumeric())
    })
}

/// Case variants of the query to try against case-sensitive title-ordered
/// ZIM listings ("einstein" → "Einstein", "albert ein" → "Albert Ein", ...).
pub fn case_variants(display: &str) -> Vec<String> {
    fn upper_first(word: &str) -> String {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }
    fn upper_first_rest_lower(word: &str) -> String {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => first
                .to_uppercase()
                .chain(chars.flat_map(char::to_lowercase))
                .collect(),
            None => String::new(),
        }
    }

    let lower = display.to_lowercase();
    let candidates = [
        display.to_string(),
        upper_first(display),
        upper_first_rest_lower(display),
        display
            .split(' ')
            .map(upper_first)
            .collect::<Vec<_>>()
            .join(" "),
        lower.clone(),
        display.to_uppercase(),
        // Old-style ZIM titles sometimes keep underscores.
        upper_first(&display.replace(' ', "_")),
    ];

    let mut out: Vec<String> = Vec::new();
    for candidate in candidates {
        if !candidate.is_empty() && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

/// Rank, de-duplicate (by `link`, keeping the best score) and limit hits.
///
/// Each kind keeps up to `min_per_kind` of its best hits before the remaining
/// slots are filled by score. The returned list is sorted by score
/// (descending), then kind, then title. Returns `(results, total, truncated)`
/// where `total` is the de-duplicated match count.
pub fn merge_results(
    hits: Vec<SearchHit>,
    limit: usize,
    min_per_kind: usize,
) -> (Vec<SearchHit>, usize, bool) {
    let mut best: HashMap<String, SearchHit> = HashMap::new();
    for hit in hits {
        if !hit.score.is_finite() {
            continue;
        }
        match best.get(&hit.link) {
            Some(existing) if existing.score >= hit.score => {}
            _ => {
                best.insert(hit.link.clone(), hit);
            }
        }
    }

    let mut sorted: Vec<SearchHit> = best.into_values().collect();
    sorted.sort_by(compare_hits);
    let total = sorted.len();
    if total <= limit {
        return (sorted, total, false);
    }

    let mut taken = vec![false; total];
    let mut per_kind: HashMap<SearchKind, usize> = HashMap::new();
    let mut count = 0;

    // Pass 1: guaranteed slots per kind.
    for (i, hit) in sorted.iter().enumerate() {
        if count >= limit {
            break;
        }
        let used = per_kind.entry(hit.kind).or_insert(0);
        if *used < min_per_kind {
            *used += 1;
            taken[i] = true;
            count += 1;
        }
    }
    // Pass 2: fill by score.
    for slot in taken.iter_mut() {
        if count >= limit {
            break;
        }
        if !*slot {
            *slot = true;
            count += 1;
        }
    }

    let results: Vec<SearchHit> = sorted
        .into_iter()
        .zip(taken)
        .filter_map(|(hit, keep)| keep.then_some(hit))
        .collect();
    (results, total, true)
}

fn compare_hits(a: &SearchHit, b: &SearchHit) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.kind.cmp(&b.kind))
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        .then_with(|| a.link.cmp(&b.link))
}

/// Keep only the `k` best `(score, item)` pairs.
fn top_k<T>(mut items: Vec<(f64, T)>, k: usize) -> Vec<(f64, T)> {
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    items.truncate(k);
    items
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

/// Percent-encode a query-string component (RFC 3986 unreserved kept).
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

pub fn book_link(filename: &str) -> String {
    format!("/books?file={}", encode_component(filename))
}

pub fn article_link(filename: &str, path: &str) -> String {
    format!(
        "/books?file={}&path={}",
        encode_component(filename),
        encode_component(path)
    )
}

pub fn content_link(filename: &str, query: &str) -> String {
    format!(
        "/books?file={}&q={}",
        encode_component(filename),
        encode_component(query)
    )
}

pub fn map_link(filename: &str) -> String {
    format!("/maps?map={}", encode_component(filename))
}

pub fn poi_link(lat: f64, lng: f64, label: &str) -> String {
    format!(
        "/maps?lat={}&lng={}&zoom=14&label={}",
        format_coord(lat),
        format_coord(lng),
        encode_component(label)
    )
}

fn format_coord(value: f64) -> String {
    let text = format!("{:.6}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_string() } else { text.to_string() }
}

// ---------------------------------------------------------------------------
// File stamps and bounded caches
// ---------------------------------------------------------------------------

/// Identity of a file version: cache entries are reused only while the file's
/// size and mtime are unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl FileStamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

struct CacheSlot<V> {
    stamp: FileStamp,
    value: Arc<V>,
    weight: usize,
    last_used: u64,
}

/// LRU cache keyed by path + [`FileStamp`] with a total weight bound.
struct StampedCache<V> {
    slots: HashMap<PathBuf, CacheSlot<V>>,
    max_weight: usize,
    total_weight: usize,
    tick: u64,
}

impl<V> StampedCache<V> {
    fn new(max_weight: usize) -> Self {
        Self {
            slots: HashMap::new(),
            max_weight,
            total_weight: 0,
            tick: 0,
        }
    }

    fn get(&mut self, path: &Path, stamp: &FileStamp) -> Option<Arc<V>> {
        self.tick += 1;
        let tick = self.tick;
        let stale = match self.slots.get_mut(path) {
            Some(slot) if slot.stamp == *stamp => {
                slot.last_used = tick;
                return Some(Arc::clone(&slot.value));
            }
            Some(_) => true,
            None => false,
        };
        if stale {
            self.remove(path);
        }
        None
    }

    fn insert(&mut self, path: PathBuf, stamp: FileStamp, value: Arc<V>, weight: usize) {
        let weight = weight.max(1);
        if weight > self.max_weight {
            return;
        }
        self.remove(&path);
        while self.total_weight + weight > self.max_weight {
            let Some(oldest) = self
                .slots
                .iter()
                .min_by_key(|(_, slot)| slot.last_used)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            self.remove(&oldest);
        }
        self.tick += 1;
        self.total_weight += weight;
        self.slots.insert(
            path,
            CacheSlot {
                stamp,
                value,
                weight,
                last_used: self.tick,
            },
        );
    }

    fn remove(&mut self, path: &Path) {
        if let Some(slot) = self.slots.remove(path) {
            self.total_weight -= slot.weight;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn book_title_cache() -> &'static Mutex<StampedCache<Option<String>>> {
    static CACHE: OnceLock<Mutex<StampedCache<Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StampedCache::new(BOOK_TITLE_CACHE_MAX)))
}

fn zim_index_cache() -> &'static Mutex<StampedCache<ZimTitleIndex>> {
    static CACHE: OnceLock<Mutex<StampedCache<ZimTitleIndex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StampedCache::new(ZIM_INDEX_CACHE_MAX_ENTRIES)))
}

fn poi_cache() -> &'static Mutex<StampedCache<Vec<PoiRecord>>> {
    static CACHE: OnceLock<Mutex<StampedCache<Vec<PoiRecord>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StampedCache::new(POI_CACHE_MAX_RECORDS)))
}

fn index_builds_in_flight() -> &'static Mutex<HashSet<PathBuf>> {
    static BUILDING: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    BUILDING.get_or_init(|| Mutex::new(HashSet::new()))
}

// ---------------------------------------------------------------------------
// Directory listing
// ---------------------------------------------------------------------------

/// Regular files directly inside `dir` (symlinks followed), sorted by name.
fn list_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = entry.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            Some((name, path))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn extension_lower(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// Source: maps
// ---------------------------------------------------------------------------

pub fn search_maps(maps_dir: &Path, query: &SearchQuery) -> Vec<SearchHit> {
    list_files(maps_dir)
        .into_iter()
        .filter(|(name, _)| matches!(extension_lower(name).as_str(), "pmtiles" | "mbtiles"))
        .filter_map(|(name, _)| {
            let label = humanize_filename(&name);
            let score = match_score(&label, query)
                .into_iter()
                .chain(match_score(&name, query))
                .fold(None, |acc: Option<f64>, s| Some(acc.map_or(s, |a| a.max(s))))?;
            Some(SearchHit {
                kind: SearchKind::Map,
                title: label,
                subtitle: Some(format!("{} map", extension_lower(&name).to_uppercase())),
                link: map_link(&name),
                source: name,
                score: score + SearchKind::Map.bonus(),
                lat: None,
                lng: None,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Source: book titles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BookFile {
    pub filename: String,
    pub path: PathBuf,
    pub format: &'static str,
    pub title: Option<String>,
    pub size: u64,
}

impl BookFile {
    fn label(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| humanize_filename(&self.filename))
    }
}

/// Books directly in `books_dir` with their (cached) extracted titles.
/// Once the deadline has passed, uncached titles are not extracted (the
/// filename is used instead) so a directory of large PDFs cannot stall it.
pub fn list_books(books_dir: &Path, deadline: &Deadline) -> Vec<BookFile> {
    list_files(books_dir)
        .into_iter()
        .filter_map(|(filename, path)| {
            let format = crate::library::detect_format(&filename)?;
            let stamp = FileStamp::of(&path)?;
            let title = cached_book_title(&path, &stamp, format, deadline);
            Some(BookFile {
                filename,
                path,
                format,
                title,
                size: stamp.len,
            })
        })
        .collect()
}

fn cached_book_title(path: &Path, stamp: &FileStamp, format: &str, deadline: &Deadline) -> Option<String> {
    if let Some(title) = lock(book_title_cache()).get(path, stamp) {
        return (*title).clone();
    }
    if deadline.expired() {
        return None;
    }
    let title = if format == "pdf" && stamp.len > MAX_PDF_TITLE_BYTES {
        None
    } else {
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            crate::library::extract_title(path, format)
        }))
        .ok()
        .flatten()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    };
    lock(book_title_cache()).insert(path.to_path_buf(), stamp.clone(), Arc::new(title.clone()), 1);
    title
}

pub fn search_book_titles(books: &[BookFile], query: &SearchQuery) -> Vec<SearchHit> {
    books
        .iter()
        .filter_map(|book| {
            let label = book.label();
            let score = [
                book.title.as_deref().and_then(|t| match_score(t, query)),
                match_score(&humanize_filename(&book.filename), query),
                match_score(&book.filename, query),
            ]
            .into_iter()
            .flatten()
            .fold(None, |acc: Option<f64>, s| Some(acc.map_or(s, |a| a.max(s))))?;

            let format = book.format.to_uppercase();
            let subtitle = if book.title.is_some() {
                format!("{} · {}", format, book.filename)
            } else {
                format
            };
            Some(SearchHit {
                kind: SearchKind::Book,
                title: label,
                subtitle: Some(subtitle),
                source: book.filename.clone(),
                score: score + SearchKind::Book.bonus(),
                link: book_link(&book.filename),
                lat: None,
                lng: None,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Source: book contents (Markdown / EPUB full text)
// ---------------------------------------------------------------------------

pub fn search_book_contents(
    books_dir: &Path,
    books: &[BookFile],
    query: &SearchQuery,
    deadline: &Deadline,
) -> (Vec<SearchHit>, bool) {
    let mut hits = Vec::new();
    let eligible = books.iter().filter(|book| match book.format {
        "md" => book.size <= MAX_MARKDOWN_CONTENT_BYTES,
        "epub" => book.size <= MAX_EPUB_CONTENT_BYTES,
        _ => false,
    });

    let mut scanned = 0;
    for book in eligible {
        if scanned >= MAX_CONTENT_BOOKS {
            return (hits, true);
        }
        if deadline.expired() {
            return (hits, true);
        }
        scanned += 1;

        let found = std::panic::catch_unwind(AssertUnwindSafe(|| {
            crate::library::search_book(books_dir, &book.filename, &query.display, CONTENT_HITS_PER_BOOK)
        }));
        let response = match found {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                debug!("Global search skipped content of {}: {}", book.filename, error);
                continue;
            }
            Err(_) => {
                warn!("Global search: content search panicked for {}", book.filename);
                continue;
            }
        };
        let Some(first) = response.results.first() else {
            continue;
        };

        let raw_snippet = first.snippet.clone().unwrap_or_else(|| first.title.clone());
        let snippet = crate::library::snippet_around(&raw_snippet, &query.folded, 70)
            .unwrap_or_else(|| raw_snippet.chars().take(160).collect());
        let matches = response.results.len().min(CONTENT_HITS_PER_BOOK) as f64;

        hits.push(SearchHit {
            kind: SearchKind::Content,
            title: book.label(),
            subtitle: Some(snippet),
            source: book.filename.clone(),
            score: SCORE_CONTENT + matches * 10.0 + SearchKind::Content.bonus(),
            link: content_link(&book.filename, &query.display),
            lat: None,
            lng: None,
        });
    }
    (hits, false)
}

// ---------------------------------------------------------------------------
// Source: POIs (GeoJSON)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PoiRecord {
    pub name: String,
    pub description: Option<String>,
    name_folded: String,
    title_folded: String,
    description_folded: String,
    pub lat: f64,
    pub lng: f64,
}

pub fn search_poi_dir(poi_dir: &Path, query: &SearchQuery, deadline: &Deadline) -> (Vec<SearchHit>, bool) {
    let mut hits = Vec::new();
    for (filename, path) in list_files(poi_dir) {
        if !matches!(extension_lower(&filename).as_str(), "geojson" | "json") {
            continue;
        }
        if deadline.expired() {
            return (hits, true);
        }
        let Some(records) = load_poi_records(&path) else {
            continue;
        };
        hits.extend(search_poi_records(&records, &filename, query, PER_POI_FILE_LIMIT));
    }
    (hits, false)
}

fn load_poi_records(path: &Path) -> Option<Arc<Vec<PoiRecord>>> {
    let stamp = FileStamp::of(path)?;
    if stamp.len > MAX_POI_FILE_BYTES {
        return None;
    }
    if let Some(records) = lock(poi_cache()).get(path, &stamp) {
        return Some(records);
    }
    let bytes = std::fs::read(path).ok()?;
    let records = match parse_poi_records(&bytes) {
        Ok(records) => Arc::new(records),
        Err(error) => {
            debug!("Global search skipped malformed GeoJSON {}: {}", path.display(), error);
            // Cache the failure as an empty list so a broken file is not
            // re-parsed on every keystroke.
            Arc::new(Vec::new())
        }
    };
    let weight = records.len();
    lock(poi_cache()).insert(path.to_path_buf(), stamp, Arc::clone(&records), weight);
    Some(records)
}

/// Extract searchable records from a GeoJSON FeatureCollection, Feature, or
/// bare array of Features. Features without usable coordinates are skipped.
pub fn parse_poi_records(bytes: &[u8]) -> Result<Vec<PoiRecord>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid JSON: {e}"))?;

    let features: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(obj) => match obj.get("type").and_then(|t| t.as_str()) {
            Some("FeatureCollection") => obj
                .get("features")
                .and_then(|f| f.as_array())
                .map(|f| f.iter().collect())
                .unwrap_or_default(),
            Some("Feature") => vec![&value],
            _ => return Err("not a GeoJSON Feature or FeatureCollection".to_string()),
        },
        _ => return Err("not a GeoJSON object".to_string()),
    };

    Ok(features
        .into_iter()
        .filter_map(poi_record_from_feature)
        .take(MAX_POI_RECORDS_PER_FILE)
        .collect())
}

fn poi_record_from_feature(feature: &serde_json::Value) -> Option<PoiRecord> {
    let props = feature.get("properties").and_then(|p| p.as_object());
    let prop_text = |key: &str| -> Option<String> {
        let value = props?.get(key)?;
        let text = match value {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        (!text.is_empty()).then_some(text)
    };

    let name = prop_text("name");
    let title = prop_text("title");
    let description = prop_text("description");
    if name.is_none() && title.is_none() && description.is_none() {
        return None;
    }

    let (lat, lng) = geometry_anchor(feature.get("geometry")?)?;
    let label = name
        .clone()
        .or_else(|| title.clone())
        .unwrap_or_else(|| "Unnamed place".to_string());

    Some(PoiRecord {
        name_folded: name.as_deref().map(fold).unwrap_or_default(),
        title_folded: title.as_deref().map(fold).unwrap_or_default(),
        description_folded: description.as_deref().map(fold).unwrap_or_default(),
        name: label,
        description,
        lat,
        lng,
    })
}

/// A representative `(lat, lng)` for a geometry: the point itself, or the
/// mean of (at most a few thousand) positions for lines/polygons.
pub fn geometry_anchor(geometry: &serde_json::Value) -> Option<(f64, f64)> {
    const MAX_POSITIONS: usize = 5000;

    fn collect(value: &serde_json::Value, out: &mut Vec<(f64, f64)>) {
        if out.len() >= MAX_POSITIONS {
            return;
        }
        let Some(items) = value.as_array() else {
            return;
        };
        if items.len() >= 2 && items[0].is_number() && items[1].is_number() {
            if let (Some(lng), Some(lat)) = (items[0].as_f64(), items[1].as_f64()) {
                if lat.is_finite()
                    && lng.is_finite()
                    && (-90.0..=90.0).contains(&lat)
                    && (-180.0..=180.0).contains(&lng)
                {
                    out.push((lat, lng));
                }
            }
            return;
        }
        for item in items {
            collect(item, out);
        }
    }

    let mut positions = Vec::new();
    match geometry.get("type").and_then(|t| t.as_str()) {
        Some("GeometryCollection") => {
            for child in geometry.get("geometries")?.as_array()? {
                if let Some(anchor) = geometry_anchor(child) {
                    positions.push(anchor);
                }
            }
        }
        _ => collect(geometry.get("coordinates")?, &mut positions),
    }
    if positions.is_empty() {
        return None;
    }
    let n = positions.len() as f64;
    let (lat_sum, lng_sum) = positions
        .iter()
        .fold((0.0, 0.0), |(a, b), (lat, lng)| (a + lat, b + lng));
    Some((lat_sum / n, lng_sum / n))
}

pub fn search_poi_records(
    records: &[PoiRecord],
    filename: &str,
    query: &SearchQuery,
    limit: usize,
) -> Vec<SearchHit> {
    let matched: Vec<(f64, &PoiRecord)> = records
        .iter()
        .filter_map(|record| {
            let mut best = [
                score_folded(&record.name_folded, query),
                score_folded(&record.title_folded, query),
            ]
            .into_iter()
            .flatten()
            .fold(None, |acc: Option<f64>, s| Some(acc.map_or(s, |a| a.max(s))));
            if best.is_none()
                && query.char_len >= MIN_CONTENT_QUERY_CHARS
                && record.description_folded.contains(&query.folded)
            {
                best = Some(SCORE_CONTENT);
            }
            best.map(|score| (score, record))
        })
        .collect();

    top_k(matched, limit)
        .into_iter()
        .map(|(score, record)| {
            let subtitle = match &record.description {
                Some(desc) => {
                    let short: String = desc.chars().take(120).collect();
                    let ellipsis = if desc.chars().count() > 120 { "…" } else { "" };
                    format!("{short}{ellipsis} · {filename}")
                }
                None => filename.to_string(),
            };
            SearchHit {
                kind: SearchKind::Poi,
                title: record.name.clone(),
                subtitle: Some(subtitle),
                source: filename.to_string(),
                score: score + SearchKind::Poi.bonus(),
                link: poi_link(record.lat, record.lng, &record.name),
                lat: Some(record.lat),
                lng: Some(record.lng),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Source: ZIM article titles
// ---------------------------------------------------------------------------

/// Case-folded titles of an archive's articles with their URL-pointer index.
struct ZimTitleIndex {
    entries: Vec<(Box<str>, u32)>,
}

#[derive(Debug, Default)]
struct ArchiveOutcome {
    hits: Vec<SearchHit>,
    partial: bool,
}

fn open_zim(path: &Path) -> Option<Zim> {
    match std::panic::catch_unwind(AssertUnwindSafe(|| Zim::new(path))) {
        Ok(Ok(zim)) => Some(zim),
        _ => None,
    }
}

fn entry_display_title(entry: &DirectoryEntry) -> &str {
    if entry.title.is_empty() {
        &entry.url
    } else {
        &entry.title
    }
}

/// Whether a directory entry (possibly a redirect) can stand for an article.
/// `articles_only` is set when the entry came from the v1 article listing.
fn is_article_candidate(entry: &DirectoryEntry, articles_only: bool) -> bool {
    if entry.url.is_empty() || entry.url.starts_with("_assets_/") {
        return false;
    }
    match entry.namespace {
        Namespace::Articles => true,
        Namespace::UserContent => {
            articles_only
                || matches!(entry.mime_type, MimeType::Redirect)
                || crate::handlers::is_searchable_article_entry(entry)
        }
        _ => false,
    }
}

/// The title-ordered listing, preferring the article-only v1 listing.
fn title_listing(zim: &Zim) -> Option<(zim::Listing<'_>, bool)> {
    if let Ok(Some(listing)) = zim.article_list_by_title() {
        return Some((listing, true));
    }
    match zim.entry_list_by_title() {
        Ok(Some(listing)) => Some((listing, false)),
        _ => None,
    }
}

fn search_zim_archive(
    archive: &BookFile,
    query: &SearchQuery,
    limit: usize,
    deadline: &Deadline,
) -> ArchiveOutcome {
    match std::panic::catch_unwind(AssertUnwindSafe(|| {
        search_zim_archive_inner(archive, query, limit, deadline)
    })) {
        Ok(Some(outcome)) => outcome,
        Ok(None) => ArchiveOutcome::default(),
        Err(_) => {
            warn!("Global search: ZIM search panicked for {}", archive.filename);
            ArchiveOutcome::default()
        }
    }
}

fn search_zim_archive_inner(
    archive: &BookFile,
    query: &SearchQuery,
    limit: usize,
    deadline: &Deadline,
) -> Option<ArchiveOutcome> {
    let stamp = FileStamp::of(&archive.path)?;
    let zim = open_zim(&archive.path)?;
    let listing = title_listing(&zim);
    let entry_count = listing
        .as_ref()
        .and_then(|(l, _)| l.len().ok())
        .unwrap_or(zim.header.article_count as usize);
    let want = limit * 3;
    let mut partial = false;

    let cached = lock(zim_index_cache()).get(&archive.path, &stamp);
    let index = match cached {
        Some(index) => Some(index),
        None if entry_count <= SYNC_INDEX_MAX_ENTRIES => {
            let built = build_title_index(&zim, listing.as_ref(), deadline).map(Arc::new);
            if let Some(index) = &built {
                lock(zim_index_cache()).insert(
                    archive.path.clone(),
                    stamp.clone(),
                    Arc::clone(index),
                    index.entries.len(),
                );
            }
            built
        }
        None => {
            if entry_count <= ASYNC_INDEX_MAX_ENTRIES {
                schedule_background_index(archive.path.clone(), stamp.clone());
            }
            None
        }
    };

    let candidates: Vec<(f64, u32)> = match (&index, &listing) {
        (Some(index), _) => score_title_index(index, query, want),
        (None, Some((listing, articles_only))) => {
            let found = prefix_candidates(&zim, listing, *articles_only, query, want, deadline);
            partial |= deadline.expired();
            found
        }
        (None, None) => {
            let (found, timed_out) = scan_candidates(&zim, query, want, deadline);
            partial |= timed_out;
            found
        }
    };

    let archive_label = archive.label();
    let mut seen_paths = HashSet::new();
    let mut hits = Vec::new();
    for (score, idx) in top_k(candidates, want) {
        if hits.len() >= limit {
            break;
        }
        let Ok(entry) = zim.get_by_url_index(idx) else {
            continue;
        };
        let matched_title = entry_display_title(&entry).replace('_', " ");
        let Ok(resolved) = zim.resolve(entry) else {
            continue;
        };
        if !crate::handlers::is_searchable_article_entry(&resolved) {
            continue;
        }
        let path = crate::library::normalize_zim_url(&resolved.url);
        if path.is_empty() || path.starts_with("_assets_/") || !seen_paths.insert(path.clone()) {
            continue;
        }
        let resolved_title = entry_display_title(&resolved).replace('_', " ");
        let subtitle = if resolved_title != matched_title {
            format!("→ {} · {}", resolved_title, archive_label)
        } else {
            archive_label.clone()
        };
        hits.push(SearchHit {
            kind: SearchKind::Article,
            title: matched_title,
            subtitle: Some(subtitle),
            source: archive.filename.clone(),
            score: score + SearchKind::Article.bonus(),
            link: article_link(&archive.filename, &path),
            lat: None,
            lng: None,
        });
    }

    Some(ArchiveOutcome { hits, partial })
}

/// Build the folded title index for an archive. Returns `None` when the
/// deadline expires first (nothing is cached then).
fn build_title_index(
    zim: &Zim,
    listing: Option<&(zim::Listing<'_>, bool)>,
    deadline: &Deadline,
) -> Option<ZimTitleIndex> {
    let (indices, articles_only): (Vec<u32>, bool) = match listing {
        Some((listing, articles_only)) => (listing.to_vec().ok()?, *articles_only),
        None => ((0..zim.header.article_count).collect(), false),
    };

    let mut entries = Vec::new();
    for (n, idx) in indices.into_iter().enumerate() {
        if n % 512 == 0 && deadline.expired() {
            return None;
        }
        let Ok(entry) = zim.get_by_url_index(idx) else {
            continue;
        };
        if !is_article_candidate(&entry, articles_only) {
            continue;
        }
        let folded = fold(entry_display_title(&entry));
        if !folded.is_empty() {
            entries.push((folded.into_boxed_str(), idx));
        }
    }
    entries.shrink_to_fit();
    Some(ZimTitleIndex { entries })
}

/// Build one archive's title index on a background thread. Only one build
/// runs at a time; other archives are picked up by later searches.
fn schedule_background_index(path: PathBuf, stamp: FileStamp) {
    {
        let mut in_flight = lock(index_builds_in_flight());
        if !in_flight.is_empty() {
            return;
        }
        in_flight.insert(path.clone());
    }
    let spawned = std::thread::Builder::new()
        .name("fyr-search-index".to_string())
        .spawn({
            let path = path.clone();
            move || {
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let Some(zim) = open_zim(&path) else {
                        return;
                    };
                    let listing = title_listing(&zim);
                    let deadline = Deadline::after(BACKGROUND_INDEX_BUDGET);
                    if let Some(index) = build_title_index(&zim, listing.as_ref(), &deadline) {
                        let weight = index.entries.len();
                        lock(zim_index_cache()).insert(path.clone(), stamp, Arc::new(index), weight);
                    }
                }));
                lock(index_builds_in_flight()).remove(&path);
            }
        });
    if spawned.is_err() {
        lock(index_builds_in_flight()).remove(&path);
    }
}

fn score_title_index(index: &ZimTitleIndex, query: &SearchQuery, want: usize) -> Vec<(f64, u32)> {
    let matched = index
        .entries
        .iter()
        .filter_map(|(title, idx)| score_folded(title, query).map(|score| (score, *idx)))
        .collect();
    top_k(matched, want)
}

/// Prefix lookup on a title-ordered listing: binary-search each case variant
/// of the query and walk forward while titles still start with it. Costs
/// O(variants × (log n + want)) directory reads regardless of archive size.
fn prefix_candidates(
    zim: &Zim,
    listing: &zim::Listing<'_>,
    articles_only: bool,
    query: &SearchQuery,
    want: usize,
    deadline: &Deadline,
) -> Vec<(f64, u32)> {
    let Ok(len) = listing.len() else {
        return Vec::new();
    };
    let load = |pos: usize| -> Option<(u32, DirectoryEntry)> {
        let idx = listing.get(pos).ok()??;
        zim.get_by_url_index(idx).ok().map(|entry| (idx, entry))
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    'variants: for variant in case_variants(&query.display) {
        // Old archives keep articles in `A`, new ones in `C`.
        for ns in [b'A', b'C'] {
            if deadline.expired() {
                break 'variants;
            }
            let Some(start) = lower_bound(len, |pos| {
                load(pos).map(|(_, entry)| {
                    (entry.namespace.as_byte(), entry_display_title(&entry).as_bytes())
                        .cmp(&(ns, variant.as_bytes()))
                })
            }) else {
                continue;
            };

            let mut taken = 0;
            for pos in start..len {
                if taken >= want || deadline.expired() {
                    break;
                }
                let Some((idx, entry)) = load(pos) else {
                    break;
                };
                if entry.namespace.as_byte() != ns || !entry_display_title(&entry).starts_with(&variant) {
                    break;
                }
                if !is_article_candidate(&entry, articles_only) || !seen.insert(idx) {
                    continue;
                }
                if let Some(score) = match_score(entry_display_title(&entry), query) {
                    out.push((score, idx));
                    taken += 1;
                }
            }
        }
    }
    out
}

/// First position whose key is not `Less`, or `None` if a read fails.
fn lower_bound(len: usize, mut cmp: impl FnMut(usize) -> Option<Ordering>) -> Option<usize> {
    let (mut low, mut high) = (0usize, len);
    while low < high {
        let mid = low + (high - low) / 2;
        match cmp(mid)? {
            Ordering::Less => low = mid + 1,
            _ => high = mid,
        }
    }
    Some(low)
}

/// Linear scan in URL order for archives without a title listing. Bounded by
/// the deadline; returns whether it ran out of time.
fn scan_candidates(
    zim: &Zim,
    query: &SearchQuery,
    want: usize,
    deadline: &Deadline,
) -> (Vec<(f64, u32)>, bool) {
    let mut out = Vec::new();
    for idx in 0..zim.header.article_count {
        if idx % 512 == 0 && deadline.expired() {
            return (top_k(out, want), true);
        }
        let Ok(entry) = zim.get_by_url_index(idx) else {
            continue;
        };
        if !is_article_candidate(&entry, false) {
            continue;
        }
        if let Some(score) = match_score(entry_display_title(&entry), query) {
            out.push((score, idx));
            // Keep memory bounded on very common queries.
            if out.len() >= want * 20 {
                out = top_k(out, want);
            }
        }
    }
    (top_k(out, want), false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> SearchQuery {
        SearchQuery::new(text).expect("non-empty query")
    }

    fn hit(kind: SearchKind, title: &str, score: f64, link: &str) -> SearchHit {
        SearchHit {
            kind,
            title: title.to_string(),
            subtitle: None,
            source: "src".to_string(),
            score,
            link: link.to_string(),
            lat: None,
            lng: None,
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fyr-search-{}-{}", label, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn query_normalisation_handles_blank_unicode_and_length() {
        assert!(SearchQuery::new("").is_none());
        assert!(SearchQuery::new("   \t\n ").is_none());
        let query = q("  Ärzte   OHNE_Grenzen ");
        assert_eq!(query.display, "Ärzte OHNE_Grenzen");
        assert_eq!(query.folded, "ärzte ohne grenzen");
        assert_eq!(query.tokens, vec!["ärzte", "ohne", "grenzen"]);
        let long = "x".repeat(10_000);
        assert_eq!(q(&long).char_len, MAX_QUERY_CHARS);
        // Multi-byte input is truncated on character boundaries.
        let long_unicode = "ü".repeat(500);
        assert_eq!(q(&long_unicode).display.chars().count(), MAX_QUERY_CHARS);
    }

    #[test]
    fn scoring_prefers_exact_then_prefix_then_word_then_substring() {
        let query = q("berg");
        let exact = match_score("Berg", &query).unwrap();
        let prefix = match_score("Bergen", &query).unwrap();
        let word = match_score("Big Berg Lake", &query).unwrap();
        let substring = match_score("Heidelberg", &query).unwrap();
        assert!(exact > prefix && prefix > word && word > substring, "{exact} {prefix} {word} {substring}");
        assert!(match_score("Oslo", &query).is_none());
        // Shorter titles win within a tier.
        assert!(match_score("Bergen", &query).unwrap() > match_score("Bergenfield Township", &query).unwrap());
    }

    #[test]
    fn scoring_is_case_and_unicode_insensitive() {
        assert!(match_score("ÅLESUND", &q("ålesund")).unwrap() >= SCORE_EXACT - 1.0);
        // No full Unicode case folding (ß vs SS), but it must not panic.
        let _ = match_score("straße", &q("STRASSE"));
        assert!(match_score("Città del Vaticano", &q("città")).unwrap() >= SCORE_PREFIX - MAX_LENGTH_PENALTY);
        assert!(match_score("Tōkyō_Tower", &q("tōkyō tower")).unwrap() >= SCORE_EXACT - 1.0);
    }

    #[test]
    fn multi_token_queries_match_words_in_any_order() {
        let query = q("einstein albert");
        let score = match_score("Albert Einstein", &query).unwrap();
        assert!((SCORE_ALL_TOKENS - MAX_LENGTH_PENALTY..=SCORE_ALL_TOKENS).contains(&score));
        assert!(match_score("Albert Camus", &query).is_none());
    }

    #[test]
    fn single_character_queries_do_not_match_mid_word() {
        let query = q("a");
        assert!(match_score("Alps", &query).is_some());
        assert!(match_score("Big Atlas", &query).is_some());
        assert!(match_score("Oslo map", &query).is_none());
    }

    #[test]
    fn word_start_detection() {
        assert!(contains_at_word_start("the alps guide", "alps"));
        assert!(contains_at_word_start("(alps)", "alps"));
        assert!(!contains_at_word_start("palps", "alps"));
        assert!(contains_at_word_start("palps alps", "alps"));
        assert!(!contains_at_word_start("anything", ""));
    }

    #[test]
    fn case_variants_cover_title_case() {
        let variants = case_variants("albert ein");
        assert!(variants.contains(&"Albert ein".to_string()));
        assert!(variants.contains(&"Albert Ein".to_string()));
        assert!(variants.contains(&"albert ein".to_string()));
        assert!(variants.contains(&"Albert_ein".to_string()));
        let unique: HashSet<_> = variants.iter().collect();
        assert_eq!(unique.len(), variants.len());
        assert!(case_variants("über").contains(&"Über".to_string()));
    }

    #[test]
    fn merge_dedupes_by_link_keeping_best_score() {
        let hits = vec![
            hit(SearchKind::Article, "A", 500.0, "/books?file=a"),
            hit(SearchKind::Book, "A", 900.0, "/books?file=a"),
            hit(SearchKind::Map, "M", 700.0, "/maps?map=m"),
        ];
        let (results, total, truncated) = merge_results(hits, 10, 5);
        assert_eq!(total, 2);
        assert!(!truncated);
        assert_eq!(results[0].kind, SearchKind::Book);
        assert_eq!(results[0].score, 900.0);
        assert_eq!(results[1].kind, SearchKind::Map);
    }

    #[test]
    fn merge_reserves_slots_per_kind_and_applies_limit() {
        let mut hits: Vec<SearchHit> = (0..50)
            .map(|i| hit(SearchKind::Article, &format!("art {i}"), 900.0 - i as f64, &format!("/a/{i}")))
            .collect();
        hits.push(hit(SearchKind::Book, "weak book", 100.0, "/b/1"));
        hits.push(hit(SearchKind::Poi, "weak poi", 50.0, "/p/1"));
        hits.push(hit(SearchKind::Content, "nan", f64::NAN, "/c/nan"));

        let (results, total, truncated) = merge_results(hits, 10, 2);
        assert_eq!(total, 52, "NaN scores are dropped");
        assert!(truncated);
        assert_eq!(results.len(), 10);
        assert!(results.iter().any(|h| h.kind == SearchKind::Book));
        assert!(results.iter().any(|h| h.kind == SearchKind::Poi));
        assert_eq!(results.iter().filter(|h| h.kind == SearchKind::Article).count(), 8);
        // Sorted by score descending.
        assert!(results.windows(2).all(|w| w[0].score >= w[1].score));
        // The best articles are the ones kept.
        assert_eq!(results[0].title, "art 0");
    }

    #[test]
    fn merge_with_limit_smaller_than_kind_count() {
        let hits = vec![
            hit(SearchKind::Book, "b", 10.0, "/b"),
            hit(SearchKind::Map, "m", 20.0, "/m"),
            hit(SearchKind::Poi, "p", 30.0, "/p"),
        ];
        let (results, total, truncated) = merge_results(hits, 1, 5);
        assert_eq!((results.len(), total, truncated), (1, 3, true));
        assert_eq!(results[0].title, "p");
    }

    #[test]
    fn parse_kinds_filter() {
        assert_eq!(parse_kinds(None).len(), 5);
        assert_eq!(parse_kinds(Some("  ")).len(), 5);
        let only = parse_kinds(Some("book, Maps,bogus"));
        assert_eq!(only, [SearchKind::Book, SearchKind::Map].into_iter().collect());
        assert!(parse_kinds(Some("bogus")).is_empty());
    }

    #[test]
    fn links_follow_frontend_contracts() {
        assert_eq!(book_link("My Book.epub"), "/books?file=My%20Book.epub");
        assert_eq!(
            article_link("wiki_en.zim", "A/Albert_Einstein"),
            "/books?file=wiki_en.zim&path=A%2FAlbert_Einstein"
        );
        assert_eq!(content_link("g.md", "a&b=c"), "/books?file=g.md&q=a%26b%3Dc");
        assert_eq!(map_link("alps.pmtiles"), "/maps?map=alps.pmtiles");
        assert_eq!(
            poi_link(59.9139, 10.7522, "Café #1"),
            "/maps?lat=59.9139&lng=10.7522&zoom=14&label=Caf%C3%A9%20%231"
        );
        assert_eq!(poi_link(-0.0, 0.0, "x"), "/maps?lat=0&lng=0&zoom=14&label=x");
    }

    #[test]
    fn humanize_filenames() {
        assert_eq!(humanize_filename("alps_hiking-guide.v2.pmtiles"), "alps hiking guide v2");
        assert_eq!(humanize_filename("noext"), "noext");
    }

    #[test]
    fn stamped_cache_evicts_lru_and_invalidates_on_stamp_change() {
        let mut cache: StampedCache<u32> = StampedCache::new(10);
        let stamp = FileStamp { len: 1, modified: None };
        cache.insert("a".into(), stamp.clone(), Arc::new(1), 4);
        cache.insert("b".into(), stamp.clone(), Arc::new(2), 4);
        assert!(cache.get(Path::new("a"), &stamp).is_some()); // a is now most recent
        cache.insert("c".into(), stamp.clone(), Arc::new(3), 4); // evicts b
        assert!(cache.get(Path::new("b"), &stamp).is_none());
        assert!(cache.get(Path::new("a"), &stamp).is_some());
        assert!(cache.total_weight <= 10);

        let changed = FileStamp { len: 2, modified: None };
        assert!(cache.get(Path::new("a"), &changed).is_none());
        assert!(cache.get(Path::new("a"), &stamp).is_none(), "stale entry dropped");

        cache.insert("huge".into(), stamp.clone(), Arc::new(9), 11);
        assert!(cache.get(Path::new("huge"), &stamp).is_none(), "oversized values are not cached");
    }

    #[test]
    fn lower_bound_finds_first_not_less() {
        let data = [1, 3, 3, 5, 7];
        let find = |target: i32| lower_bound(data.len(), |i| Some(data[i].cmp(&target)));
        assert_eq!(find(0), Some(0));
        assert_eq!(find(3), Some(1));
        assert_eq!(find(4), Some(3));
        assert_eq!(find(8), Some(5));
        assert_eq!(lower_bound(5, |_| None), None);
        assert_eq!(lower_bound(0, |_| None), Some(0));
    }

    #[test]
    fn expired_deadline_and_cancellation() {
        assert!(Deadline::after(Duration::ZERO).expired());
        let flag = Arc::new(AtomicBool::new(false));
        let deadline = Deadline::new(Duration::from_secs(60), Arc::clone(&flag));
        assert!(!deadline.expired());
        let child = deadline.child(Duration::from_secs(600));
        assert!(child.remaining() <= deadline.remaining() + Duration::from_millis(5));
        flag.store(true, AtomicOrdering::Relaxed);
        assert!(deadline.expired() && child.expired());
    }

    #[test]
    fn geojson_parsing_handles_shapes_and_malformed_input() {
        let doc = br#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[10.75,59.91]},
                 "properties":{"name":"Oslo Hut","description":"Cosy cabin by the fjord"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[10,60],[12,62]]},
                 "properties":{"title":"Ridge Trail"}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[2,0],[2,2],[0,2]]]},
                 "properties":{"name": 42}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[999,999]},"properties":{"name":"Bad coords"}},
                {"type":"Feature","geometry":null,"properties":{"name":"No geometry"}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},"properties":{}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":["a","b"]},"properties":{"name":"Strings"}},
                {"type":"Feature","geometry":{"type":"GeometryCollection","geometries":[
                    {"type":"Point","coordinates":[4,4]},{"type":"Point","coordinates":[6,6]}]},
                 "properties":{"name":"Collection"}},
                "not a feature",
                null
            ]
        }"#;
        let records = parse_poi_records(doc).expect("parse");
        let names: Vec<&str> = records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Oslo Hut", "Ridge Trail", "42", "Collection"]);
        assert_eq!((records[0].lat, records[0].lng), (59.91, 10.75));
        assert_eq!((records[1].lat, records[1].lng), (61.0, 11.0));
        assert_eq!((records[3].lat, records[3].lng), (5.0, 5.0));

        assert!(parse_poi_records(b"{not json").is_err());
        assert!(parse_poi_records(b"[]").unwrap().is_empty());
        assert!(parse_poi_records(br#"{"type":"Topology"}"#).is_err());
        assert!(parse_poi_records(b"42").is_err());
        let single = br#"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"name":"Solo"}}"#;
        assert_eq!(parse_poi_records(single).unwrap().len(), 1);
    }

    #[test]
    fn poi_search_matches_names_and_descriptions() {
        let doc = br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[10.75,59.91]},
             "properties":{"name":"Oslo Hut","description":"Cosy cabin by the fjord"}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[5.32,60.39]},
             "properties":{"name":"Bergen Station"}}
        ]}"#;
        let records = parse_poi_records(doc).unwrap();

        let by_name = search_poi_records(&records, "huts.geojson", &q("oslo"), 10);
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].kind, SearchKind::Poi);
        assert_eq!(by_name[0].link, "/maps?lat=59.91&lng=10.75&zoom=14&label=Oslo%20Hut");
        assert_eq!(by_name[0].lat, Some(59.91));

        let by_desc = search_poi_records(&records, "huts.geojson", &q("fjord"), 10);
        assert_eq!(by_desc.len(), 1);
        assert!(by_desc[0].score < by_name[0].score);
        assert!(search_poi_records(&records, "huts.geojson", &q("fj"), 10).is_empty(), "short queries skip descriptions");

        let limited = search_poi_records(&records, "huts.geojson", &q("o"), 1);
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn file_sources_search_real_temp_files() {
        let root = temp_dir("sources");
        let roots = SearchRoots {
            books: root.join("books"),
            maps: root.join("maps"),
            poi: root.join("poi"),
        };
        for dir in [&roots.books, &roots.maps, &roots.poi] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(
            roots.books.join("field-guide.md"),
            "# Alpine Field Guide\n\nHow to purify water in the mountains.\nMore about Wasserfälle.\n",
        )
        .unwrap();
        std::fs::write(roots.books.join("notes.md"), "no heading here, just water notes").unwrap();
        std::fs::write(roots.books.join("broken.epub"), "not a zip").unwrap();
        std::fs::write(roots.books.join("broken.zim"), "not a zim").unwrap();
        std::fs::write(roots.books.join("ignored.txt"), "alpine").unwrap();
        std::fs::write(roots.maps.join("alpine_region.pmtiles"), "PMTiles").unwrap();
        std::fs::write(roots.maps.join("readme.txt"), "alpine").unwrap();
        std::fs::write(
            roots.poi.join("huts.geojson"),
            br#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[7.5,46.5]},"properties":{"name":"Alpine Hut"}}]}"#,
        )
        .unwrap();
        std::fs::write(roots.poi.join("broken.geojson"), "{{{").unwrap();
        std::fs::write(roots.poi.join("tracks.fgb"), "binary").unwrap();

        let all = parse_kinds(None);
        let deadline = Deadline::after(SEARCH_BUDGET);
        let outcome = run_search(&roots, &q("alpine"), &all, &deadline);
        let kinds: HashSet<SearchKind> = outcome.hits.iter().map(|h| h.kind).collect();
        assert!(kinds.contains(&SearchKind::Book), "{:?}", outcome.hits);
        assert!(kinds.contains(&SearchKind::Map));
        assert!(kinds.contains(&SearchKind::Poi));
        assert!(kinds.contains(&SearchKind::Content));
        assert!(!outcome.hits.iter().any(|h| h.source.ends_with(".txt")));
        let book = outcome.hits.iter().find(|h| h.kind == SearchKind::Book).unwrap();
        assert_eq!(book.title, "Alpine Field Guide");
        assert_eq!(book.link, "/books?file=field-guide.md");

        // Full-text hits: unicode content, link carries the query.
        let water = run_search(&roots, &q("WASSERFÄLLE"), &all, &deadline);
        let content: Vec<_> = water.hits.iter().filter(|h| h.kind == SearchKind::Content).collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].link, "/books?file=field-guide.md&q=WASSERF%C3%84LLE");
        assert!(content[0].subtitle.as_deref().unwrap().contains("Wasserfälle"));

        // Book with no heading is found by humanized filename.
        let notes = run_search(&roots, &q("notes"), &[SearchKind::Book].into_iter().collect(), &deadline);
        assert_eq!(notes.hits.len(), 1);
        assert_eq!(notes.hits[0].title, "notes");

        // Kinds filter restricts sources.
        let maps_only = run_search(&roots, &q("alpine"), &[SearchKind::Map].into_iter().collect(), &deadline);
        assert!(maps_only.hits.iter().all(|h| h.kind == SearchKind::Map));
        assert_eq!(maps_only.hits[0].link, "/maps?map=alpine_region.pmtiles");

        // An expired budget returns quickly and flags partial results.
        let expired = Deadline::after(Duration::ZERO);
        let partial = run_search(&roots, &q("alpine"), &all, &expired);
        assert!(partial.partial);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_directories_yield_no_results() {
        let root = temp_dir("missing");
        let roots = SearchRoots {
            books: root.join("nope-books"),
            maps: root.join("nope-maps"),
            poi: root.join("nope-poi"),
        };
        let outcome = run_search(&roots, &q("anything"), &parse_kinds(None), &Deadline::after(SEARCH_BUDGET));
        assert!(outcome.hits.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
    /// Exercises the ZIM paths against real archives when
    /// `FYR_TEST_ZIM_DIR` points at a directory of `.zim` files (for example
    /// the openZIM testing suite archives). Skipped otherwise, since there is
    /// no ZIM writer to build fixtures with.
    #[test]
    fn zim_title_search_against_fixture_archives() {
        let Some(dir) = std::env::var_os("FYR_TEST_ZIM_DIR") else {
            eprintln!("FYR_TEST_ZIM_DIR not set; skipping ZIM fixture test");
            return;
        };
        let mut checked = 0;
        for (filename, path) in list_files(Path::new(&dir)) {
            if extension_lower(&filename) != "zim" {
                continue;
            }
            let zim = open_zim(&path).expect("fixture opens");
            let listing = title_listing(&zim);
            let index = build_title_index(&zim, listing.as_ref(), &Deadline::after(Duration::from_secs(30)))
                .expect("index builds");
            // Pick a real article title and search for a prefix of it.
            let Some((title, _)) = index
                .entries
                .iter()
                .find(|(t, _)| t.chars().count() >= 4 && t.chars().next().is_some_and(char::is_alphabetic))
            else {
                continue;
            };
            let prefix: String = title.chars().take(4).collect();
            let query = q(&prefix);

            let book = BookFile {
                filename: filename.clone(),
                path: path.clone(),
                format: "zim",
                title: None,
                size: 0,
            };
            let outcome = search_zim_archive(&book, &query, PER_ARCHIVE_LIMIT, &Deadline::after(SEARCH_BUDGET));
            assert!(!outcome.hits.is_empty(), "{filename}: no hits for {prefix:?}");
            for hit in &outcome.hits {
                assert_eq!(hit.kind, SearchKind::Article);
                assert!(hit.link.starts_with(&format!("/books?file={}&path=", encode_component(&filename))));
                assert!(!hit.link.contains(&*path.to_string_lossy()));
            }
            let links: HashSet<&str> = outcome.hits.iter().map(|h| h.link.as_str()).collect();
            assert_eq!(links.len(), outcome.hits.len(), "{filename}: duplicate article paths");

            // The listing-based prefix lookup (used for archives too large to
            // index) must agree that the prefix exists.
            if let Some((listing, articles_only)) = listing.as_ref() {
                let found = prefix_candidates(&zim, listing, *articles_only, &query, 10, &Deadline::after(SEARCH_BUDGET));
                assert!(!found.is_empty(), "{filename}: prefix lookup found nothing for {prefix:?}");
            }
            // And the URL-order scan fallback.
            let (scanned, timed_out) = scan_candidates(&zim, &query, 10, &Deadline::after(SEARCH_BUDGET));
            assert!(!timed_out);
            assert!(!scanned.is_empty(), "{filename}: scan found nothing");
            eprintln!(
                "{filename}: {} entries, {} hits for {prefix:?}: {:?}",
                index.entries.len(),
                outcome.hits.len(),
                outcome.hits.iter().take(3).map(|h| (&h.title, &h.link)).collect::<Vec<_>>()
            );
            checked += 1;
        }
        assert!(checked > 0, "no usable .zim files in FYR_TEST_ZIM_DIR");
    }
}
