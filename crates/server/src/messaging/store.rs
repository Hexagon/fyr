//! SQLite storage for messages.
//!
//! One database (`DATA_DIR/messages.sqlite`) holds both the built-in public
//! channel ("board", id [`PUBLIC_CHANNEL_ID`]) and every end-to-end encrypted
//! channel. Public messages are stored as plaintext fields; encrypted
//! messages are stored only as `nonce` + `ciphertext` and the server never
//! sees their contents or keys.
//!
//! All functions here are blocking and must run on the blocking pool.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::path::Path;

/// Id of the built-in public (unencrypted) channel.
pub const PUBLIC_CHANNEL_ID: &str = "public";

/// Ordered schema migrations. Index + 1 is the schema version.
const MIGRATIONS: &[&str] = &[
    // v1: channels + messages + meta (server salt)
    "CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value BLOB NOT NULL
     );
     CREATE TABLE channels (
        id            TEXT PRIMARY KEY,
        kind          TEXT NOT NULL CHECK (kind IN ('public', 'encrypted')),
        verifier      BLOB,
        created_at    INTEGER NOT NULL,
        last_activity INTEGER NOT NULL
     );
     CREATE TABLE messages (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER,
        pinned     INTEGER NOT NULL DEFAULT 0,
        nick       TEXT,
        device_tag TEXT,
        category   TEXT,
        body       TEXT,
        lat        REAL,
        lng        REAL,
        nonce      BLOB,
        ciphertext BLOB
     );
     CREATE INDEX messages_by_channel ON messages(channel_id, id);
     CREATE INDEX messages_by_expiry ON messages(expires_at);
     INSERT INTO channels (id, kind, verifier, created_at, last_activity)
        VALUES ('public', 'public', NULL, 0, 0);",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Public,
    Encrypted,
}

#[derive(Debug, Clone)]
pub struct ChannelRecord {
    pub kind: ChannelKind,
    /// SHA-256 of the channel auth key (encrypted channels only).
    pub verifier: Option<Vec<u8>>,
}

/// One stored message as returned to clients. Public messages carry the
/// plaintext fields, encrypted messages only `nonce` and `ciphertext`
/// (both base64-encoded by the API layer).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MessageRow {
    pub id: i64,
    pub channel_id: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lng: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ciphertext: Option<String>,
}

/// A validated plaintext message for the public channel.
#[derive(Debug, Clone)]
pub struct NewPublicMessage {
    pub nick: String,
    pub device_tag: String,
    pub category: Option<String>,
    pub text: String,
    pub location: Option<(f64, f64)>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ListQuery<'a> {
    /// Only messages with id > since (ascending).
    pub since: Option<i64>,
    /// Only messages with id < before (newest page below a cursor).
    pub before: Option<i64>,
    pub limit: u32,
    pub category: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetentionLimits {
    pub max_per_channel: u32,
    pub max_total_messages: u32,
    /// Encrypted channels with no messages and no activity for this long
    /// are removed.
    pub idle_channel_ms: i64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PruneStats {
    pub expired: usize,
    pub over_channel_cap: usize,
    pub over_total_cap: usize,
    pub idle_channels: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CreateChannel {
    Created,
    AlreadyExists,
    TooManyChannels,
}

pub struct Store {
    conn: Connection,
}

const SELECT_COLUMNS: &str = "id, channel_id, created_at, expires_at, pinned, nick, device_tag, \
                              category, body, lat, lng, nonce, ciphertext";

/// Live-message predicate: pinned messages never expire.
const LIVE: &str = "(pinned = 1 OR expires_at IS NULL OR expires_at > ?now)";

impl Store {
    /// Open (or create) the database file and run pending migrations.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// In-memory database for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // WAL is not available for in-memory databases; ignore the result.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;
        let current: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))?;
        for (index, sql) in MIGRATIONS.iter().enumerate() {
            let version = index as i64 + 1;
            if version <= current {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("DELETE FROM schema_version", [])?;
            tx.execute("INSERT INTO schema_version (version) VALUES (?1)", [version])?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
    }

    /// Return the persisted server salt, generating it with `generate` on
    /// first use.
    pub fn server_salt(&self, generate: impl FnOnce() -> [u8; 32]) -> rusqlite::Result<[u8; 32]> {
        let existing: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'server_salt'", [], |r| r.get(0))
            .optional()?;
        if let Some(bytes) = existing {
            if let Ok(salt) = <[u8; 32]>::try_from(bytes.as_slice()) {
                return Ok(salt);
            }
        }
        let salt = generate();
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('server_salt', ?1)",
            [salt.to_vec()],
        )?;
        Ok(salt)
    }

    pub fn channel(&self, id: &str) -> rusqlite::Result<Option<ChannelRecord>> {
        self.conn
            .query_row(
                "SELECT kind, verifier FROM channels WHERE id = ?1",
                [id],
                |row| {
                    let kind: String = row.get(0)?;
                    Ok(ChannelRecord {
                        kind: if kind == "public" {
                            ChannelKind::Public
                        } else {
                            ChannelKind::Encrypted
                        },
                        verifier: row.get(1)?,
                    })
                },
            )
            .optional()
    }

    pub fn create_channel(
        &self,
        id: &str,
        verifier: &[u8],
        now: i64,
        max_channels: u32,
    ) -> rusqlite::Result<CreateChannel> {
        if self.channel(id)?.is_some() {
            return Ok(CreateChannel::AlreadyExists);
        }
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM channels WHERE kind = 'encrypted'", [], |r| r.get(0))?;
        if count >= i64::from(max_channels) {
            return Ok(CreateChannel::TooManyChannels);
        }
        self.conn.execute(
            "INSERT INTO channels (id, kind, verifier, created_at, last_activity)
             VALUES (?1, 'encrypted', ?2, ?3, ?3)",
            params![id, verifier, now],
        )?;
        Ok(CreateChannel::Created)
    }

    pub fn delete_channel(&self, id: &str) -> rusqlite::Result<bool> {
        if id == PUBLIC_CHANNEL_ID {
            return Ok(false);
        }
        Ok(self.conn.execute("DELETE FROM channels WHERE id = ?1", [id])? > 0)
    }

    pub fn insert_public(
        &self,
        message: &NewPublicMessage,
        now: i64,
        max_per_channel: u32,
    ) -> rusqlite::Result<MessageRow> {
        let (lat, lng) = message.location.unzip();
        self.conn.execute(
            "INSERT INTO messages (channel_id, created_at, expires_at, pinned, nick, device_tag,
                                   category, body, lat, lng)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                PUBLIC_CHANNEL_ID,
                now,
                message.expires_at,
                message.nick,
                message.device_tag,
                message.category,
                message.text,
                lat,
                lng
            ],
        )?;
        self.after_insert(PUBLIC_CHANNEL_ID, now, max_per_channel)
    }

    pub fn insert_encrypted(
        &self,
        channel_id: &str,
        nonce: &[u8],
        ciphertext: &[u8],
        now: i64,
        expires_at: i64,
        max_per_channel: u32,
    ) -> rusqlite::Result<MessageRow> {
        self.conn.execute(
            "INSERT INTO messages (channel_id, created_at, expires_at, pinned, nonce, ciphertext)
             VALUES (?1, ?2, ?3, 0, ?4, ?5)",
            params![channel_id, now, expires_at, nonce, ciphertext],
        )?;
        self.after_insert(channel_id, now, max_per_channel)
    }

    fn after_insert(&self, channel_id: &str, now: i64, max_per_channel: u32) -> rusqlite::Result<MessageRow> {
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "UPDATE channels SET last_activity = ?2 WHERE id = ?1",
            params![channel_id, now],
        )?;
        self.enforce_channel_cap(channel_id, max_per_channel)?;
        self.message(channel_id, id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Keep only the newest `max` unpinned messages of a channel.
    fn enforce_channel_cap(&self, channel_id: &str, max: u32) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM messages WHERE channel_id = ?1 AND pinned = 0 AND id <= (
                SELECT id FROM messages WHERE channel_id = ?1 AND pinned = 0
                ORDER BY id DESC LIMIT 1 OFFSET ?2)",
            params![channel_id, max],
        )
    }

    pub fn message(&self, channel_id: &str, id: i64) -> rusqlite::Result<Option<MessageRow>> {
        self.conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM messages WHERE channel_id = ?1 AND id = ?2"),
                params![channel_id, id],
                map_row,
            )
            .optional()
    }

    /// List live messages in ascending id order.
    ///
    /// With `since`, returns the oldest `limit` messages after that id
    /// (catch-up). Otherwise returns the newest `limit` messages, optionally
    /// below `before` (history paging). The bool is `true` when more
    /// messages exist beyond the returned window.
    pub fn list(
        &self,
        channel_id: &str,
        query: ListQuery<'_>,
        now: i64,
    ) -> rusqlite::Result<(Vec<MessageRow>, bool)> {
        use rusqlite::types::Value;
        let limit = i64::from(query.limit.max(1));
        let mut values: Vec<Value> = vec![channel_id.to_string().into(), now.into()];
        let mut sql = format!(
            "SELECT {SELECT_COLUMNS} FROM messages WHERE channel_id = ?1 AND {}",
            LIVE.replace("?now", "?2")
        );
        if let Some(category) = query.category {
            values.push(category.to_string().into());
            sql.push_str(&format!(" AND category = ?{}", values.len()));
        }
        let ascending = query.since.is_some();
        if let Some(since) = query.since {
            values.push(since.into());
            sql.push_str(&format!(" AND id > ?{}", values.len()));
        } else if let Some(before) = query.before {
            values.push(before.into());
            sql.push_str(&format!(" AND id < ?{}", values.len()));
        }
        values.push((limit + 1).into());
        sql.push_str(&format!(
            " ORDER BY id {} LIMIT ?{}",
            if ascending { "ASC" } else { "DESC" },
            values.len()
        ));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt
            .query_map(rusqlite::params_from_iter(values), map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let has_more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        if !ascending {
            rows.reverse();
        }
        Ok((rows, has_more))
    }

    /// Pinned messages of a channel (ascending).
    pub fn pinned(&self, channel_id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM messages WHERE channel_id = ?1 AND pinned = 1 ORDER BY id ASC LIMIT 50"
        ))?;
        let rows = stmt.query_map([channel_id], map_row)?;
        rows.collect()
    }

    pub fn delete_message(&self, channel_id: &str, id: i64) -> rusqlite::Result<bool> {
        Ok(self.conn.execute(
            "DELETE FROM messages WHERE channel_id = ?1 AND id = ?2",
            params![channel_id, id],
        )? > 0)
    }

    /// Pin or unpin a message. Unpinning an already-expired message gives
    /// it one more day so it does not vanish instantly.
    pub fn set_pinned(
        &self,
        channel_id: &str,
        id: i64,
        pinned: bool,
        now: i64,
    ) -> rusqlite::Result<Option<MessageRow>> {
        let changed = self.conn.execute(
            "UPDATE messages SET pinned = ?3,
                 expires_at = CASE WHEN ?3 = 0 AND expires_at IS NOT NULL AND expires_at < ?4
                                   THEN ?4 + 86400000 ELSE expires_at END
             WHERE channel_id = ?1 AND id = ?2",
            params![channel_id, id, pinned, now],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.message(channel_id, id)
    }

    /// Apply retention: expiry, per-channel cap, global cap and idle
    /// encrypted channels. Only counts are reported; clients drop expired
    /// messages themselves and re-sync on reconnect.
    pub fn prune(&self, now: i64, limits: RetentionLimits) -> rusqlite::Result<PruneStats> {
        let mut stats = PruneStats::default();
        stats.expired = self.conn.execute(
            "DELETE FROM messages WHERE pinned = 0 AND expires_at IS NOT NULL AND expires_at <= ?1",
            [now],
        )?;

        let over: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT channel_id FROM messages WHERE pinned = 0
                 GROUP BY channel_id HAVING COUNT(*) > ?1",
            )?;
            let rows = stmt.query_map([limits.max_per_channel], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for channel_id in over {
            stats.over_channel_cap += self.enforce_channel_cap(&channel_id, limits.max_per_channel)?;
        }

        stats.over_total_cap = self.conn.execute(
            "DELETE FROM messages WHERE pinned = 0 AND id <= (
                SELECT id FROM messages WHERE pinned = 0 ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            [limits.max_total_messages],
        )?;

        stats.idle_channels = self.conn.execute(
            "DELETE FROM channels WHERE kind = 'encrypted' AND last_activity < ?1
               AND NOT EXISTS (SELECT 1 FROM messages WHERE messages.channel_id = channels.id)",
            [now - limits.idle_channel_ms],
        )?;
        Ok(stats)
    }

    #[cfg(test)]
    pub fn count_messages(&self, channel_id: &str) -> i64 {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE channel_id = ?1",
                [channel_id],
                |r| r.get(0),
            )
            .unwrap()
    }
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    use base64::Engine;
    let b64 = |bytes: Option<Vec<u8>>| bytes.map(|b| base64::engine::general_purpose::STANDARD.encode(b));
    Ok(MessageRow {
        id: row.get(0)?,
        channel_id: row.get(1)?,
        created_at: row.get(2)?,
        expires_at: row.get(3)?,
        pinned: row.get::<_, i64>(4)? != 0,
        nick: row.get(5)?,
        device_tag: row.get(6)?,
        category: row.get(7)?,
        text: row.get(8)?,
        lat: row.get(9)?,
        lng: row.get(10)?,
        nonce: b64(row.get(11)?),
        ciphertext: b64(row.get(12)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn public(text: &str, expires_at: i64) -> NewPublicMessage {
        NewPublicMessage {
            nick: "Ada".into(),
            device_tag: "abcdef".into(),
            category: None,
            text: text.into(),
            location: None,
            expires_at,
        }
    }

    fn limits() -> RetentionLimits {
        RetentionLimits { max_per_channel: 1000, max_total_messages: 100_000, idle_channel_ms: 90 * DAY }
    }

    #[test]
    fn migrations_are_idempotent_and_seed_public_channel() {
        let dir = std::env::temp_dir().join(format!("fyr-msg-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("messages.sqlite");
        let salt = {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
            assert_eq!(store.channel(PUBLIC_CHANNEL_ID).unwrap().unwrap().kind, ChannelKind::Public);
            store.server_salt(|| [9u8; 32]).unwrap()
        };
        // Reopen: no re-migration, salt is persisted (generator not called).
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(store.server_salt(|| panic!("salt regenerated")).unwrap(), salt);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn insert_list_since_before_and_category() {
        let store = Store::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for i in 0..5 {
            let mut msg = public(&format!("m{i}"), 10 * DAY);
            if i % 2 == 0 {
                msg.category = Some("alert".into());
            }
            ids.push(store.insert_public(&msg, i, 1000).unwrap().id);
        }
        let all = |q| store.list(PUBLIC_CHANNEL_ID, q, 100).unwrap();
        let (latest, more) = all(ListQuery { limit: 3, ..Default::default() });
        assert_eq!(latest.iter().map(|m| m.id).collect::<Vec<_>>(), ids[2..].to_vec());
        assert!(more);
        let (after, more) = all(ListQuery { since: Some(ids[1]), limit: 2, ..Default::default() });
        assert_eq!(after.iter().map(|m| m.id).collect::<Vec<_>>(), ids[2..4].to_vec());
        assert!(more);
        let (before, _) = all(ListQuery { before: Some(ids[2]), limit: 10, ..Default::default() });
        assert_eq!(before.iter().map(|m| m.id).collect::<Vec<_>>(), ids[..2].to_vec());
        let (alerts, _) = all(ListQuery { category: Some("alert"), limit: 10, ..Default::default() });
        assert_eq!(alerts.len(), 3);
        assert_eq!(latest[0].text.as_deref(), Some("m2"));
        assert!(latest[0].ciphertext.is_none());
    }

    #[test]
    fn expired_messages_are_hidden_and_pruned_but_pinned_survive() {
        let store = Store::open_in_memory().unwrap();
        let old = store.insert_public(&public("old", 5), 0, 1000).unwrap();
        let pinned = store.insert_public(&public("pinned", 5), 0, 1000).unwrap();
        let fresh = store.insert_public(&public("fresh", 100 * DAY), 0, 1000).unwrap();
        store.set_pinned(PUBLIC_CHANNEL_ID, pinned.id, true, 1).unwrap().unwrap();

        let (visible, _) = store.list(PUBLIC_CHANNEL_ID, ListQuery { limit: 10, ..Default::default() }, 10).unwrap();
        assert_eq!(visible.iter().map(|m| m.id).collect::<Vec<_>>(), vec![pinned.id, fresh.id]);

        let stats = store.prune(10, limits()).unwrap();
        assert_eq!(stats.expired, 1);
        assert!(store.message(PUBLIC_CHANNEL_ID, old.id).unwrap().is_none());
        assert_eq!(store.pinned(PUBLIC_CHANNEL_ID).unwrap().len(), 1);

        // Unpinning an expired message gives it a grace day.
        let unpinned = store.set_pinned(PUBLIC_CHANNEL_ID, pinned.id, false, 10).unwrap().unwrap();
        assert_eq!(unpinned.expires_at, Some(10 + DAY));
    }

    #[test]
    fn channel_cap_keeps_newest_and_pinned() {
        let store = Store::open_in_memory().unwrap();
        let first = store.insert_public(&public("first", DAY), 0, 3).unwrap();
        store.set_pinned(PUBLIC_CHANNEL_ID, first.id, true, 0).unwrap();
        let mut last = Vec::new();
        for i in 0..6 {
            last.push(store.insert_public(&public(&format!("{i}"), DAY), 0, 3).unwrap().id);
        }
        assert_eq!(store.count_messages(PUBLIC_CHANNEL_ID), 4, "3 newest + pinned");
        assert!(store.message(PUBLIC_CHANNEL_ID, first.id).unwrap().is_some());
        assert!(store.message(PUBLIC_CHANNEL_ID, last[2]).unwrap().is_none());
        assert!(store.message(PUBLIC_CHANNEL_ID, last[3]).unwrap().is_some());
    }

    #[test]
    fn prune_enforces_total_cap_and_idle_channels() {
        let store = Store::open_in_memory().unwrap();
        let busy = "a".repeat(32);
        let idle = "b".repeat(32);
        assert_eq!(store.create_channel(&busy, &[1; 32], 0, 10).unwrap(), CreateChannel::Created);
        assert_eq!(store.create_channel(&busy, &[2; 32], 0, 10).unwrap(), CreateChannel::AlreadyExists);
        assert_eq!(store.channel(&busy).unwrap().unwrap().verifier, Some(vec![1; 32]), "never overwritten");
        assert_eq!(store.create_channel(&idle, &[1; 32], 0, 10).unwrap(), CreateChannel::Created);
        assert_eq!(store.create_channel(&"c".repeat(32), &[1; 32], 0, 2).unwrap(), CreateChannel::TooManyChannels);
        for _ in 0..5 {
            store.insert_encrypted(&busy, &[0; 24], &[0; 32], 100 * DAY, 200 * DAY, 1000).unwrap();
        }
        let stats = store
            .prune(100 * DAY, RetentionLimits { max_per_channel: 1000, max_total_messages: 2, idle_channel_ms: 90 * DAY })
            .unwrap();
        assert_eq!(stats.over_total_cap, 3);
        assert_eq!(stats.idle_channels, 1);
        assert!(store.channel(&idle).unwrap().is_none());
        assert!(store.channel(&busy).unwrap().is_some());
        assert!(store.channel(PUBLIC_CHANNEL_ID).unwrap().is_some(), "public channel is never pruned");
        assert!(!store.delete_channel(PUBLIC_CHANNEL_ID).unwrap());
        assert!(store.delete_channel(&busy).unwrap());
        assert_eq!(store.count_messages(&busy), 0, "cascade");
    }

    #[test]
    fn encrypted_rows_expose_only_ciphertext() {
        let store = Store::open_in_memory().unwrap();
        let id = "d".repeat(32);
        store.create_channel(&id, &[1; 32], 0, 10).unwrap();
        let row = store.insert_encrypted(&id, &[1; 24], &[2; 20], 0, DAY, 1000).unwrap();
        let json = serde_json::to_value(&row).unwrap();
        assert!(json.get("text").is_none() && json.get("nick").is_none());
        assert_eq!(json["nonce"].as_str().unwrap().len(), 32);
        assert!(json["ciphertext"].is_string());
    }
}
