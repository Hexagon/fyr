//! Server backup and restore (admin only).
//!
//! `GET /api/backup` streams a `.zip` with the server's own state:
//!
//! * `settings.json` (serialized from memory, so never half-written),
//! * `curated-content.json`,
//! * `download_tasks.json` (serialized from memory),
//! * `poi/*.geojson|json` and small `poi/*.fgb` files,
//! * `messages.sqlite` — a consistent snapshot taken with `VACUUM INTO`
//!   through a separate read connection, never a copy of the live WAL file,
//! * `manifest.json` — format, Fyr version, creation time and the size and
//!   SHA-256 of every other entry.
//!
//! Large content (maps, books, models, misc, inbox, indexes) is excluded.
//!
//! `POST /api/restore` takes such a zip as a multipart upload. Everything is
//! validated before anything is written: entry names against a fixed
//! allow-list (no traversal, absolute paths, symlinks, directories or
//! duplicates), entry count and per-entry/total uncompressed size caps that
//! are enforced on the bytes actually decompressed (zip bombs cannot lie
//! their way past them), the manifest's sizes and hashes, and each file's
//! content (JSON shape, SQLite integrity + exact schema).
//!
//! Applying: `settings.json` is reloaded in memory, `curated-content.json` and
//! POI files are atomically renamed into place (POIs not in the backup are
//! kept). `messages.sqlite` and `download_tasks.json` belong to running
//! services that keep their own state in memory, so they are staged in
//! `DATA_DIR/restore-pending/` and swapped in by [`apply_pending_restore`] on
//! the next start, before those services open their files. The response says
//! `restart_required: true` in that case.
//!
//! Both routes live in the admin `protected` router; see AGENTS.md and
//! DEVELOPER_MANUAL §3.5.

use crate::AppState;
use axum::{
    body::{Body, Bytes},
    extract::{Multipart, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ring::digest;
use rusqlite::{config::DbConfig, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::{error, info, warn};
use types::{AppSettings, Config, DownloadSource, DownloadStatus, DownloadTask};
use uuid::Uuid;

pub const FORMAT: &str = "fyr-backup";
pub const FORMAT_VERSION: u32 = 1;

const MIB: u64 = 1024 * 1024;

/// Largest accepted restore upload (the compressed zip).
#[cfg(not(test))]
pub const MAX_UPLOAD_BYTES: u64 = 512 * MIB;
/// Small in tests so the cap can be exercised without huge bodies.
#[cfg(test)]
pub const MAX_UPLOAD_BYTES: u64 = 2 * MIB;

/// `.fgb` POI files above this size are left out of backups.
pub const MAX_BACKUP_FGB_BYTES: u64 = 16 * MIB;

/// Directory (inside `DATA_DIR`) for in-flight backup/restore temp files.
const TMP_DIR: &str = ".backup-tmp";
/// Directory (inside `DATA_DIR`) holding files applied on the next start.
pub const PENDING_DIR: &str = "restore-pending";

const MANIFEST: &str = "manifest.json";
const SETTINGS: &str = "settings.json";
const CURATED: &str = "curated-content.json";
const DOWNLOAD_TASKS: &str = "download_tasks.json";
const MESSAGES: &str = "messages.sqlite";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MAX_RESTORED_TASKS: usize = 10_000;

/// Caps applied while reading an uploaded backup. All sizes are measured
/// on the decompressed bytes, not the sizes the zip claims.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_json_bytes: u64,
    pub max_fgb_bytes: u64,
    pub max_messages_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            max_total_bytes: 1024 * MIB,
            max_manifest_bytes: MIB,
            max_json_bytes: 64 * MIB,
            max_fgb_bytes: 64 * MIB,
            max_messages_bytes: 1024 * MIB,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub fyr_version: String,
    pub created_at: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// The only entry names a backup may contain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EntryKind {
    Manifest,
    Settings,
    Curated,
    DownloadTasks,
    Messages,
    Poi(String),
}

fn classify_entry(name: &str) -> Option<EntryKind> {
    match name {
        MANIFEST => Some(EntryKind::Manifest),
        SETTINGS => Some(EntryKind::Settings),
        CURATED => Some(EntryKind::Curated),
        DOWNLOAD_TASKS => Some(EntryKind::DownloadTasks),
        MESSAGES => Some(EntryKind::Messages),
        _ => {
            let file = name.strip_prefix("poi/")?;
            is_safe_poi_name(file).then(|| EntryKind::Poi(file.to_string()))
        }
    }
}

/// A plain POI file name: no separators, no traversal, no hidden files, no
/// control or drive characters, and a POI extension.
pub fn is_safe_poi_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 200 || name.starts_with('.') || name.ends_with(' ') {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return false;
    }
    poi_extension(name).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoiFormat {
    Json,
    Fgb,
}

fn poi_extension(name: &str) -> Option<PoiFormat> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".geojson") || lower.ends_with(".json") {
        Some(PoiFormat::Json)
    } else if lower.ends_with(".fgb") {
        Some(PoiFormat::Fgb)
    } else {
        None
    }
}

fn entry_cap(kind: &EntryKind, limits: &Limits) -> u64 {
    match kind {
        EntryKind::Manifest => limits.max_manifest_bytes,
        EntryKind::Messages => limits.max_messages_bytes,
        EntryKind::Poi(name) if poi_extension(name) == Some(PoiFormat::Fgb) => limits.max_fgb_bytes,
        _ => limits.max_json_bytes,
    }
}

fn is_lower_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ---------------------------------------------------------------------------
// Temp/pending directories
// ---------------------------------------------------------------------------

fn tmp_dir(config: &Config) -> PathBuf {
    config.data_dir.join(TMP_DIR)
}

fn pending_dir(config: &Config) -> PathBuf {
    config.data_dir.join(PENDING_DIR)
}

/// Create a directory readable by the owner only (0700 on Unix).
fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn ensure_tmp_dir(config: &Config) -> io::Result<PathBuf> {
    let dir = tmp_dir(config);
    if let Ok(meta) = fs::symlink_metadata(&dir) {
        if !meta.is_dir() {
            return Err(io::Error::new(io::ErrorKind::Other, "backup temp path is not a directory"));
        }
    }
    create_private_dir(&dir)?;
    Ok(dir)
}

fn set_private_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Startup hook: remove leftover temp files and swap in files staged by a
/// previous restore. Must run before the download manager and the messages
/// database are opened.
pub fn prepare_on_startup(config: &Config) -> anyhow::Result<Vec<String>> {
    let tmp = tmp_dir(config);
    if fs::symlink_metadata(&tmp).map(|m| m.is_dir()).unwrap_or(false) {
        if let Err(error) = fs::remove_dir_all(&tmp) {
            warn!("Failed to clean backup temp directory: {}", error);
        }
    }
    apply_pending_restore(config)
}

/// Swap `restore-pending/*` into the data directory. Idempotent: if the
/// process dies half way, the next start finishes the job.
pub fn apply_pending_restore(config: &Config) -> anyhow::Result<Vec<String>> {
    let pending = pending_dir(config);
    match fs::symlink_metadata(&pending) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            warn!("Ignoring {} because it is not a directory", PENDING_DIR);
            return Ok(Vec::new());
        }
        Err(_) => return Ok(Vec::new()),
    }

    let mut applied = Vec::new();
    let staged_db = pending.join(MESSAGES);
    if staged_db.is_file() {
        let live = config.messages_db_path();
        // Drop the old WAL/SHM first: a stale WAL must never be replayed
        // into a different database file.
        for suffix in ["-wal", "-shm", "-journal"] {
            let side = PathBuf::from(format!("{}{}", live.display(), suffix));
            match fs::remove_file(&side) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(anyhow::anyhow!("Failed to remove {}: {}", side.display(), error)),
            }
        }
        fs::rename(&staged_db, &live)?;
        set_private_file(&live);
        applied.push(MESSAGES.to_string());
    }

    let staged_tasks = pending.join(DOWNLOAD_TASKS);
    if staged_tasks.is_file() {
        fs::rename(&staged_tasks, config.data_dir.join(DOWNLOAD_TASKS))?;
        applied.push(DOWNLOAD_TASKS.to_string());
    }

    fs::remove_dir_all(&pending)?;
    if !applied.is_empty() {
        info!("Applied restored files from a backup: {}", applied.join(", "));
    }
    Ok(applied)
}

/// True when a restore is waiting for a restart.
pub fn restart_pending(config: &Config) -> bool {
    pending_dir(config).is_dir()
}

// ---------------------------------------------------------------------------
// Backup
// ---------------------------------------------------------------------------

struct HashingWriter<W: Write> {
    inner: W,
    ctx: digest::Context,
    written: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner, ctx: digest::Context::new(&digest::SHA256), written: 0 }
    }

    fn finish(self) -> (W, u64, String) {
        (self.inner, self.written, hex::encode(self.ctx.finish().as_ref()))
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.ctx.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Take a transactionally consistent copy of a (possibly busy, WAL-mode)
/// SQLite database with `VACUUM INTO`, using its own connection.
pub fn snapshot_sqlite(source: &Path, dest: &Path) -> anyhow::Result<()> {
    let dest_str = dest
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("snapshot path is not valid UTF-8"))?;
    let conn = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute("VACUUM INTO ?1", [dest_str])?;
    set_private_file(dest);
    Ok(())
}

/// A finished backup zip in the temp directory.
pub struct BuiltBackup {
    pub path: PathBuf,
    pub manifest: Manifest,
}

enum Source {
    Bytes(Vec<u8>),
    File(PathBuf),
}

/// Build a backup zip. Blocking.
pub fn build_backup(
    config: &Config,
    settings: &AppSettings,
    tasks: &[DownloadTask],
) -> anyhow::Result<BuiltBackup> {
    let tmp = ensure_tmp_dir(config)?;
    let id = Uuid::new_v4();
    let zip_path = tmp.join(format!("{id}.zip"));
    let snapshot_path = tmp.join(format!("{id}.sqlite"));

    let result = build_backup_into(config, settings, tasks, &zip_path, &snapshot_path);
    let _ = fs::remove_file(&snapshot_path);
    match result {
        Ok(manifest) => Ok(BuiltBackup { path: zip_path, manifest }),
        Err(error) => {
            let _ = fs::remove_file(&zip_path);
            Err(error)
        }
    }
}

fn build_backup_into(
    config: &Config,
    settings: &AppSettings,
    tasks: &[DownloadTask],
    zip_path: &Path,
    snapshot_path: &Path,
) -> anyhow::Result<Manifest> {
    let mut sources: Vec<(String, Source)> = Vec::new();
    sources.push((SETTINGS.to_string(), Source::Bytes(serde_json::to_vec_pretty(settings)?)));

    let curated = config.curated_content_path();
    if is_regular_file(&curated) {
        sources.push((CURATED.to_string(), Source::File(curated)));
    }

    let task_map: BTreeMap<&str, &DownloadTask> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    sources.push((DOWNLOAD_TASKS.to_string(), Source::Bytes(serde_json::to_vec_pretty(&task_map)?)));

    let messages_db = config.messages_db_path();
    if is_regular_file(&messages_db) {
        snapshot_sqlite(&messages_db, snapshot_path)?;
        sources.push((MESSAGES.to_string(), Source::File(snapshot_path.to_path_buf())));
    }

    let limits = Limits::default();
    let mut poi: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(config.poi_dir()) {
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else { continue };
            if !is_safe_poi_name(&name) {
                continue;
            }
            // symlink_metadata: symlinks are skipped, never followed.
            let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
            if !meta.is_file() {
                continue;
            }
            let cap = match poi_extension(&name) {
                Some(PoiFormat::Fgb) => MAX_BACKUP_FGB_BYTES,
                _ => limits.max_json_bytes,
            };
            if meta.len() > cap {
                info!("Backup: skipping large POI file {} ({} bytes)", name, meta.len());
                continue;
            }
            poi.push((name, entry.path()));
        }
    }
    poi.sort();
    poi.truncate(limits.max_entries.saturating_sub(sources.len() + 1));
    for (name, path) in poi {
        sources.push((format!("poi/{name}"), Source::File(path)));
    }

    let file = File::create(zip_path)?;
    set_private_file(zip_path);
    let mut zip = zip::ZipWriter::new(file);
    let mut files = Vec::new();
    for (name, source) in sources {
        let size_hint = match &source {
            Source::Bytes(bytes) => bytes.len() as u64,
            Source::File(path) => fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        };
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600)
            .large_file(size_hint >= u32::MAX as u64);
        zip.start_file(name.clone(), options)?;
        let mut writer = HashingWriter::new(&mut zip);
        match source {
            Source::Bytes(bytes) => writer.write_all(&bytes)?,
            Source::File(path) => {
                let mut input = File::open(&path)?;
                io::copy(&mut input, &mut writer)?;
            }
        }
        let (_, size, sha256) = writer.finish();
        files.push(ManifestFile { path: name, size, sha256 });
    }

    let manifest = Manifest {
        format: FORMAT.to_string(),
        format_version: FORMAT_VERSION,
        fyr_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        files,
    };
    zip.start_file(
        MANIFEST,
        zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600),
    )?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    let mut file = zip.finish()?;
    file.flush()?;
    file.sync_all()?;
    Ok(manifest)
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message, "message": message }))).into_response()
}

/// GET /api/backup — stream a backup zip (admin only; protected router).
pub async fn download_backup(State(state): State<Arc<AppState>>) -> Response {
    if restart_pending(&state.config) {
        warn!("Backup requested while a restore waits for a restart; it contains the running (pre-restore) messages database");
    }
    let settings = state.settings_manager.current();
    let tasks = state.download_manager.list_tasks().await;
    let config = Arc::clone(&state.config);
    let built = match tokio::task::spawn_blocking(move || build_backup(&config, &settings, &tasks)).await {
        Ok(Ok(built)) => built,
        Ok(Err(err)) => {
            error!("Backup failed: {:#}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Backup failed. See the server log.");
        }
        Err(err) => {
            error!("Backup task failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Backup failed. See the server log.");
        }
    };

    let path = built.path.clone();
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(err) => {
            error!("Failed to open backup archive: {}", err);
            let _ = tokio::fs::remove_file(&path).await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Backup failed. See the server log.");
        }
    };
    let length = file.metadata().await.map(|m| m.len()).ok();

    // Stream the file in chunks and delete it afterwards (also when the
    // client goes away mid-download).
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(4);
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(err)).await;
                    break;
                }
            }
        }
        drop(file);
        if let Err(err) = tokio::fs::remove_file(&path).await {
            warn!("Failed to remove temporary backup archive: {}", err);
        }
    });

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut response = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"fyr-backup-{stamp}.zip\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    info!("Backup created ({} files)", built.manifest.files.len());
    response
}

// ---------------------------------------------------------------------------
// Restore: validation
// ---------------------------------------------------------------------------

/// A user-facing restore failure. Messages never contain host paths.
#[derive(Debug)]
pub struct RestoreError {
    pub status: StatusCode,
    pub message: String,
}

impl RestoreError {
    fn invalid(message: impl Into<String>) -> Self {
        Self { status: StatusCode::UNPROCESSABLE_ENTITY, message: message.into() }
    }

    fn too_large(message: impl Into<String>) -> Self {
        Self { status: StatusCode::PAYLOAD_TOO_LARGE, message: message.into() }
    }

    fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        error!("Restore failed ({}): {}", context, error);
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Restore failed on the server. See the server log.".into(),
        }
    }
}

/// A validated backup extracted into a private staging directory. The
/// directory is removed when this value is dropped.
pub struct PreparedRestore {
    staging: PathBuf,
    pub manifest: Manifest,
    pub settings: Option<AppSettings>,
    pub curated: bool,
    pub download_tasks: bool,
    pub messages: bool,
    pub poi: Vec<String>,
}

impl Drop for PreparedRestore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staging);
    }
}

/// Read at most `cap` decompressed bytes of `reader` into `dest`, hashing
/// them. Fails when the entry is larger than `cap`.
fn copy_capped(reader: &mut impl Read, dest: &mut impl Write, cap: u64) -> io::Result<(u64, String)> {
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut limited = reader.take(cap.saturating_add(1));
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = limited.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > cap {
            return Err(io::Error::new(io::ErrorKind::Other, "entry exceeds size cap"));
        }
        ctx.update(&buf[..n]);
        dest.write_all(&buf[..n])?;
    }
    Ok((total, hex::encode(ctx.finish().as_ref())))
}

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

/// Validate the uploaded zip at `upload` and extract it into a staging
/// directory. Nothing in the live data directory is touched. Blocking.
pub fn prepare_restore(config: &Config, upload: &Path, limits: &Limits) -> Result<PreparedRestore, RestoreError> {
    let tmp = ensure_tmp_dir(config).map_err(|e| RestoreError::internal("temp dir", e))?;
    let staging = tmp.join(format!("restore-{}", Uuid::new_v4()));
    create_private_dir(&staging).map_err(|e| RestoreError::internal("staging dir", e))?;
    let mut prepared = PreparedRestore {
        staging: staging.clone(),
        manifest: Manifest {
            format: String::new(),
            format_version: 0,
            fyr_version: String::new(),
            created_at: String::new(),
            files: Vec::new(),
        },
        settings: None,
        curated: false,
        download_tasks: false,
        messages: false,
        poi: Vec::new(),
    };

    let file = File::open(upload).map_err(|e| RestoreError::internal("open upload", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| RestoreError::invalid("The file is not a valid zip archive."))?;

    if archive.len() == 0 {
        return Err(RestoreError::invalid("The backup is empty."));
    }
    if archive.len() > limits.max_entries {
        return Err(RestoreError::too_large(format!(
            "The backup has too many entries (limit {}).",
            limits.max_entries
        )));
    }

    // Pass 1: names, types and duplicates — before decompressing anything.
    let mut kinds: HashMap<String, (usize, EntryKind)> = HashMap::new();
    let mut lowercase_names = HashSet::new();
    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| RestoreError::invalid("The zip archive is corrupt."))?;
        let name = entry.name().to_string();
        let kind = validate_entry_name(&name)?;
        if entry.is_dir() {
            return Err(RestoreError::invalid(format!("Unexpected directory entry \"{}\".", printable(&name))));
        }
        if let Some(mode) = entry.unix_mode() {
            let file_type = mode & S_IFMT;
            if file_type != 0 && file_type != S_IFREG {
                return Err(RestoreError::invalid(format!(
                    "Entry \"{}\" is not a regular file (symlinks and special files are rejected).",
                    printable(&name)
                )));
            }
        }
        // Encrypted entries fail later in `by_index` ("corrupt"); they are
        // never produced by Fyr.
        if entry.size() > entry_cap(&kind, limits) {
            return Err(RestoreError::too_large(format!("Entry \"{}\" is too large.", printable(&name))));
        }
        declared_total = declared_total.saturating_add(entry.size());
        if declared_total > limits.max_total_bytes {
            return Err(RestoreError::too_large("The backup is too large when uncompressed."));
        }
        if !lowercase_names.insert(name.to_lowercase()) {
            return Err(RestoreError::invalid(format!("Duplicate entry \"{}\".", printable(&name))));
        }
        kinds.insert(name, (index, kind));
    }

    // Manifest.
    let (manifest_index, _) = kinds
        .get(MANIFEST)
        .cloned()
        .ok_or_else(|| RestoreError::invalid("The backup has no manifest.json; it was not made by Fyr."))?;
    let mut manifest_bytes = Vec::new();
    {
        let mut entry = archive
            .by_index(manifest_index)
            .map_err(|_| RestoreError::invalid("The zip archive is corrupt."))?;
        copy_capped(&mut entry, &mut manifest_bytes, limits.max_manifest_bytes)
            .map_err(|_| RestoreError::too_large("manifest.json is too large or corrupt."))?;
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| RestoreError::invalid("manifest.json is not valid."))?;
    if manifest.format != FORMAT {
        return Err(RestoreError::invalid("manifest.json does not describe a Fyr backup."));
    }
    if manifest.format_version == 0 || manifest.format_version > FORMAT_VERSION {
        return Err(RestoreError::invalid(format!(
            "Unsupported backup format version {} (this server reads up to {}).",
            manifest.format_version, FORMAT_VERSION
        )));
    }
    if manifest.fyr_version.len() > 64 || manifest.created_at.len() > 64 {
        return Err(RestoreError::invalid("manifest.json is not valid."));
    }

    // Manifest and archive must list exactly the same files.
    let mut listed = HashSet::new();
    for file in &manifest.files {
        if file.path == MANIFEST || !is_lower_hex64(&file.sha256) {
            return Err(RestoreError::invalid("manifest.json is not valid."));
        }
        if !listed.insert(file.path.as_str()) {
            return Err(RestoreError::invalid("manifest.json lists a file twice."));
        }
        if !kinds.contains_key(&file.path) {
            return Err(RestoreError::invalid(format!(
                "manifest.json lists \"{}\" but the archive does not contain it.",
                printable(&file.path)
            )));
        }
    }
    for name in kinds.keys() {
        if name != MANIFEST && !listed.contains(name.as_str()) {
            return Err(RestoreError::invalid(format!(
                "The archive contains \"{}\" which manifest.json does not list.",
                printable(name)
            )));
        }
    }

    // Pass 2: extract with actual-size caps and verify sizes and hashes.
    let mut extracted_total = 0u64;
    for file in &manifest.files {
        let (index, kind) = kinds[&file.path].clone();
        let cap = entry_cap(&kind, limits).min(file.size);
        let staged = staging_path(&staging, &kind);
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|e| RestoreError::internal("create staged file", e))?;
        let mut entry = archive
            .by_index(index)
            .map_err(|_| RestoreError::invalid("The zip archive is corrupt."))?;
        let (size, sha256) = copy_capped(&mut entry, &mut out, cap).map_err(|_| {
            RestoreError::invalid(format!(
                "\"{}\" does not match its manifest size or is corrupt.",
                printable(&file.path)
            ))
        })?;
        drop(entry);
        out.sync_all().map_err(|e| RestoreError::internal("sync staged file", e))?;
        if size != file.size {
            return Err(RestoreError::invalid(format!(
                "\"{}\" does not match its manifest size.",
                printable(&file.path)
            )));
        }
        if !crate::auth::constant_time_eq(sha256.as_bytes(), file.sha256.as_bytes()) {
            return Err(RestoreError::invalid(format!(
                "Checksum mismatch for \"{}\"; the backup is damaged or was modified.",
                printable(&file.path)
            )));
        }
        extracted_total += size;
        if extracted_total > limits.max_total_bytes {
            return Err(RestoreError::too_large("The backup is too large when uncompressed."));
        }
        validate_content(&kind, &staged, &staging, &mut prepared)?;
    }
    prepared.poi.sort();
    prepared.manifest = manifest;
    Ok(prepared)
}

fn printable(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect()
}

fn validate_entry_name(name: &str) -> Result<EntryKind, RestoreError> {
    let unsafe_path = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.contains(':')
        || name.split('/').any(|part| part.is_empty() || part == "." || part == "..");
    if unsafe_path {
        return Err(RestoreError::invalid(format!(
            "Unsafe path \"{}\" in backup (absolute paths and traversal are rejected).",
            printable(name)
        )));
    }
    classify_entry(name)
        .ok_or_else(|| RestoreError::invalid(format!("Unexpected entry \"{}\" in backup.", printable(name))))
}

fn staging_path(staging: &Path, kind: &EntryKind) -> PathBuf {
    match kind {
        EntryKind::Manifest => staging.join(MANIFEST),
        EntryKind::Settings => staging.join(SETTINGS),
        EntryKind::Curated => staging.join(CURATED),
        EntryKind::DownloadTasks => staging.join(DOWNLOAD_TASKS),
        EntryKind::Messages => staging.join(MESSAGES),
        // Name already validated by `is_safe_poi_name`; prefixed so it can
        // never collide with the fixed names above.
        EntryKind::Poi(name) => staging.join(format!("poi-{name}")),
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value, ()> {
    let file = File::open(path).map_err(|_| ())?;
    serde_json::from_reader(io::BufReader::new(file)).map_err(|_| ())
}

fn validate_content(
    kind: &EntryKind,
    staged: &Path,
    staging: &Path,
    prepared: &mut PreparedRestore,
) -> Result<(), RestoreError> {
    match kind {
        EntryKind::Manifest => {}
        EntryKind::Settings => {
            let value = read_json(staged).map_err(|_| RestoreError::invalid("settings.json is not valid JSON."))?;
            let settings: AppSettings = serde_json::from_value(value)
                .map_err(|_| RestoreError::invalid("settings.json does not contain Fyr settings."))?;
            prepared.settings = Some(settings);
        }
        EntryKind::Curated => {
            let value = read_json(staged)
                .map_err(|_| RestoreError::invalid("curated-content.json is not valid JSON."))?;
            if !value.is_object() {
                return Err(RestoreError::invalid("curated-content.json must be a JSON object."));
            }
            prepared.curated = true;
        }
        EntryKind::DownloadTasks => {
            let raw = fs::read(staged).map_err(|e| RestoreError::internal("read tasks", e))?;
            let cleaned = sanitize_download_tasks(&raw)?;
            fs::write(staged, cleaned).map_err(|e| RestoreError::internal("write tasks", e))?;
            prepared.download_tasks = true;
        }
        EntryKind::Messages => {
            validate_messages_db(staged, staging)?;
            prepared.messages = true;
        }
        EntryKind::Poi(name) => {
            match poi_extension(name) {
                Some(PoiFormat::Fgb) => {
                    let mut magic = [0u8; 3];
                    let ok = File::open(staged)
                        .and_then(|mut f| f.read_exact(&mut magic))
                        .map(|_| &magic == b"fgb")
                        .unwrap_or(false);
                    if !ok {
                        return Err(RestoreError::invalid(format!("poi/{} is not a FlatGeobuf file.", printable(name))));
                    }
                }
                _ => {
                    read_json(staged).map_err(|_| {
                        RestoreError::invalid(format!("poi/{} is not valid JSON.", printable(name)))
                    })?;
                }
            }
            prepared.poi.push(name.clone());
        }
    }
    Ok(())
}

fn is_safe_task_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Keep only well-formed URL tasks whose id is a safe file-name component
/// (ids are used to name partial files) and make them all inert: in-flight
/// tasks become failed and nothing is resumable, because partial files are
/// never part of a backup. Local-import tasks are dropped: they refer to
/// inbox files that are not in the backup.
fn sanitize_download_tasks(raw: &[u8]) -> Result<Vec<u8>, RestoreError> {
    let tasks: HashMap<String, DownloadTask> = serde_json::from_slice(raw)
        .map_err(|_| RestoreError::invalid("download_tasks.json is not valid."))?;
    let mut kept: BTreeMap<String, DownloadTask> = BTreeMap::new();
    let mut dropped = 0usize;
    for (key, mut task) in tasks {
        let url_ok = matches!(
            &task.source,
            DownloadSource::Url { url } if (url.starts_with("http://") || url.starts_with("https://")) && url.len() <= 8192
        );
        if key != task.id || !is_safe_task_id(&task.id) || !url_ok || kept.len() >= MAX_RESTORED_TASKS {
            dropped += 1;
            continue;
        }
        if !matches!(
            task.status,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        ) {
            task.status = DownloadStatus::Failed;
            task.error = Some("Interrupted: restored from a backup".to_string());
        }
        task.resumable = false;
        task.resumed_from = None;
        kept.insert(key, task);
    }
    if dropped > 0 {
        info!("Restore: dropped {} download task(s) that cannot be restored", dropped);
    }
    serde_json::to_vec_pretty(&kept).map_err(|e| RestoreError::internal("serialize tasks", e))
}

/// Check that an uploaded messages database is intact and has exactly the
/// schema this build creates. The database is opened in SQLite's defensive
/// mode with an untrusted schema, and any trigger, view or unknown object
/// is rejected, so a crafted file cannot run SQL when the server uses it.
fn validate_messages_db(path: &Path, staging: &Path) -> Result<(), RestoreError> {
    let bad = || RestoreError::invalid("messages.sqlite is not a valid Fyr messages database.");
    let mut header = [0u8; 16];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|_| bad())?;
    if &header != SQLITE_HEADER {
        return Err(bad());
    }

    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| bad())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true).map_err(|_| bad())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false).map_err(|_| bad())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false).map_err(|_| bad())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false).map_err(|_| bad())?;

    let integrity: String = conn
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .map_err(|_| bad())?;
    if integrity != "ok" {
        return Err(RestoreError::invalid("messages.sqlite failed SQLite's integrity check."));
    }

    let schema = schema_of(&conn).map_err(|_| bad())?;
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| row.get(0))
        .map_err(|_| bad())?;
    if version > crate::messaging::store::SCHEMA_VERSION {
        return Err(RestoreError::invalid(
            "messages.sqlite comes from a newer Fyr version; upgrade this server first.",
        ));
    }
    if version < 1 {
        return Err(bad());
    }

    // Compare against a freshly created database of this build.
    let reference_path = staging.join("reference.sqlite");
    let reference = {
        let store = crate::messaging::store::Store::open(&reference_path)
            .map_err(|e| RestoreError::internal("reference schema", e))?;
        drop(store);
        let conn = Connection::open(&reference_path).map_err(|e| RestoreError::internal("reference schema", e))?;
        schema_of(&conn).map_err(|e| RestoreError::internal("reference schema", e))?
    };
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", reference_path.display(), suffix));
    }
    if version == crate::messaging::store::SCHEMA_VERSION && schema != reference {
        return Err(RestoreError::invalid("messages.sqlite has an unexpected schema."));
    }
    if schema.keys().any(|(kind, _)| kind != "table" && kind != "index") {
        return Err(RestoreError::invalid("messages.sqlite contains triggers or views."));
    }

    // Store it in rollback-journal mode so it is a single self-contained
    // file; the server switches it back to WAL when it opens it.
    conn.pragma_update(None, "journal_mode", "DELETE").map_err(|_| bad())?;
    drop(conn);
    for suffix in ["-wal", "-shm", "-journal"] {
        let _ = fs::remove_file(format!("{}{}", path.display(), suffix));
    }
    Ok(())
}

type Schema = BTreeMap<(String, String), Option<String>>;

fn schema_of(conn: &Connection) -> rusqlite::Result<Schema> {
    let mut stmt = conn.prepare("SELECT type, name, sql FROM sqlite_schema")?;
    let rows = stmt.query_map([], |row| {
        Ok(((row.get::<_, String>(0)?, row.get::<_, String>(1)?), row.get::<_, Option<String>>(2)?))
    })?;
    rows.collect()
}

// ---------------------------------------------------------------------------
// Restore: apply
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq)]
pub struct RestoreResponse {
    pub message: String,
    pub backup_created_at: String,
    pub backup_fyr_version: String,
    /// Files applied immediately.
    pub restored: Vec<String>,
    /// Files applied on the next server start.
    pub pending_restart: Vec<String>,
    pub restart_required: bool,
}

/// Rename `src` over `dest`. Both live under `DATA_DIR`; if they are on
/// different filesystems, copy next to `dest` first so the final step is
/// still an atomic rename.
fn move_into_place(src: &Path, dest: &Path) -> io::Result<()> {
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    let parent = dest.parent().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no parent"))?;
    let tmp = parent.join(format!(".restore-{}.tmp", Uuid::new_v4()));
    fs::copy(src, &tmp)?;
    if let Err(error) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    let _ = fs::remove_file(src);
    Ok(())
}

/// Move validated files into place (live files) or into
/// `restore-pending/` (files of running services). Blocking. Settings are
/// applied by the caller through the `SettingsManager`.
pub fn apply_files(config: &Config, prepared: &PreparedRestore) -> Result<(Vec<String>, Vec<String>), RestoreError> {
    let mut restored = Vec::new();
    let mut pending = Vec::new();

    // Stage files for the next start first. A newer restore always replaces
    // a pending older one.
    let tmp = ensure_tmp_dir(config).map_err(|e| RestoreError::internal("temp dir", e))?;
    let next_pending = tmp.join(format!("pending-{}", Uuid::new_v4()));
    create_private_dir(&next_pending).map_err(|e| RestoreError::internal("pending dir", e))?;
    for (wanted, name) in [(prepared.messages, MESSAGES), (prepared.download_tasks, DOWNLOAD_TASKS)] {
        if wanted {
            fs::rename(prepared.staging.join(name), next_pending.join(name))
                .map_err(|e| RestoreError::internal("stage pending file", e))?;
            pending.push(name.to_string());
        }
    }
    let live_pending = pending_dir(config);
    if fs::symlink_metadata(&live_pending).is_ok() {
        fs::remove_dir_all(&live_pending).map_err(|e| RestoreError::internal("replace pending dir", e))?;
    }
    if pending.is_empty() {
        let _ = fs::remove_dir_all(&next_pending);
    } else {
        fs::rename(&next_pending, &live_pending).map_err(|e| RestoreError::internal("pending dir", e))?;
    }

    if prepared.curated {
        move_into_place(&prepared.staging.join(CURATED), &config.curated_content_path())
            .map_err(|e| RestoreError::internal("curated content", e))?;
        restored.push(CURATED.to_string());
    }

    if !prepared.poi.is_empty() {
        let poi_dir = config.poi_dir();
        fs::create_dir_all(&poi_dir).map_err(|e| RestoreError::internal("poi dir", e))?;
        for name in &prepared.poi {
            move_into_place(&staging_path(&prepared.staging, &EntryKind::Poi(name.clone())), &poi_dir.join(name))
                .map_err(|e| RestoreError::internal("poi file", e))?;
            restored.push(format!("poi/{name}"));
        }
    }
    Ok((restored, pending))
}

/// One restore at a time per data directory.
fn restore_lock() -> &'static Mutex<HashSet<PathBuf>> {
    static LOCK: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(HashSet::new()))
}

struct RestoreGuard(PathBuf);

impl RestoreGuard {
    fn acquire(data_dir: &Path) -> Option<Self> {
        let mut active = restore_lock().lock().unwrap_or_else(|e| e.into_inner());
        active.insert(data_dir.to_path_buf()).then(|| Self(data_dir.to_path_buf()))
    }
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        restore_lock().lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// POST /api/restore — restore a backup zip (admin only; protected router).
pub async fn restore_backup(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let Some(_guard) = RestoreGuard::acquire(&state.config.data_dir) else {
        return json_error(StatusCode::CONFLICT, "Another restore is already running.");
    };
    let tmp = match ensure_tmp_dir(&state.config) {
        Ok(dir) => dir,
        Err(err) => {
            error!("Restore: cannot create temp directory: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
        }
    };
    let upload = RemoveOnDrop(tmp.join(format!("{}.upload", Uuid::new_v4())));

    let mut received = false;
    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return multipart_error(err.status(), &err.body_text()),
        };
        if field.name() != Some("file") {
            continue;
        }
        let mut out = match tokio::fs::File::create(&upload.0).await {
            Ok(file) => file,
            Err(err) => {
                error!("Restore: cannot create upload file: {}", err);
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
            }
        };
        let mut size = 0u64;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as u64;
                    if size > MAX_UPLOAD_BYTES {
                        return json_error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            &format!("The backup file is larger than {} MiB.", MAX_UPLOAD_BYTES / MIB),
                        );
                    }
                    use tokio::io::AsyncWriteExt;
                    if let Err(err) = out.write_all(&chunk).await {
                        error!("Restore: cannot write upload: {}", err);
                        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
                    }
                }
                Ok(None) => break,
                Err(err) => return multipart_error(err.status(), &err.body_text()),
            }
        }
        use tokio::io::AsyncWriteExt;
        if out.flush().await.is_err() {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
        }
        received = size > 0;
        break;
    }
    if !received {
        return json_error(StatusCode::BAD_REQUEST, "No backup file was uploaded (form field \"file\").");
    }

    let config = Arc::clone(&state.config);
    let upload_path = upload.0.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let prepared = prepare_restore(&config, &upload_path, &Limits::default())?;
        let (restored, pending) = apply_files(&config, &prepared)?;
        Ok::<_, RestoreError>((prepared.manifest.clone(), prepared.settings.clone(), restored, pending))
    })
    .await;
    drop(upload);

    let (manifest, settings, mut restored, pending) = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            warn!("Restore rejected: {}", err.message);
            return json_error(err.status, &err.message);
        }
        Err(err) => {
            error!("Restore task failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
        }
    };

    if let Some(settings) = settings {
        match state.settings_manager.replace(settings) {
            Ok(updated) => {
                let timeout = crate::handlers::resolve_download_request_timeout_secs(&updated);
                state.download_manager.set_request_timeout_secs(timeout).await;
                restored.insert(0, SETTINGS.to_string());
            }
            Err(err) => {
                error!("Restore: failed to save settings: {}", err);
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Restore failed on the server. See the server log.");
            }
        }
    }

    let restart_required = !pending.is_empty();
    let message = if restart_required {
        format!(
            "Backup restored. Restart Fyr to finish restoring {}.",
            pending.join(" and ")
        )
    } else {
        "Backup restored.".to_string()
    };
    info!(
        "Restore applied: {} file(s) now, {} after restart",
        restored.len(),
        pending.len()
    );
    Json(RestoreResponse {
        message,
        backup_created_at: manifest.created_at,
        backup_fyr_version: manifest.fyr_version,
        restored,
        pending_restart: pending,
        restart_required,
    })
    .into_response()
}

fn multipart_error(status: StatusCode, detail: &str) -> Response {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("The backup file is larger than {} MiB.", MAX_UPLOAD_BYTES / MIB),
        );
    }
    warn!("Restore: invalid multipart upload: {}", detail);
    json_error(StatusCode::BAD_REQUEST, "Invalid upload request.")
}

#[cfg(test)]
mod tests;
