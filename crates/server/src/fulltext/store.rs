//! The per-archive FTS5 index database.
//!
//! Schema (version [`SCHEMA_VERSION`]):
//!
//! ```sql
//! CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
//! CREATE VIRTUAL TABLE articles USING fts5(
//!     path UNINDEXED, title, body,
//!     tokenize = 'unicode61 remove_diacritics 2');
//! ```
//!
//! `meta` holds the archive filename, size and mtime (the index is only
//! valid for that exact file version), the build state, the resume cursor
//! (next URL-pointer index to process) and counters. Rows and the cursor are
//! committed in the same transaction, so a build interrupted at any point
//! resumes without duplicates. The database runs in WAL mode so searches
//! read a consistent snapshot while the indexer writes.

use rusqlite::{params, Connection, ErrorCode, OpenFlags};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;

/// BM25 column weights: path (unindexed), title, body.
const RANK_FUNCTION: &str = "bm25(0.0, 4.0, 1.0)";
/// Snippet length in tokens.
const SNIPPET_TOKENS: i64 = 18;
/// SQLite VM steps between deadline checks while a query runs.
pub(crate) const PROGRESS_STEPS: i32 = 2000;
/// Readers wait at most this long for a lock (WAL readers normally never
/// wait; this only covers a checkpoint in progress).
const READER_BUSY_TIMEOUT: Duration = Duration::from_millis(200);

/// Identity of an archive version: size plus modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveStamp {
    pub len: u64,
    pub mtime_ns: i64,
}

impl ArchiveStamp {
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Some(Self {
            len: meta.len(),
            mtime_ns,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexState {
    Building,
    Ready,
    Failed,
}

impl IndexState {
    pub fn as_str(self) -> &'static str {
        match self {
            IndexState::Building => "building",
            IndexState::Ready => "ready",
            IndexState::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "building" => Some(IndexState::Building),
            "ready" => Some(IndexState::Ready),
            "failed" => Some(IndexState::Failed),
            _ => None,
        }
    }
}

/// Contents of the `meta` table.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexMeta {
    pub schema: u32,
    pub archive: String,
    pub stamp: ArchiveStamp,
    pub state: IndexState,
    /// Next URL-pointer index to process.
    pub cursor: u32,
    /// Directory entries in the archive (the cursor's end).
    pub total: u32,
    /// Articles stored.
    pub articles: u64,
    /// Articles queued in `todo` but not stored yet.
    pub pending: u64,
    /// Short, path-free explanation for a failed build.
    pub note: Option<String>,
    /// Unix seconds of the last write.
    pub updated_at: i64,
}

impl IndexMeta {
    /// Whether this index belongs to `archive` at version `stamp` and uses
    /// the current schema.
    pub fn matches(&self, archive: &str, stamp: &ArchiveStamp) -> bool {
        self.schema == SCHEMA_VERSION && self.archive == archive && self.stamp == *stamp
    }

    fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        let get = |key: &str| map.get(key).map(String::as_str);
        let num = |key: &str| get(key).and_then(|v| v.parse::<i64>().ok());
        Some(Self {
            schema: num("schema_version")? as u32,
            archive: get("archive")?.to_string(),
            stamp: ArchiveStamp {
                len: num("archive_len")? as u64,
                mtime_ns: num("archive_mtime_ns")?,
            },
            state: IndexState::parse(get("state")?)?,
            cursor: num("cursor")? as u32,
            total: num("total_entries")? as u32,
            articles: num("articles")? as u64,
            pending: num("pending").unwrap_or(0) as u64,
            note: get("note").filter(|n| !n.is_empty()).map(str::to_string),
            updated_at: num("updated_at").unwrap_or(0),
        })
    }
}

pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Every `meta` row as a map (shared with the place index).
pub(crate) fn read_meta_map(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM meta")?;
    let map: HashMap<String, String> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(map)
}

fn read_meta(conn: &Connection) -> rusqlite::Result<Option<IndexMeta>> {
    Ok(IndexMeta::from_map(&read_meta_map(conn)?))
}

/// Upsert `meta` rows and stamp `updated_at` (shared with the place index).
pub(crate) fn set_meta(conn: &Connection, values: &[(&str, String)]) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO meta(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?;
    for (key, value) in values {
        stmt.execute(params![key, value])?;
    }
    stmt.execute(params!["updated_at", now_secs().to_string()])?;
    Ok(())
}

/// Open an existing index without creating it, for reading only.
pub(crate) fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    // READ_WRITE (not READ_ONLY) so SQLite can attach the WAL/shm files even
    // after a clean close removed them; `query_only` forbids any writes.
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(READER_BUSY_TIMEOUT)?;
    conn.pragma_update(None, "query_only", true)?;
    Ok(conn)
}

/// Read an index's metadata, or `None` when it is missing or unreadable.
pub fn inspect(path: &Path) -> Option<IndexMeta> {
    if !path.is_file() {
        return None;
    }
    let conn = open_read_only(path).ok()?;
    read_meta(&conn).ok().flatten()
}

/// Whether a SQLite error means the disk (or quota) is full.
pub fn is_disk_full(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.code == ErrorCode::DiskFull)
}

pub(crate) fn is_interrupt(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.code == ErrorCode::OperationInterrupted)
}

/// When a running query should give up.
#[derive(Debug, Clone)]
pub struct Interrupt {
    pub deadline: Instant,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Interrupt {
    pub fn at(deadline: Instant) -> Self {
        Self { deadline, cancel: None }
    }

    pub(crate) fn fired(&self) -> bool {
        Instant::now() >= self.deadline
            || self.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

/// Query failure modes. `Interrupted` means the deadline or a cancel flag
/// stopped the query (the caller reports partial results).
#[derive(Debug)]
pub enum QueryError {
    Interrupted,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::Interrupted => f.write_str("query interrupted"),
            QueryError::Sqlite(error) => write!(f, "{error}"),
        }
    }
}

impl From<rusqlite::Error> for QueryError {
    fn from(error: rusqlite::Error) -> Self {
        if is_interrupt(&error) {
            QueryError::Interrupted
        } else {
            QueryError::Sqlite(error)
        }
    }
}

/// A global-search hit: article path, title and an FTS5 snippet of the body
/// delimited with [`super::HIGHLIGHT_START`] / [`super::HIGHLIGHT_END`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
}

/// A retrieval candidate: the stored plain text of a matching article.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDoc {
    pub path: String,
    pub title: String,
    pub body: String,
}

/// Read-only handle on one index.
pub struct IndexReader {
    conn: Connection,
    pub meta: IndexMeta,
}

impl IndexReader {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = open_read_only(path)?;
        let meta = read_meta(&conn)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(Self { conn, meta })
    }

    fn with_interrupt<T>(
        &self,
        interrupt: &Interrupt,
        run: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, QueryError> {
        if interrupt.fired() {
            return Err(QueryError::Interrupted);
        }
        let check = interrupt.clone();
        self.conn.progress_handler(PROGRESS_STEPS, Some(move || check.fired()))?;
        let result = run(&self.conn);
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        result.map_err(QueryError::from)
    }

    /// Best `limit` matches for the FTS5 expression `expr` (build it with
    /// [`super::all_terms_expr`] / [`super::any_terms_expr`]), with snippets.
    pub fn search(&self, expr: &str, limit: usize, interrupt: &Interrupt) -> Result<Vec<TextHit>, QueryError> {
        let start = super::HIGHLIGHT_START.to_string();
        let end = super::HIGHLIGHT_END.to_string();
        self.with_interrupt(interrupt, |conn| {
            let mut stmt = conn.prepare(
                "SELECT path, title, snippet(articles, 2, ?3, ?4, '…', ?5) \
                 FROM articles WHERE articles MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![expr, limit as i64, start, end, SNIPPET_TOKENS], |row| {
                Ok(TextHit {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                })
            })?;
            rows.collect()
        })
    }

    /// Best `limit` matching articles with their stored text.
    pub fn documents(&self, expr: &str, limit: usize, interrupt: &Interrupt) -> Result<Vec<TextDoc>, QueryError> {
        self.with_interrupt(interrupt, |conn| {
            let mut stmt = conn.prepare(
                "SELECT path, title, body FROM articles WHERE articles MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![expr, limit as i64], |row| {
                Ok(TextDoc {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                })
            })?;
            rows.collect()
        })
    }
}

/// Write handle used by the background indexer.
pub struct IndexWriter {
    conn: Connection,
}

impl IndexWriter {
    fn configure(conn: &Connection) -> rusqlite::Result<()> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Keep the page cache small on low-memory boards (~8 MiB).
        conn.pragma_update(None, "cache_size", -8000)?;
        Ok(())
    }

    /// Create a fresh index at `path` (which must not exist) for `archive`.
    pub fn create(path: &Path, archive: &str, stamp: ArchiveStamp, total: u32) -> rusqlite::Result<(Self, IndexMeta)> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS todo(
                 cluster INTEGER NOT NULL, blob INTEGER NOT NULL, entry INTEGER NOT NULL,
                 PRIMARY KEY (cluster, blob, entry)) WITHOUT ROWID;
             CREATE VIRTUAL TABLE IF NOT EXISTS articles USING fts5(
                 path UNINDEXED, title, body,
                 tokenize = 'unicode61 remove_diacritics 2');",
        )?;
        conn.execute(
            "INSERT INTO articles(articles, rank) VALUES ('rank', ?1)",
            params![RANK_FUNCTION],
        )?;
        let meta = IndexMeta {
            schema: SCHEMA_VERSION,
            archive: archive.to_string(),
            stamp,
            state: IndexState::Building,
            cursor: 0,
            total,
            articles: 0,
            pending: 0,
            note: None,
            updated_at: now_secs(),
        };
        set_meta(
            &conn,
            &[
                ("schema_version", SCHEMA_VERSION.to_string()),
                ("archive", archive.to_string()),
                ("archive_len", stamp.len.to_string()),
                ("archive_mtime_ns", stamp.mtime_ns.to_string()),
                ("state", IndexState::Building.as_str().to_string()),
                ("cursor", "0".to_string()),
                ("total_entries", total.to_string()),
                ("articles", "0".to_string()),
                ("pending", "0".to_string()),
                ("note", String::new()),
            ],
        )?;
        Ok((Self { conn }, meta))
    }

    /// Reopen an existing index to resume or finish it.
    pub fn open(path: &Path) -> rusqlite::Result<(Self, IndexMeta)> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure(&conn)?;
        let meta = read_meta(&conn)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok((Self { conn }, meta))
    }

    /// The next `limit` queued articles as `(cluster, blob, entry)`, in
    /// cluster order.
    pub fn next_pending(&self, limit: usize) -> rusqlite::Result<Vec<(u32, u32, u32)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT cluster, blob, entry FROM todo ORDER BY cluster, blob, entry LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect()
    }

    /// Start a batch; rows and the new cursor are committed together.
    pub fn batch(&mut self) -> rusqlite::Result<Batch<'_>> {
        Ok(Batch {
            tx: self.conn.transaction()?,
        })
    }

    pub fn mark_failed(&self, note: &str) -> rusqlite::Result<()> {
        set_meta(
            &self.conn,
            &[
                ("state", IndexState::Failed.as_str().to_string()),
                ("note", note.to_string()),
            ],
        )
    }

    /// Merge the FTS5 b-trees (faster queries, smaller file) and mark the
    /// index ready. `cancelled` interrupts the merge; the index then stays
    /// `building` with its cursor at the end and is finished on the next run.
    pub fn finish(&self, cancelled: impl Fn() -> bool + Send + 'static) -> rusqlite::Result<()> {
        self.conn.progress_handler(PROGRESS_STEPS * 10, Some(cancelled))?;
        let merged = self
            .conn
            .execute("INSERT INTO articles(articles) VALUES ('optimize')", []);
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        merged?;
        self.conn.execute_batch("DROP TABLE IF EXISTS todo;")?;
        set_meta(
            &self.conn,
            &[
                ("state", IndexState::Ready.as_str().to_string()),
                ("note", String::new()),
            ],
        )?;
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }
}

/// One indexing transaction. Dropping it without [`Batch::commit`] rolls
/// back the rows (the cursor then still points at the batch start).
pub struct Batch<'a> {
    tx: rusqlite::Transaction<'a>,
}

impl Batch<'_> {
    pub fn insert(&self, path: &str, title: &str, body: &str) -> rusqlite::Result<()> {
        self.tx
            .prepare_cached("INSERT INTO articles(path, title, body) VALUES (?1, ?2, ?3)")?
            .execute(params![path, title, body])?;
        Ok(())
    }

    /// Queue an article for the indexing phase.
    pub fn queue(&self, cluster: u32, blob: u32, entry: u32) -> rusqlite::Result<()> {
        self.tx
            .prepare_cached("INSERT OR IGNORE INTO todo(cluster, blob, entry) VALUES (?1, ?2, ?3)")?
            .execute(params![cluster, blob, entry])?;
        Ok(())
    }

    /// Remove every queued article up to and including `last` (key order).
    pub fn dequeue_through(&self, last: (u32, u32, u32)) -> rusqlite::Result<()> {
        self.tx
            .prepare_cached("DELETE FROM todo WHERE (cluster, blob, entry) <= (?1, ?2, ?3)")?
            .execute(params![last.0, last.1, last.2])?;
        Ok(())
    }

    pub fn commit(self, cursor: u32, articles: u64, pending: u64) -> rusqlite::Result<()> {
        set_meta(
            &self.tx,
            &[
                ("cursor", cursor.to_string()),
                ("articles", articles.to_string()),
                ("pending", pending.to_string()),
            ],
        )?;
        self.tx.commit()
    }
}
