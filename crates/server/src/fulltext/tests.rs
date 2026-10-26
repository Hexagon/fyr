//! Full-text index tests against real (hand-built) ZIM archives.
//!
//! Jobs are run synchronously with `run_job` wherever possible, so results
//! do not depend on a background thread's timing; the worker-thread tests
//! wait explicitly for the state they assert.

use super::store::{self, IndexWriter, QueryError, TextHit};
use super::worker::{run_job, BuildOutcome, CancelReason, IndexError, IndexerSettings, JobControl, JobOutcome};
use super::*;
use crate::search::{self, Deadline, SearchKind, SearchQuery, SearchRoots};
use crate::test_zim::{write_zim, TestArticle, TestRedirect};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    settings: IndexerSettings,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("fyr-fulltext-{label}-{}", uuid::Uuid::new_v4()));
        let books = root.join("books");
        std::fs::create_dir_all(&books).unwrap();
        let settings = IndexerSettings {
            index_dir: root.join("index"),
            books_dir: books,
            enabled: true,
            auto_max_articles: 1000,
            min_free_bytes: 0,
            throttle: 0.0,
            chunk_entries: 500,
            rescan_interval: Duration::from_millis(50),
        };
        Self { root, settings }
    }

    fn archive(&self, name: &str) -> PathBuf {
        self.settings.books_dir.join(name)
    }

    fn index(&self, name: &str) -> PathBuf {
        index_path(&self.settings.index_dir, name)
    }

    fn run(&self, name: &str) -> JobOutcome {
        run_job(&self.settings, name, &Arc::new(JobControl::new(true)))
    }

    fn reader(&self, name: &str) -> Option<IndexReader> {
        open_reader(&self.settings.index_dir, name, &self.archive(name))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn far() -> Interrupt {
    Interrupt::at(Instant::now() + Duration::from_secs(30))
}

fn paths(hits: &[TextHit]) -> Vec<&str> {
    hits.iter().map(|h| h.path.as_str()).collect()
}

fn html(title: &str, body: &str) -> String {
    format!("<html><head><title>{title}</title><style>p {{ color: red }}</style></head><body><h1>{title}</h1>{body}</body></html>")
}

/// Wiki with body-only facts: no title mentions the words searched for.
fn write_wiki(path: &Path) {
    write_zim(
        path,
        &[
            TestArticle::new(
                "Alps",
                "Alps",
                &html(
                    "Alps",
                    "<p>The Alps are a mountain range. The city of Zürich lies north of them.</p>\
                     <ul><li>Highest: Mont Blanc</li></ul>\
                     <script>var hidden = 'glacierword';</script>",
                ),
            ),
            TestArticle::new(
                "Coronation",
                "Coronation",
                &html("Coronation", "<p>Swedish kings were crowned in Uppsala Cathedral for centuries.</p>"),
            ),
            TestArticle::new(
                "Kings_of_Sweden",
                "Kings of Sweden",
                &html("Kings of Sweden", "<p>The list of kings starts in the early Middle Ages.</p>"),
            ),
            TestArticle::new(
                "Markup",
                "Markup",
                &html("Markup", "<p>Escaping example: &lt;script&gt;alert(1)&lt;/script&gt; is shown as text.</p>"),
            ),
        ],
        &[TestRedirect::new("Zurich_redirect", "Zurich", "Alps")],
    );
}

// --- pure helpers ---------------------------------------------------------

#[test]
fn query_expressions_quote_every_term() {
    assert_eq!(all_terms_expr("Zürich  lake").as_deref(), Some("\"zurich\" \"lake\" *"));
    assert_eq!(all_terms_expr("go").as_deref(), Some("\"go\""), "short last term is not a prefix");
    assert_eq!(all_terms_expr("  ¡¿ "), None);
    // FTS5 syntax in user input becomes plain quoted terms.
    let hostile = all_terms_expr("title:x OR body NEAR(\"a\" b) -c ^d * AND NOT").unwrap();
    assert_eq!(hostile, "\"title\" \"x\" \"or\" \"body\" \"near\" \"a\" \"b\" \"c\" \"d\" \"and\" \"not\" *");
    assert_eq!(
        any_terms_expr(&["water".into(), "sea-level".into(), "water".into()]).as_deref(),
        Some("\"water\" OR \"sea\" OR \"level\"")
    );
    assert_eq!(any_terms_expr(&[]), None);
    assert!(query_terms(&"word ".repeat(100)).len() == 1);
    assert_eq!(query_terms(&"x".repeat(500))[0].chars().count(), 64);
}

#[test]
fn index_names_are_unique_and_path_free() {
    let a = index_file_name("wikipedia_en_all.zim");
    assert!(a.starts_with("wikipedia_en_all-") && a.ends_with(".fts.sqlite"), "{a}");
    assert_ne!(index_file_name("a b.zim"), index_file_name("a_b.zim"));
    assert!(!index_file_name("../../etc/passwd.zim").contains('/'));
    assert!(is_valid_archive_name("wiki.ZIM"));
    for bad in ["", ".hidden.zim", "a/b.zim", "a\\b.zim", "book.epub", "..", "x.zim\0"] {
        assert!(!is_valid_archive_name(bad), "{bad:?}");
    }
}

#[test]
fn progress_counts_scan_then_storage() {
    assert_eq!(worker::progress_percent(0, 0, 0, 0), 0.0);
    assert_eq!(worker::progress_percent(50, 100, 0, 10), 5.0);
    assert_eq!(worker::progress_percent(100, 100, 0, 10), 10.0);
    assert_eq!(worker::progress_percent(100, 100, 5, 5), 55.0);
    assert_eq!(worker::progress_percent(100, 100, 10, 0), 100.0);
    assert_eq!(worker::progress_percent(100, 100, 0, 0), 100.0);
}

#[test]
fn snippet_segments_split_markers_and_keep_markup_as_text() {
    let raw = format!("a {HIGHLIGHT_START}<b>x</b>{HIGHLIGHT_END} c\nd{HIGHLIGHT_START}{HIGHLIGHT_END}\u{7}");
    let segments = snippet_segments(&raw);
    assert_eq!(
        segments,
        vec![
            SnippetSegment { text: "a ".into(), matched: false },
            SnippetSegment { text: "<b>x</b>".into(), matched: true },
            SnippetSegment { text: " c d".into(), matched: false },
        ]
    );
    let json = serde_json::to_string(&segments[1]).unwrap();
    assert_eq!(json, r#"{"text":"<b>x</b>","match":true}"#);
}

#[test]
fn article_text_drops_scripts_markers_and_caps_size() {
    let text = article_text(&format!(
        "<p>One {HIGHLIGHT_START}two</p><script>secret()</script><li>Short</li><p>{}</p>",
        "word ".repeat(200_000)
    ));
    assert!(text.starts_with("One two\nShort\nword word"), "{}", &text[..40]);
    assert!(!text.contains("secret"));
    assert!(text.len() <= MAX_BODY_BYTES);
    assert!(text.len() > MAX_BODY_BYTES - 16);
}

// --- indexing ---------------------------------------------------------------

#[test]
fn indexes_an_archive_end_to_end_with_accent_insensitive_matching() {
    let fx = Fixture::new("e2e");
    write_wiki(&fx.archive("wiki.zim"));
    assert_eq!(fx.run("wiki.zim"), JobOutcome::Completed { articles: 4 });

    let reader = fx.reader("wiki.zim").expect("usable index");
    assert_eq!(reader.meta.state, IndexState::Ready);
    assert_eq!(reader.meta.articles, 4, "redirects and metadata are not indexed");
    assert_eq!(reader.meta.cursor, reader.meta.total);

    let search = |q: &str| reader.search(&all_terms_expr(q).unwrap(), 10, &far()).unwrap();
    // Body-only words, any case and accents, and prefixes.
    for q in ["zurich", "ZÜRICH", "Zurich", "zür", "mountain range"] {
        assert_eq!(paths(&search(q)), vec!["Alps"], "{q}");
    }
    // Short list items are indexed; scripts and styles are not.
    assert_eq!(paths(&search("mont blanc")), vec!["Alps"]);
    assert!(search("glacierword").is_empty());
    assert!(search("color red").is_empty());
    // Titles are indexed (and weighted above body matches).
    assert_eq!(paths(&search("kings")), vec!["Kings_of_Sweden", "Coronation"]);

    let hit = &search("uppsala cathedral")[0];
    assert_eq!(hit.title, "Coronation");
    let segments = snippet_segments(&hit.snippet);
    let marked: Vec<&str> = segments.iter().filter(|s| s.matched).map(|s| s.text.as_str()).collect();
    assert_eq!(marked, vec!["Uppsala", "Cathedral"]);

    // Markup in article text stays literal text in snippets (the UI renders
    // segments as text, never HTML).
    let markup = &search("alert")[0];
    let text: String = snippet_segments(&markup.snippet).into_iter().map(|s| s.text).collect();
    assert!(text.contains("<script>alert(1)</script>"), "{text}");

    // Documents carry the stored text for retrieval.
    let docs = reader.documents(&any_terms_expr(&["crowned".into()]).unwrap(), 5, &far()).unwrap();
    assert_eq!(docs.len(), 1);
    assert!(docs[0].body.contains("crowned in Uppsala Cathedral"));
}

#[test]
fn hostile_queries_never_error() {
    let fx = Fixture::new("hostile");
    write_wiki(&fx.archive("wiki.zim"));
    fx.run("wiki.zim");
    let reader = fx.reader("wiki.zim").unwrap();
    for q in [
        "\"", "\"\"\"", "a\" OR \"b", "title:alps", "alps*", "NEAR(alps mountain, 2)", "-alps", "^alps", "alps AND",
        "(", ")", "{title}: alps", "'; DROP TABLE articles; --", "\u{E000}alps\u{E001}",
    ] {
        if let Some(expr) = all_terms_expr(q) {
            reader.search(&expr, 10, &far()).unwrap_or_else(|e| panic!("{q:?} → {expr}: {e:?}"));
        }
    }
    // The table survived the injection attempt.
    assert_eq!(reader.search(&all_terms_expr("alps").unwrap(), 10, &far()).unwrap().len(), 1);
}

#[test]
fn expired_interrupt_stops_queries() {
    let fx = Fixture::new("interrupt");
    write_wiki(&fx.archive("wiki.zim"));
    fx.run("wiki.zim");
    let reader = fx.reader("wiki.zim").unwrap();
    let past = Interrupt::at(Instant::now() - Duration::from_millis(1));
    assert!(matches!(reader.search("\"alps\"", 10, &past), Err(QueryError::Interrupted)));
}

#[test]
fn replaced_archive_is_ignored_then_reindexed() {
    let fx = Fixture::new("replace");
    write_wiki(&fx.archive("wiki.zim"));
    fx.run("wiki.zim");
    assert!(fx.reader("wiki.zim").is_some());

    write_zim(
        &fx.archive("wiki.zim"),
        &[TestArticle::new("Volcano", "Volcano", &html("Volcano", "<p>Lava flows from the crater of the volcano.</p>"))],
        &[],
    );
    assert!(fx.reader("wiki.zim").is_none(), "stale index must not be used");
    let stale = store::inspect(&fx.index("wiki.zim")).unwrap();
    assert!(!stale.matches("wiki.zim", &ArchiveStamp::of(&fx.archive("wiki.zim")).unwrap()));

    assert_eq!(fx.run("wiki.zim"), JobOutcome::Completed { articles: 1 });
    let reader = fx.reader("wiki.zim").unwrap();
    assert_eq!(paths(&reader.search("\"lava\"", 10, &far()).unwrap()), vec!["Volcano"]);
    assert!(reader.search("\"zurich\"", 10, &far()).unwrap().is_empty());
}

#[test]
fn interrupted_builds_resume_from_the_cursor_without_duplicates() {
    let mut fx = Fixture::new("resume");
    let articles: Vec<TestArticle> = (0..30)
        .map(|i| TestArticle::new(&format!("Page_{i:02}"), &format!("Page {i}"), &html("x", &format!("<p>common text number{i}</p>"))))
        .collect();
    write_zim(&fx.archive("big.zim"), &articles, &[]);
    fx.settings.chunk_entries = 7;

    let control = Arc::new(JobControl { max_chunks: Some(2), ..JobControl::new(true) });
    assert_eq!(run_job(&fx.settings, "big.zim", &control), JobOutcome::Cancelled(CancelReason::Yield));
    let meta = store::inspect(&fx.index("big.zim")).unwrap();
    assert_eq!(meta.state, IndexState::Building);
    assert_eq!(meta.cursor, meta.total, "the directory scan finished");
    assert_eq!((meta.articles, meta.pending), (14, 16));
    // A partial index already answers queries.
    let partial = fx.reader("big.zim").expect("partial index is usable");
    assert_eq!(partial.search("\"common\"", 100, &far()).unwrap().len(), 14);
    drop(partial);
    let status = FulltextIndexer::new(fx.settings.clone()).status();
    assert_eq!(status.archives[0].state, "paused");
    assert_eq!(status.archives[0].percent, worker::progress_percent(meta.total, meta.total, 14, 16));

    // A cancelled job rolls back its open batch: nothing moves.
    let cancelled = Arc::new(JobControl::new(true));
    cancelled.cancel(CancelReason::Shutdown);
    assert_eq!(run_job(&fx.settings, "big.zim", &cancelled), JobOutcome::Cancelled(CancelReason::Shutdown));
    let after = store::inspect(&fx.index("big.zim")).unwrap();
    assert_eq!((after.articles, after.pending), (14, 16));

    assert_eq!(fx.run("big.zim"), JobOutcome::Completed { articles: 30 });
    let reader = fx.reader("big.zim").unwrap();
    let hits = reader.search("\"common\"", 100, &far()).unwrap();
    assert_eq!(hits.len(), 30);
    assert_eq!(hits.iter().map(|h| &h.path).collect::<HashSet<_>>().len(), 30, "no duplicate rows");
    assert_eq!(reader.meta.state, IndexState::Ready);
}

#[test]
fn readers_see_committed_rows_while_a_batch_is_open() {
    let fx = Fixture::new("concurrent");
    write_wiki(&fx.archive("wiki.zim"));
    fx.run("wiki.zim");
    let (mut writer, _) = IndexWriter::open(&fx.index("wiki.zim")).unwrap();
    let batch = writer.batch().unwrap();
    batch.insert("Extra", "Extra", "uncommitted zurich text").unwrap();
    // WAL: a concurrent reader is not blocked and sees the last commit only.
    let reader = fx.reader("wiki.zim").unwrap();
    assert_eq!(paths(&reader.search("\"zurich\"", 10, &far()).unwrap()), vec!["Alps"]);
    drop(batch);
    assert_eq!(paths(&reader.search("\"zurich\"", 10, &far()).unwrap()), vec!["Alps"]);
}

#[test]
fn disk_space_guard_stops_indexing() {
    let mut fx = Fixture::new("disk");
    write_wiki(&fx.archive("wiki.zim"));
    fx.settings.min_free_bytes = u64::MAX;
    assert_eq!(fx.run("wiki.zim"), JobOutcome::LowDisk);
    assert!(!fx.index("wiki.zim").exists());
}

#[test]
fn corrupt_archives_fail_once_and_are_not_retried_automatically() {
    let fx = Fixture::new("corrupt");
    std::fs::write(fx.archive("broken.zim"), b"ZIM? not really, just some bytes").unwrap();
    assert!(matches!(fx.run("broken.zim"), JobOutcome::Failed(_)));
    let meta = store::inspect(&fx.index("broken.zim")).unwrap();
    assert_eq!(meta.state, IndexState::Failed);
    assert!(meta.note.is_some_and(|n| !n.contains('/')));
    assert!(fx.reader("broken.zim").is_none());

    let indexer = FulltextIndexer::new(fx.settings.clone());
    let status = indexer.status();
    assert_eq!(status.archives[0].state, "failed");
}

// --- worker thread and admin controls --------------------------------------

fn wait_for(indexer: &FulltextIndexer, file: &str, state: &str) -> worker::ArchiveIndexStatus {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let status = indexer.status();
        let entry = status.archives.iter().find(|a| a.filename == file).cloned().expect("archive listed");
        if entry.state == state {
            return entry;
        }
        assert!(Instant::now() < end, "{file} never reached {state}: {entry:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn worker_indexes_automatically_and_obeys_admin_controls() {
    let mut fx = Fixture::new("worker");
    write_wiki(&fx.archive("small.zim"));
    let big: Vec<TestArticle> = (0..12)
        .map(|i| TestArticle::new(&format!("B{i}"), &format!("B {i}"), &html("b", "<p>bulk article body</p>")))
        .collect();
    write_zim(&fx.archive("big.zim"), &big, &[]);
    fx.settings.auto_max_articles = 6;
    let indexer = FulltextIndexer::new(fx.settings.clone());

    // Before the worker runs: nothing indexed, only the small one is automatic.
    let before = indexer.status();
    let small = before.archives.iter().find(|a| a.filename == "small.zim").unwrap();
    let large = before.archives.iter().find(|a| a.filename == "big.zim").unwrap();
    assert_eq!((small.state, small.auto, small.articles), ("not_indexed", true, Some(5)));
    assert_eq!((large.state, large.auto), ("not_indexed", false));

    indexer.start();
    let ready = wait_for(&indexer, "small.zim", "ready");
    assert_eq!(ready.indexed_articles, 4);
    assert_eq!(ready.percent, 100.0);
    assert!(ready.index_bytes > 0);
    // The large archive is left alone.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(indexer.status().archives.iter().find(|a| a.filename == "big.zim").unwrap().state, "not_indexed");

    // Admin build of the large archive ignores the cap.
    assert_eq!(indexer.request_build("big.zim"), Ok(BuildOutcome::Queued));
    wait_for(&indexer, "big.zim", "ready");
    assert_eq!(indexer.request_build("big.zim"), Ok(BuildOutcome::AlreadyReady));

    // Delete: index gone, archive excluded (also across restarts) until rebuilt.
    indexer.remove("small.zim", Duration::from_secs(10)).unwrap();
    assert!(!fx.index("small.zim").exists());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(wait_for(&indexer, "small.zim", "excluded").indexed_articles, 0);
    let restarted = FulltextIndexer::new(fx.settings.clone());
    assert_eq!(restarted.status().archives.iter().find(|a| a.filename == "small.zim").unwrap().state, "excluded");
    assert_eq!(indexer.request_build("small.zim"), Ok(BuildOutcome::Queued));
    wait_for(&indexer, "small.zim", "ready");

    // Deleting the archive drops its index; orphans are cleaned on rescan.
    indexer.forget_archive("big.zim", Duration::from_secs(10));
    std::fs::remove_file(fx.archive("big.zim")).unwrap();
    assert!(!fx.index("big.zim").exists());

    // Errors.
    assert_eq!(indexer.request_build("missing.zim"), Err(IndexError::NotFound));
    assert_eq!(indexer.request_build("../x.zim"), Err(IndexError::InvalidName));
    let disabled = FulltextIndexer::new(IndexerSettings { enabled: false, ..fx.settings.clone() });
    assert_eq!(disabled.request_build("small.zim"), Err(IndexError::Disabled));
    assert!(disabled.reader_dir().is_none());

    let status_json = serde_json::to_string(&indexer.status()).unwrap();
    assert!(!status_json.contains(&*fx.root.to_string_lossy()), "status leaked a path: {status_json}");

    let started = Instant::now();
    indexer.shutdown(Duration::from_secs(5));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn shutdown_cancels_a_running_job_and_leaves_it_resumable() {
    let mut fx = Fixture::new("shutdown");
    let many: Vec<TestArticle> = (0..400)
        .map(|i| TestArticle::new(&format!("P{i:04}"), &format!("P {i}"), &html("p", &"<p>filler words for a slow build</p>".repeat(20))))
        .collect();
    write_zim(&fx.archive("slow.zim"), &many, &[]);
    fx.settings.chunk_entries = 5;
    fx.settings.throttle = 50.0; // slow enough to catch it mid-build (pauses are interruptible)
    let indexer = FulltextIndexer::new(fx.settings.clone());
    indexer.start();
    wait_for(&indexer, "slow.zim", "indexing");
    indexer.shutdown(Duration::from_secs(10));

    let meta = store::inspect(&fx.index("slow.zim")).unwrap();
    assert_eq!(meta.state, IndexState::Building);
    assert!(meta.articles < 400 && meta.articles + meta.pending <= 400, "{meta:?}");
    fx.settings.throttle = 0.0;
    assert_eq!(fx.run("slow.zim"), JobOutcome::Completed { articles: 400 });
}

// --- consumers: global search and assistant retrieval ----------------------

#[test]
fn global_search_merges_article_text_hits() {
    let fx = Fixture::new("search");
    write_wiki(&fx.archive("wiki.zim"));
    let roots = SearchRoots {
        books: fx.settings.books_dir.clone(),
        maps: fx.root.join("maps"),
        poi: fx.root.join("poi"),
        index: Some(fx.settings.index_dir.clone()),
    };
    let all: HashSet<SearchKind> = SearchKind::ALL.into_iter().collect();
    let run = |q: &str| search::run_search(&roots, &SearchQuery::new(q).unwrap(), &all, &Deadline::after(Duration::from_secs(10)));

    // No index yet: body words are not found.
    assert!(run("zurich").hits.iter().all(|h| h.kind != SearchKind::ArticleText));

    fx.run("wiki.zim");
    // "Zurich" is also the title of a redirect to Alps: the title hit wins
    // and the body hit for the same article is dropped as a duplicate.
    let zurich = run("zurich");
    assert!(zurich.hits.iter().any(|h| h.kind == SearchKind::Article && h.link == "/books?file=wiki.zim&path=Alps"));
    assert!(zurich.hits.iter().all(|h| h.kind != SearchKind::ArticleText), "{:?}", zurich.hits);

    let outcome = run("Mountain RANGE");
    let text: Vec<_> = outcome.hits.iter().filter(|h| h.kind == SearchKind::ArticleText).collect();
    assert_eq!(text.len(), 1, "{:?}", outcome.hits);
    assert_eq!(text[0].title, "Alps");
    assert_eq!(text[0].link, "/books?file=wiki.zim&path=Alps&q=Mountain%20RANGE");
    assert_eq!(text[0].source, "wiki.zim");
    assert_eq!(text[0].subtitle.as_deref(), Some("Test Wiki"));
    let snippet = text[0].snippet.as_ref().unwrap();
    let marked: Vec<&str> = snippet.iter().filter(|s| s.matched).map(|s| s.text.as_str()).collect();
    assert_eq!(marked, vec!["mountain", "range"], "{snippet:?}");
    // Accent-insensitive body match through the global search.
    let cathedral = run("Zürich lies");
    assert!(cathedral.hits.iter().any(|h| h.kind == SearchKind::ArticleText && h.title == "Alps"));

    // Title hits win over body hits for the same article.
    let kings = run("kings");
    let article_links: HashSet<&str> = kings.hits.iter().filter(|h| h.kind == SearchKind::Article).map(|h| h.link.as_str()).collect();
    for hit in kings.hits.iter().filter(|h| h.kind == SearchKind::ArticleText) {
        let base = hit.link.split("&q=").next().unwrap();
        assert!(!article_links.contains(base), "duplicate of a title hit: {}", hit.link);
    }
    let best_title = kings.hits.iter().filter(|h| h.kind == SearchKind::Article).map(|h| h.score).fold(0.0, f64::max);
    let best_text = kings.hits.iter().filter(|h| h.kind == SearchKind::ArticleText).map(|h| h.score).fold(0.0, f64::max);
    assert!(best_title > best_text);

    // Merge keeps the new kind and its JSON shape.
    let (merged, _, _) = search::merge_results(outcome.hits.clone(), 40, search::MIN_PER_KIND);
    let json = serde_json::to_value(merged.iter().find(|h| h.kind == SearchKind::ArticleText).unwrap()).unwrap();
    assert_eq!(json["kind"], "article_text");
    assert!(json["snippet"].as_array().unwrap().iter().any(|s| s["match"] == true));

    // Kinds filter and short queries.
    let only: HashSet<SearchKind> = [SearchKind::ArticleText].into_iter().collect();
    let filtered = search::run_search(&roots, &SearchQuery::new("cathedral").unwrap(), &only, &Deadline::after(Duration::from_secs(5)));
    assert_eq!(filtered.hits.len(), 1);
    assert!(run("zu").hits.iter().all(|h| h.kind != SearchKind::ArticleText), "needs 3+ chars");
    assert_eq!(search::parse_kinds(Some("article_text")), only);

    // An expired budget reports partial instead of blocking.
    let expired = search::run_search(&roots, &SearchQuery::new("cathedral").unwrap(), &only, &Deadline::after(Duration::ZERO));
    assert!(expired.partial);
    assert!(expired.hits.is_empty());

    // Disabled full-text search ignores existing indexes.
    let off = SearchRoots { index: None, ..roots.clone() };
    let none = search::run_search(&off, &SearchQuery::new("cathedral").unwrap(), &only, &Deadline::after(Duration::from_secs(5)));
    assert!(none.hits.is_empty());
}

#[test]
fn retrieval_prefers_full_text_hits_over_title_matches() {
    use crate::ai::retrieval::{retrieve_with_index, RetrievalLimits};
    let fx = Fixture::new("retrieval");
    write_wiki(&fx.archive("wiki.zim"));
    // One article per archive, no shared title index (deterministic).
    let limits = RetrievalLimits {
        max_zim_articles_per_archive: 1,
        use_title_index: false,
        ..Default::default()
    };
    let prompt = "Where were the kings crowned? Uppsala cathedral?";

    // Without an index: the title prefix "Kings" wins, which does not answer.
    let by_title = retrieve_with_index(&fx.settings.books_dir, None, prompt, 4, &limits);
    assert_eq!(by_title[0].zim_path.as_deref(), Some("Kings_of_Sweden"));

    fx.run("wiki.zim");
    let index_dir = fx.settings.index_dir.clone();
    let by_text = retrieve_with_index(&fx.settings.books_dir, Some(&index_dir), prompt, 4, &limits);
    assert_eq!(by_text.len(), 1, "{by_text:#?}");
    assert_eq!(by_text[0].zim_path.as_deref(), Some("Coronation"));
    assert!(by_text[0].text.contains("crowned in Uppsala Cathedral"));
    assert_eq!(by_text[0].link(), "/books?file=wiki.zim&path=Coronation");

    // With more slots, title fallbacks still fill in after the FTS hits.
    let roomy = RetrievalLimits { max_zim_articles_per_archive: 6, ..limits.clone() };
    let both = retrieve_with_index(&fx.settings.books_dir, Some(&index_dir), prompt, 4, &roomy);
    let found: HashSet<_> = both.iter().filter_map(|p| p.zim_path.as_deref()).collect();
    assert!(found.contains("Coronation") && found.contains("Kings_of_Sweden"), "{found:?}");

    // An exhausted time budget yields nothing rather than blocking.
    let none = RetrievalLimits { time_budget: Duration::ZERO, ..limits };
    assert!(retrieve_with_index(&fx.settings.books_dir, Some(&index_dir), prompt, 4, &none).is_empty());
}

/// Indexing throughput on synthetic data (uncompressed archive, so this
/// measures text extraction + FTS5 inserts, not decompression). Run with
/// `cargo test -p server --release -- --ignored fulltext_throughput --nocapture`.
#[test]
#[ignore]
fn fulltext_throughput() {
    let fx = Fixture::new("throughput");
    let words = ["river", "mountain", "kingdom", "harbour", "forest", "language", "railway", "winter", "church", "market"];
    let articles: Vec<TestArticle> = (0..20_000)
        .map(|i| {
            let paragraphs: String = (0..8)
                .map(|p| {
                    let text: Vec<String> = (0..60)
                        .map(|w| format!("{}{}", words[(i + p * 7 + w) % words.len()], (i * 31 + w) % 997))
                        .collect();
                    format!("<p>{}</p>", text.join(" "))
                })
                .collect();
            TestArticle::new(&format!("Article_{i}"), &format!("Article {i}"), &html("t", &paragraphs))
        })
        .collect();
    write_zim(&fx.archive("bench.zim"), &articles, &[]);
    let archive_mb = std::fs::metadata(fx.archive("bench.zim")).unwrap().len() as f64 / 1e6;
    let started = Instant::now();
    assert_eq!(fx.run("bench.zim"), JobOutcome::Completed { articles: 20_000 });
    let secs = started.elapsed().as_secs_f64();
    let index_mb = index_size(&fx.index("bench.zim")) as f64 / 1e6;
    let reader = fx.reader("bench.zim").unwrap();
    let q = Instant::now();
    let hits = reader.search(&all_terms_expr("river1 mount").unwrap(), 10, &far()).unwrap();
    eprintln!(
        "indexed 20000 articles ({archive_mb:.1} MB html) in {secs:.2}s = {:.0} articles/s; index {index_mb:.1} MB; query {} hits in {:?}",
        20_000.0 / secs,
        hits.len(),
        q.elapsed()
    );
}
