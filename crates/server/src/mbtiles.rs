//! Read-only connection pool for MBTiles (SQLite) archives.
//!
//! A single map viewport issues dozens of parallel tile requests. Opening the
//! SQLite file and re-reading its metadata for every tile is wasteful, so idle
//! read-only connections and the archive's tile format are kept per archive
//! and reused. Entries are keyed by a file fingerprint (size + mtime) so a
//! replaced or re-downloaded archive is never served from stale handles.

use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Maximum idle connections retained per archive.
const MAX_IDLE_PER_ARCHIVE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

struct ArchiveEntry {
    fingerprint: Fingerprint,
    format: String,
    idle: Vec<Connection>,
}

/// A checked-out connection plus what is needed to return it to the pool.
pub struct PooledConnection {
    pub conn: Connection,
    pub format: String,
    path: PathBuf,
    fingerprint: Fingerprint,
}

#[derive(Default)]
pub struct MbtilesPool {
    archives: Mutex<HashMap<PathBuf, ArchiveEntry>>,
}

impl MbtilesPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take an idle connection for `path`, or open a new one.
    ///
    /// Blocking: call from `spawn_blocking`.
    pub fn checkout(&self, path: &Path) -> rusqlite::Result<PooledConnection> {
        let fingerprint = fingerprint(path)?;

        {
            let mut archives = self.lock();
            if let Some(entry) = archives.get_mut(path) {
                if entry.fingerprint == fingerprint {
                    if let Some(conn) = entry.idle.pop() {
                        return Ok(PooledConnection {
                            conn,
                            format: entry.format.clone(),
                            path: path.to_path_buf(),
                            fingerprint,
                        });
                    }
                    let format = entry.format.clone();
                    drop(archives);
                    return Ok(PooledConnection {
                        conn: open_read_only(path)?,
                        format,
                        path: path.to_path_buf(),
                        fingerprint,
                    });
                }
                // Archive changed on disk: drop the stale handles.
                archives.remove(path);
            }
        }

        let conn = open_read_only(path)?;
        let format = conn
            .query_row(
                "SELECT value FROM metadata WHERE name = 'format' LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "pbf".to_string());

        self.lock()
            .entry(path.to_path_buf())
            .or_insert_with(|| ArchiveEntry {
                fingerprint,
                format: format.clone(),
                idle: Vec::new(),
            });

        Ok(PooledConnection {
            conn,
            format,
            path: path.to_path_buf(),
            fingerprint,
        })
    }

    /// Return a connection for reuse. Dropped if the archive changed meanwhile
    /// or the pool for this archive is full.
    pub fn checkin(&self, pooled: PooledConnection) {
        let mut archives = self.lock();
        if let Some(entry) = archives.get_mut(&pooled.path) {
            if entry.fingerprint == pooled.fingerprint && entry.idle.len() < MAX_IDLE_PER_ARCHIVE {
                entry.idle.push(pooled.conn);
            }
        }
    }

    #[cfg(test)]
    fn idle_count(&self, path: &Path) -> usize {
        self.lock().get(path).map(|entry| entry.idle.len()).unwrap_or(0)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, ArchiveEntry>> {
        self.archives
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn fingerprint(path: &Path) -> rusqlite::Result<Fingerprint> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(error.to_string()),
        )
    })?;
    Ok(Fingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_archive(path: &Path, format: &str) {
        let conn = Connection::open(path).expect("create archive");
        conn.execute_batch(
            "CREATE TABLE metadata (name TEXT, value TEXT);
             CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER, tile_data BLOB);",
        )
        .expect("create schema");
        conn.execute("INSERT INTO metadata VALUES ('format', ?1)", [format])
            .expect("insert format");
    }

    #[test]
    fn reuses_connections_and_caches_format() {
        let dir = std::env::temp_dir().join(format!("fyr-mbtiles-pool-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("atlas.mbtiles");
        create_archive(&path, "png");

        let pool = MbtilesPool::new();
        let first = pool.checkout(&path).expect("checkout");
        assert_eq!(first.format, "png");
        pool.checkin(first);
        assert_eq!(pool.idle_count(&path), 1);

        let second = pool.checkout(&path).expect("checkout again");
        assert_eq!(second.format, "png");
        assert_eq!(pool.idle_count(&path), 0, "idle connection should be reused");
        pool.checkin(second);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn replaced_archive_invalidates_cached_entry() {
        let dir = std::env::temp_dir().join(format!("fyr-mbtiles-pool-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("atlas.mbtiles");
        create_archive(&path, "png");

        let pool = MbtilesPool::new();
        let pooled = pool.checkout(&path).expect("checkout");
        pool.checkin(pooled);

        std::fs::remove_file(&path).expect("remove archive");
        create_archive(&path, "pbf");
        // Make sure the fingerprint differs even on coarse-mtime filesystems.
        let conn = Connection::open(&path).expect("reopen");
        conn.execute("INSERT INTO tiles VALUES (0, 0, 0, zeroblob(65536))", [])
            .expect("grow archive");
        drop(conn);

        let pooled = pool.checkout(&path).expect("checkout replaced");
        assert_eq!(pooled.format, "pbf");

        let _ = std::fs::remove_dir_all(dir);
    }
}
