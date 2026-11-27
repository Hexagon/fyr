//! The per-map place index database.
//!
//! Schema (version [`SCHEMA_VERSION`]):
//!
//! ```sql
//! CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
//! CREATE TABLE places(
//!     id INTEGER PRIMARY KEY, name TEXT, alt TEXT, category TEXT, kind TEXT,
//!     lon REAL, lat REAL, importance REAL, min_zoom INTEGER, coord_zoom INTEGER,
//!     dkey TEXT, cx INTEGER, cy INTEGER);
//! CREATE INDEX places_dedup ON places(dkey, cy, cx);
//! CREATE VIRTUAL TABLE places_fts USING fts5(name, alt,
//!     content = 'places', content_rowid = 'id',
//!     tokenize = 'unicode61 remove_diacritics 2');
//! ```
//!
//! `alt` holds alternative names separated by newlines. `dkey` is the folded
//! name plus category; `cx`/`cy` a grid cell the size of the category's
//! de-duplication radius, so a near-duplicate lookup reads a handful of index
//! rows. Rows and the resume cursor are committed in one transaction, the
//! database is in WAL mode, and readers see a consistent snapshot while the
//! worker writes (as for the full-text index, see `crate::fulltext::store`).

use super::{Category, Place};
use crate::fulltext::store::{
    is_disk_full, open_read_only, read_meta_map, set_meta, ArchiveStamp, IndexState, Interrupt, QueryError,
    PROGRESS_STEPS,
};
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub const SCHEMA_VERSION: u32 = 1;
const KM_PER_DEGREE: f64 = 111.32;

/// Contents of the `meta` table.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacesMeta {
    pub schema: u32,
    pub map: String,
    pub stamp: ArchiveStamp,
    pub state: IndexState,
    /// Zoom passes, ascending.
    pub zooms: Vec<u8>,
    /// Index into `zooms` of the pass in progress.
    pub pass: u32,
    /// Resume position inside the pass (PMTiles: next tile ID; MBTiles:
    /// `column << 32 | row` of the next tile).
    pub pos: u64,
    /// Tile entries processed / planned over all passes.
    pub processed: u64,
    pub total: u64,
    /// Places stored (after de-duplication).
    pub places: u64,
    /// Tiles skipped because they could not be read or decoded.
    pub bad_tiles: u64,
    pub note: Option<String>,
    pub updated_at: i64,
}

impl PlacesMeta {
    pub fn matches(&self, map: &str, stamp: &ArchiveStamp) -> bool {
        self.schema == SCHEMA_VERSION && self.map == map && self.stamp == *stamp
    }

    fn from_map(map: &HashMap<String, String>) -> Option<Self> {
        let get = |key: &str| map.get(key).map(String::as_str);
        let num = |key: &str| get(key).and_then(|v| v.parse::<i64>().ok());
        let unsigned = |key: &str| get(key).and_then(|v| v.parse::<u64>().ok());
        let state = match get("state")? {
            "building" => IndexState::Building,
            "ready" => IndexState::Ready,
            "failed" => IndexState::Failed,
            _ => return None,
        };
        let zooms = get("zooms")?
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u8>().ok())
            .collect::<Option<Vec<u8>>>()?;
        Some(Self {
            schema: num("schema_version")? as u32,
            map: get("map")?.to_string(),
            stamp: ArchiveStamp {
                len: unsigned("map_len")?,
                mtime_ns: num("map_mtime_ns")?,
            },
            state,
            zooms,
            pass: num("pass")? as u32,
            pos: unsigned("pos")?,
            processed: unsigned("processed")?,
            total: unsigned("total")?,
            places: unsigned("places")?,
            bad_tiles: unsigned("bad_tiles").unwrap_or(0),
            note: get("note").filter(|n| !n.is_empty()).map(str::to_string),
            updated_at: num("updated_at").unwrap_or(0),
        })
    }
}

fn read_meta(conn: &Connection) -> rusqlite::Result<Option<PlacesMeta>> {
    Ok(PlacesMeta::from_map(&read_meta_map(conn)?))
}

/// Metadata of an index, or `None` when it is missing or unreadable.
pub fn inspect(path: &Path) -> Option<PlacesMeta> {
    if !path.is_file() {
        return None;
    }
    let conn = open_read_only(path).ok()?;
    read_meta(&conn).ok().flatten()
}

/// One stored place, as returned by queries.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceRow {
    pub id: i64,
    pub name: String,
    pub alt: Vec<String>,
    pub category: Category,
    pub kind: String,
    pub lon: f64,
    pub lat: f64,
    pub importance: f64,
}

fn split_alt(alt: &str) -> Vec<String> {
    alt.split('\n').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// Read-only handle on one place index.
pub struct PlaceReader {
    conn: Connection,
    pub meta: PlacesMeta,
}

impl PlaceReader {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = open_read_only(path)?;
        let meta = read_meta(&conn)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok(Self { conn, meta })
    }

    /// Up to `limit` places matching the FTS5 expression `expr` (build it
    /// with `crate::fulltext::all_terms_expr` / `exact_terms_expr`, which
    /// quote every term), most important first.
    pub fn search(&self, expr: &str, limit: usize, interrupt: &Interrupt) -> Result<Vec<PlaceRow>, QueryError> {
        if interrupt.fired() {
            return Err(QueryError::Interrupted);
        }
        let check = interrupt.clone();
        self.conn.progress_handler(PROGRESS_STEPS, Some(move || check.fired()))?;
        let result = (|| -> rusqlite::Result<Vec<PlaceRow>> {
            let mut stmt = self.conn.prepare_cached(
                "SELECT p.id, p.name, p.alt, p.category, p.kind, p.lon, p.lat, p.importance \
                 FROM places_fts JOIN places p ON p.id = places_fts.rowid \
                 WHERE places_fts MATCH ?1 ORDER BY p.importance DESC, p.id LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![expr, limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, f64>(7)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, name, alt, category, kind, lon, lat, importance) = row?;
                let Some(category) = Category::parse(&category) else { continue };
                if !(lon.is_finite() && lat.is_finite()) {
                    continue;
                }
                out.push(PlaceRow {
                    id,
                    name,
                    alt: split_alt(&alt),
                    category,
                    kind,
                    lon,
                    lat,
                    importance,
                });
            }
            Ok(out)
        })();
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        result.map_err(QueryError::from)
    }
}

/// Write handle used by the background indexer.
pub struct PlaceWriter {
    conn: Connection,
}

impl PlaceWriter {
    fn configure(conn: &Connection) -> rusqlite::Result<()> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -8000)?;
        Ok(())
    }

    /// Create a fresh index at `path` (which must not exist).
    pub fn create(
        path: &Path,
        map: &str,
        stamp: ArchiveStamp,
        zooms: &[u8],
        total: u64,
    ) -> rusqlite::Result<(Self, PlacesMeta)> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS places(
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL, alt TEXT NOT NULL DEFAULT '',
                 category TEXT NOT NULL, kind TEXT NOT NULL,
                 lon REAL NOT NULL, lat REAL NOT NULL, importance REAL NOT NULL,
                 min_zoom INTEGER NOT NULL, coord_zoom INTEGER NOT NULL,
                 dkey TEXT NOT NULL, cx INTEGER NOT NULL, cy INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS places_dedup ON places(dkey, cy, cx);
             CREATE VIRTUAL TABLE IF NOT EXISTS places_fts USING fts5(
                 name, alt, content = 'places', content_rowid = 'id',
                 tokenize = 'unicode61 remove_diacritics 2');",
        )?;
        let zoom_list = zooms.iter().map(|z| z.to_string()).collect::<Vec<_>>().join(",");
        set_meta(
            &conn,
            &[
                ("schema_version", SCHEMA_VERSION.to_string()),
                ("map", map.to_string()),
                ("map_len", stamp.len.to_string()),
                ("map_mtime_ns", stamp.mtime_ns.to_string()),
                ("state", IndexState::Building.as_str().to_string()),
                ("zooms", zoom_list),
                ("pass", "0".to_string()),
                ("pos", "0".to_string()),
                ("processed", "0".to_string()),
                ("total", total.to_string()),
                ("places", "0".to_string()),
                ("bad_tiles", "0".to_string()),
                ("note", String::new()),
            ],
        )?;
        let meta = read_meta(&conn)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok((Self { conn }, meta))
    }

    /// Reopen an existing index to resume or finish it.
    pub fn open(path: &Path) -> rusqlite::Result<(Self, PlacesMeta)> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        Self::configure(&conn)?;
        let meta = read_meta(&conn)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        Ok((Self { conn }, meta))
    }

    pub fn batch(&mut self) -> rusqlite::Result<PlaceBatch<'_>> {
        Ok(PlaceBatch {
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

    /// Merge the FTS5 b-trees and mark the index ready. `cancelled`
    /// interrupts the merge; the index then stays `building` at the end of
    /// its last pass and is finished on the next run.
    pub fn finish(&self, cancelled: impl Fn() -> bool + Send + 'static) -> rusqlite::Result<()> {
        self.conn.progress_handler(PROGRESS_STEPS * 10, Some(cancelled))?;
        let merged = self.conn.execute("INSERT INTO places_fts(places_fts) VALUES ('optimize')", []);
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        merged?;
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

/// Outcome of [`PlaceBatch::add`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    Inserted,
    /// Merged into an existing nearby place with the same name and category.
    Merged,
}

/// Grid cell of `(lon, lat)` for a de-duplication radius.
fn cell(lon: f64, lat: f64, radius_km: f64) -> (i64, i64) {
    let size = radius_km / KM_PER_DEGREE;
    ((lon / size).floor() as i64, (lat / size).floor() as i64)
}

/// One indexing transaction. Dropping it without [`PlaceBatch::commit`]
/// rolls back (the cursor then still points at the batch start).
pub struct PlaceBatch<'a> {
    tx: rusqlite::Transaction<'a>,
}

impl PlaceBatch<'_> {
    /// Store `place`, or merge it into a stored place with the same folded
    /// name and category within the category's radius: alternative names are
    /// united, the lowest zoom and highest importance kept, and the position
    /// taken from the most detailed zoom.
    pub fn add(&self, place: &Place) -> rusqlite::Result<Added> {
        let radius = place.category.dedup_radius_km();
        let dkey = format!("{}\u{1f}{}", crate::search::fold(&place.name), place.category.as_str());
        let (cx, cy) = cell(place.lon, place.lat, radius);
        // Longitude cells shrink towards the poles: widen the window so it
        // still spans the radius.
        let size = radius / KM_PER_DEGREE;
        let max_lat = (place.lat.abs() + size).min(89.0);
        let span = ((1.0 / max_lat.to_radians().cos()).ceil() as i64).clamp(1, 64);
        let existing = {
            let mut stmt = self.tx.prepare_cached(
                "SELECT id, name, alt, lon, lat, importance, min_zoom, coord_zoom FROM places \
                 WHERE dkey = ?1 AND cy BETWEEN ?2 AND ?3 AND cx BETWEEN ?4 AND ?5",
            )?;
            let rows = stmt.query_map(params![dkey, cy - 1, cy + 1, cx - span, cx + span], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })?;
            let mut found = None;
            for row in rows {
                let row = row?;
                if super::haversine_km(place.lon, place.lat, row.3, row.4) <= radius {
                    found = Some(row);
                    break;
                }
            }
            found
        };

        let Some((id, name, alt, lon, lat, importance, min_zoom, coord_zoom)) = existing else {
            let alt = place.alt.join("\n");
            self.tx
                .prepare_cached(
                    "INSERT INTO places(name, alt, category, kind, lon, lat, importance, min_zoom, coord_zoom, dkey, cx, cy) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11)",
                )?
                .execute(params![
                    place.name,
                    alt,
                    place.category.as_str(),
                    place.kind,
                    place.lon,
                    place.lat,
                    place.importance,
                    i64::from(place.zoom),
                    dkey,
                    cx,
                    cy
                ])?;
            let id = self.tx.last_insert_rowid();
            self.tx
                .prepare_cached("INSERT INTO places_fts(rowid, name, alt) VALUES (?1, ?2, ?3)")?
                .execute(params![id, place.name, alt])?;
            return Ok(Added::Inserted);
        };

        let mut names = split_alt(&alt);
        let mut seen: Vec<String> = std::iter::once(&name).chain(names.iter()).map(|n| crate::search::fold(n)).collect();
        let mut alt_changed = false;
        for extra in &place.alt {
            if names.len() >= super::MAX_ALT_NAMES {
                break;
            }
            let folded = crate::search::fold(extra);
            if !seen.contains(&folded) {
                seen.push(folded);
                names.push(extra.clone());
                alt_changed = true;
            }
        }
        let zoom = i64::from(place.zoom);
        let (new_lon, new_lat, new_coord_zoom) = if zoom > coord_zoom {
            (place.lon, place.lat, zoom)
        } else {
            (lon, lat, coord_zoom)
        };
        let (ncx, ncy) = cell(new_lon, new_lat, radius);
        self.tx
            .prepare_cached(
                "UPDATE places SET alt = ?2, lon = ?3, lat = ?4, importance = ?5, min_zoom = ?6, coord_zoom = ?7, \
                 cx = ?8, cy = ?9 WHERE id = ?1",
            )?
            .execute(params![
                id,
                names.join("\n"),
                new_lon,
                new_lat,
                importance.max(place.importance),
                min_zoom.min(zoom),
                new_coord_zoom,
                ncx,
                ncy
            ])?;
        if alt_changed {
            // External-content FTS: remove the old row values, then add the new.
            self.tx
                .prepare_cached("INSERT INTO places_fts(places_fts, rowid, name, alt) VALUES ('delete', ?1, ?2, ?3)")?
                .execute(params![id, name, alt])?;
            self.tx
                .prepare_cached("INSERT INTO places_fts(rowid, name, alt) VALUES (?1, ?2, ?3)")?
                .execute(params![id, name, names.join("\n")])?;
        }
        Ok(Added::Merged)
    }

    /// Commit the rows together with the new resume position and counters.
    pub fn commit(self, pass: u32, pos: u64, processed: u64, places: u64, bad_tiles: u64) -> rusqlite::Result<()> {
        set_meta(
            &self.tx,
            &[
                ("pass", pass.to_string()),
                ("pos", pos.to_string()),
                ("processed", processed.to_string()),
                ("places", places.to_string()),
                ("bad_tiles", bad_tiles.to_string()),
            ],
        )?;
        self.tx.commit()
    }
}

/// Number of stored places (tests).
#[cfg(test)]
pub fn count_places(path: &Path) -> Option<u64> {
    let conn = open_read_only(path).ok()?;
    conn.query_row("SELECT COUNT(*) FROM places", [], |row| row.get::<_, i64>(0))
        .ok()
        .map(|n| n.max(0) as u64)
}

pub fn disk_full(error: &rusqlite::Error) -> bool {
    is_disk_full(error)
}
