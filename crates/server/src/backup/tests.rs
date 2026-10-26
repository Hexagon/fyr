use super::*;
use crate::{ai::ModelManager, auth, mbtiles, messaging, settings::SettingsManager};
use axum::http::Request;
use axum::Router;
use downloader::DownloadManager;
use std::io::Cursor;
use tower::ServiceExt;
use types::GeoPosition;

const BOUNDARY: &str = "fyr-test-boundary-7d1c";

fn state_with(mode: &str) -> AppState {
    let data_dir = std::env::temp_dir().join(format!("fyr-backup-tests-{}", Uuid::new_v4()));
    let mut config = Config::default_with_data_dir(&data_dir);
    match mode {
        "password" => config.auth.admin_password = Some("secret".into()),
        "readonly" => config.auth.readonly = true,
        _ => {}
    }
    config.initialize_directories().expect("initialize directories");
    let config = Arc::new(config);
    AppState {
        config: Arc::clone(&config),
        static_dir: data_dir.join("static"),
        download_manager: Arc::new(DownloadManager::new(&data_dir)),
        model_manager: Arc::new(ModelManager::new(Arc::clone(&config))),
        settings_manager: Arc::new(SettingsManager::new(&data_dir)),
        auth_manager: Arc::new(auth::AuthManager::new()),
        mbtiles_pool: Arc::new(mbtiles::MbtilesPool::new()),
        messaging: Arc::new(messaging::Messaging::open(&config).expect("open messages db")),
    }
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn multipart_body(bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"backup.zip\"\r\nContent-Type: application/zip\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn get_backup(app: &Router, cookie: Option<&str>) -> Response {
    let mut request = Request::get("/api/backup");
    if let Some(token) = cookie {
        request = request.header(header::COOKIE, format!("fyr_session={token}"));
    }
    app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
}

async fn post_restore(app: &Router, zip: &[u8], cookie: Option<&str>) -> Response {
    let mut request = Request::post("/api/restore")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"));
    if let Some(token) = cookie {
        request = request.header(header::COOKIE, format!("fyr_session={token}"));
    }
    app.clone()
        .oneshot(request.body(Body::from(multipart_body(zip))).unwrap())
        .await
        .unwrap()
}

async fn body_bytes(response: Response) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()
}

async fn body_json(response: Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(response).await).expect("json body")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(digest::digest(&digest::SHA256, bytes).as_ref())
}

fn manifest_for(entries: &[(&str, Vec<u8>)]) -> Manifest {
    Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        fyr_version: "test".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        files: entries
            .iter()
            .map(|(name, bytes)| ManifestFile {
                path: name.to_string(),
                size: bytes.len() as u64,
                sha256: sha256_hex(bytes),
            })
            .collect(),
    }
}

/// Build a zip with `entries` plus a manifest (computed unless given).
fn make_zip(entries: &[(&str, Vec<u8>)], manifest: Option<Manifest>) -> Vec<u8> {
    let manifest = manifest.unwrap_or_else(|| manifest_for(entries));
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn read_zip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.insert(entry.name().to_string(), data);
    }
    out
}

fn prepare(config: &Config, zip: &[u8], limits: &Limits) -> Result<PreparedRestore, RestoreError> {
    let upload = config.data_dir.join(format!("upload-{}.zip", Uuid::new_v4()));
    fs::write(&upload, zip).unwrap();
    let result = prepare_restore(config, &upload, limits);
    let _ = fs::remove_file(&upload);
    result
}

fn public_message(text: &str) -> messaging::store::NewPublicMessage {
    messaging::store::NewPublicMessage {
        nick: "Ada".into(),
        device_tag: "a1b2c3".into(),
        category: Some("info".into()),
        text: text.into(),
        location: Some((59.3, 18.0)),
        expires_at: i64::MAX / 2,
    }
}

fn list_all(messaging: &messaging::Messaging, channel: &str) -> Vec<messaging::store::MessageRow> {
    messaging.with_store_blocking(|store| {
        store
            .list(channel, messaging::store::ListQuery { limit: 500, ..Default::default() }, 0)
            .unwrap()
            .0
    })
}

fn valid_messages_db(dir: &Path) -> Vec<u8> {
    let path = dir.join(format!("ref-{}.sqlite", Uuid::new_v4()));
    let store = messaging::store::Store::open(&path).unwrap();
    store.server_salt(|| [9u8; 32]).unwrap();
    drop(store);
    let snapshot = dir.join(format!("snap-{}.sqlite", Uuid::new_v4()));
    snapshot_sqlite(&path, &snapshot).unwrap();
    let bytes = fs::read(&snapshot).unwrap();
    for p in [&path, &snapshot] {
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{}", p.display(), suffix));
        }
    }
    bytes
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn backup_restore_round_trip_restores_settings_pois_and_messages() {
    let state = state_with("open");
    let config = Arc::clone(&state.config);
    let _cleanup = Cleanup(config.data_dir.clone());

    let mut modules = HashMap::new();
    modules.insert("downloads".to_string(), serde_json::json!({ "request_timeout_seconds": 900 }));
    let original_settings = AppSettings {
        location: Some(GeoPosition { latitude: 59.33, longitude: 18.06, label: Some("Home".into()) }),
        modules,
    };
    state.settings_manager.replace(original_settings.clone()).unwrap();
    let geojson = br#"{"type":"FeatureCollection","features":[]}"#.to_vec();
    fs::write(config.poi_dir().join("camps.geojson"), &geojson).unwrap();
    let fgb = b"fgb\x03fgb\x00 small flatgeobuf".to_vec();
    fs::write(config.poi_dir().join("water.fgb"), &fgb).unwrap();
    fs::write(config.poi_dir().join("big.fgb"), vec![b'f'; (MAX_BACKUP_FGB_BYTES + 1) as usize]).unwrap();
    fs::write(config.maps_dir().join("world.pmtiles"), b"PMTiles-large-content").unwrap();
    let curated = br#"{"schema_version": 1, "custom": true}"#.to_vec();
    fs::write(config.curated_content_path(), &curated).unwrap();

    let channel = "ab".repeat(16);
    state.messaging.with_store_blocking(|store| {
        store.insert_public(&public_message("hello"), 1_000, 1000).unwrap();
        store.insert_public(&public_message("world"), 2_000, 1000).unwrap();
        store.create_channel(&channel, &[7u8; 32], 1_000, 10).unwrap();
        store.insert_encrypted(&channel, &[1u8; 24], &[2u8; 40], 3_000, i64::MAX / 2, 1000).unwrap();
    });
    let salt = state.messaging.server_salt_hex();
    let public_before = list_all(&state.messaging, "public");
    let encrypted_before = list_all(&state.messaging, &channel);
    assert_eq!(public_before.len(), 2);

    let app = crate::create_router(state.clone());
    let response = get_backup(&app, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment; filename=\"fyr-backup-"));
    let zip_bytes = body_bytes(response).await;

    let files = read_zip(&zip_bytes);
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "curated-content.json",
            "download_tasks.json",
            "manifest.json",
            "messages.sqlite",
            "poi/camps.geojson",
            "poi/water.fgb",
            "settings.json"
        ],
        "large fgb and map content must be excluded"
    );
    let manifest: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(manifest.format, FORMAT);
    assert_eq!(manifest.fyr_version, env!("CARGO_PKG_VERSION"));
    for file in &manifest.files {
        assert_eq!(sha256_hex(&files[&file.path]), file.sha256, "{}", file.path);
        assert_eq!(files[&file.path].len() as u64, file.size);
    }
    let data_dir_str = config.data_dir.display().to_string();
    for (name, bytes) in &files {
        assert!(
            !String::from_utf8_lossy(bytes).contains(&data_dir_str),
            "{name} leaks the data directory"
        );
    }
    // The temp archive is removed once streamed.
    let leftovers: Vec<_> = fs::read_dir(tmp_dir(&config))
        .map(|d| d.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "zip")).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "temp archive left behind: {leftovers:?}");

    // Wipe / diverge.
    state.settings_manager.replace(AppSettings::default()).unwrap();
    fs::remove_file(config.poi_dir().join("camps.geojson")).unwrap();
    fs::write(config.poi_dir().join("water.fgb"), b"fgb-changed").unwrap();
    fs::write(config.curated_content_path(), b"{}").unwrap();
    state.messaging.with_store_blocking(|store| {
        store.insert_public(&public_message("after backup"), 4_000, 1000).unwrap();
        store.delete_channel(&channel).unwrap();
    });

    let response = post_restore(&app, &zip_bytes, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = body_json(response).await;
    assert_eq!(result["restart_required"], true);
    assert_eq!(result["pending_restart"], serde_json::json!(["messages.sqlite", "download_tasks.json"]));
    assert_eq!(
        result["restored"],
        serde_json::json!(["settings.json", "curated-content.json", "poi/camps.geojson", "poi/water.fgb"])
    );
    assert!(!result.to_string().contains(&data_dir_str));

    // Live files are back immediately; settings are reloaded in memory.
    assert_eq!(state.settings_manager.current(), original_settings);
    assert_eq!(SettingsManager::new(&config.data_dir).current(), original_settings);
    assert_eq!(fs::read(config.poi_dir().join("camps.geojson")).unwrap(), geojson);
    assert_eq!(fs::read(config.poi_dir().join("water.fgb")).unwrap(), fgb);
    assert_eq!(fs::read(config.curated_content_path()).unwrap(), curated);
    assert!(restart_pending(&config));
    // The running messaging service is untouched until restart.
    assert_eq!(list_all(&state.messaging, "public").len(), 3);

    // Restart: drop the running services, apply pending files, reopen.
    drop(app);
    drop(state);
    let applied = prepare_on_startup(&config).unwrap();
    assert_eq!(applied, vec!["messages.sqlite".to_string(), "download_tasks.json".to_string()]);
    assert!(!restart_pending(&config));
    assert!(!tmp_dir(&config).exists());
    let reopened = messaging::Messaging::open(&config).unwrap();
    assert_eq!(reopened.server_salt_hex(), salt, "channel ids depend on the server salt");
    assert_eq!(list_all(&reopened, "public"), public_before);
    assert_eq!(list_all(&reopened, &channel), encrypted_before);
}

#[test]
fn snapshot_is_consistent_while_writes_happen() {
    let dir = std::env::temp_dir().join(format!("fyr-backup-snap-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let _cleanup = Cleanup(dir.clone());
    let db = dir.join("messages.sqlite");
    drop(messaging::store::Store::open(&db).unwrap());

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = {
        let db = db.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut conn = Connection::open(&db).unwrap();
            conn.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            let mut pairs = 0u32;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) || pairs < 50 {
                // Messages are always written in pairs inside one transaction.
                let tx = conn.transaction().unwrap();
                for _ in 0..2 {
                    tx.execute(
                        "INSERT INTO messages (channel_id, created_at, expires_at, body) VALUES ('public', 1, NULL, ?1)",
                        [format!("pair {pairs}")],
                    )
                    .unwrap();
                }
                tx.commit().unwrap();
                pairs += 1;
            }
            pairs
        })
    };

    let mut counts = Vec::new();
    for i in 0..8 {
        let snapshot = dir.join(format!("snap-{i}.sqlite"));
        snapshot_sqlite(&db, &snapshot).unwrap();
        let conn = Connection::open(&snapshot).unwrap();
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(integrity, "ok");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0)).unwrap();
        assert_eq!(count % 2, 0, "snapshot {i} caught a half-written transaction");
        counts.push(count);
        // A snapshot is a standalone file: no WAL needed to read it.
        assert!(!dir.join(format!("snap-{i}.sqlite-wal")).exists() || {
            let journal: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
            journal != "wal"
        });
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let pairs = writer.join().unwrap();
    assert!(counts.windows(2).all(|w| w[0] <= w[1]));
    assert!(*counts.last().unwrap() <= i64::from(pairs) * 2);
}

#[test]
fn manifest_hash_and_size_mismatches_are_rejected() {
    let state = state_with("open");
    let config = &state.config;
    let _cleanup = Cleanup(config.data_dir.clone());
    let settings = br#"{"modules":{}}"#.to_vec();

    let entries = [("settings.json", settings.clone())];
    let mut manifest = manifest_for(&entries);
    manifest.files[0].sha256 = sha256_hex(b"something else");
    let err = prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).err().unwrap();
    assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(err.message.contains("Checksum mismatch"), "{}", err.message);

    let mut manifest = manifest_for(&entries);
    manifest.files[0].size += 1;
    let err = prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).err().unwrap();
    assert!(err.message.contains("manifest size"), "{}", err.message);

    let mut manifest = manifest_for(&entries);
    manifest.files[0].size -= 1;
    let err = prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).err().unwrap();
    assert!(err.message.contains("manifest size"), "{}", err.message);

    // Unlisted entry, missing entry, missing manifest, wrong format/version.
    let mut manifest = manifest_for(&entries);
    manifest.files.clear();
    assert!(prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).is_err());
    let mut manifest = manifest_for(&entries);
    manifest.files.push(ManifestFile { path: "curated-content.json".into(), size: 2, sha256: sha256_hex(b"{}") });
    assert!(prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).is_err());
    let mut manifest = manifest_for(&entries);
    manifest.format_version = FORMAT_VERSION + 1;
    let err = prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).err().unwrap();
    assert!(err.message.contains("Unsupported backup format"), "{}", err.message);
    let mut manifest = manifest_for(&entries);
    manifest.format = "other".into();
    assert!(prepare(config, &make_zip(&entries, Some(manifest)), &Limits::default()).is_err());

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("settings.json", zip::write::FileOptions::default()).unwrap();
    zip.write_all(&settings).unwrap();
    let no_manifest = zip.finish().unwrap().into_inner();
    let err = prepare(config, &no_manifest, &Limits::default()).err().unwrap();
    assert!(err.message.contains("no manifest.json"), "{}", err.message);

    assert!(prepare(config, b"not a zip at all", &Limits::default()).is_err());

    // Content validation: hashes match but content is not what it claims.
    for (name, bytes) in [
        ("settings.json", b"[1,2,3]".to_vec()),
        ("curated-content.json", b"not json".to_vec()),
        ("poi/a.geojson", b"{oops".to_vec()),
        ("poi/a.fgb", b"PK\x03\x04".to_vec()),
        ("messages.sqlite", b"SQLite format 3\0garbage".to_vec()),
        ("download_tasks.json", b"[]".to_vec()),
    ] {
        let entries = [(name, bytes)];
        let err = prepare(config, &make_zip(&entries, None), &Limits::default()).err();
        assert!(err.is_some(), "{name} with bad content was accepted");
    }
    // Nothing was written to the live data directory.
    assert!(!config.poi_dir().join("a.geojson").exists());
    assert!(!restart_pending(config));
}

#[test]
fn unsafe_entry_names_and_symlinks_are_rejected() {
    let state = state_with("open");
    let config = &state.config;
    let _cleanup = Cleanup(config.data_dir.clone());
    let json = b"{}".to_vec();

    for name in [
        "../evil.json",
        "/etc/passwd",
        "/settings.json",
        "poi/../../evil.json",
        "poi/../settings.json",
        "poi\\evil.json",
        "C:/evil.json",
        "poi/.hidden.json",
        "poi/sub/evil.json",
        "poi//evil.json",
        "poi/./evil.json",
        "poi/evil.exe",
        "maps/world.pmtiles",
        "inbox/x.json",
        "settings.json/",
        "random.txt",
    ] {
        let entries = [(name, json.clone())];
        let err = prepare(config, &make_zip(&entries, None), &Limits::default()).err();
        let err = err.unwrap_or_else(|| panic!("{name} was accepted"));
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
        assert!(!err.message.contains(&config.data_dir.display().to_string()));
    }
    assert!(!config.data_dir.parent().unwrap().join("evil.json").exists());
    assert!(!config.data_dir.join("evil.json").exists());

    // Symlink entry (even one listed in the manifest with matching hash).
    let target = b"/etc/passwd".to_vec();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("poi/link.geojson", "/etc/passwd", zip::write::FileOptions::default()).unwrap();
    zip.start_file(MANIFEST, zip::write::FileOptions::default()).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest_for(&[("poi/link.geojson", target)])).unwrap()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let err = prepare(config, &bytes, &Limits::default()).err().expect("symlink accepted");
    assert!(err.message.contains("not a regular file"), "{}", err.message);
    assert!(!config.poi_dir().join("link.geojson").exists());

    // Directory entry.
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_directory("poi/", zip::write::FileOptions::default()).unwrap();
    zip.start_file(MANIFEST, zip::write::FileOptions::default()).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest_for(&[])).unwrap()).unwrap();
    assert!(prepare(config, &zip.finish().unwrap().into_inner(), &Limits::default()).is_err());

    // Duplicate names (also case-insensitively).
    let entries = [("poi/a.json", json.clone()), ("poi/A.json", json.clone())];
    assert!(prepare(config, &make_zip(&entries, None), &Limits::default()).is_err());
}

/// Overwrite the uncompressed-size fields of `name` in both the local and
/// the central directory header so the archive lies about its size.
fn patch_uncompressed_size(zip: &mut [u8], name: &str, size: u32) {
    let name = name.as_bytes();
    let mut i = 0;
    while i + 46 <= zip.len() {
        let sig = u32::from_le_bytes(zip[i..i + 4].try_into().unwrap());
        if sig == 0x0403_4b50 {
            let len = u16::from_le_bytes(zip[i + 26..i + 28].try_into().unwrap()) as usize;
            if zip.get(i + 30..i + 30 + len) == Some(name) {
                zip[i + 22..i + 26].copy_from_slice(&size.to_le_bytes());
            }
        } else if sig == 0x0201_4b50 {
            let len = u16::from_le_bytes(zip[i + 28..i + 30].try_into().unwrap()) as usize;
            if zip.get(i + 46..i + 46 + len) == Some(name) {
                zip[i + 24..i + 28].copy_from_slice(&size.to_le_bytes());
            }
        }
        i += 1;
    }
}

#[test]
fn zip_bomb_caps_are_enforced_on_real_bytes() {
    let state = state_with("open");
    let config = &state.config;
    let _cleanup = Cleanup(config.data_dir.clone());
    let small = Limits {
        max_entries: 5,
        max_total_bytes: MIB,
        max_manifest_bytes: 64 * 1024,
        max_json_bytes: MIB,
        max_fgb_bytes: MIB,
        max_messages_bytes: MIB,
    };

    // Too many entries.
    let many: Vec<(String, Vec<u8>)> = (0..6).map(|i| (format!("poi/p{i}.json"), b"{}".to_vec())).collect();
    let many_ref: Vec<(&str, Vec<u8>)> = many.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let err = prepare(config, &make_zip(&many_ref, None), &small).err().unwrap();
    assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);

    // Highly compressible entry larger than the per-entry cap (~10 KB zipped).
    let mut bomb = b"[".to_vec();
    bomb.extend(std::iter::repeat(b' ').take(4 * MIB as usize));
    bomb.push(b']');
    let entries = [("poi/bomb.json", bomb.clone())];
    let zipped = make_zip(&entries, None);
    assert!(zipped.len() < 64 * 1024);
    let err = prepare(config, &zipped, &small).err().unwrap();
    assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);

    // Several entries under the per-entry cap but over the total cap.
    let part = vec![b' '; 400 * 1024];
    let entries = [
        ("poi/a.json", [b"1".as_slice(), &part].concat()),
        ("poi/b.json", [b"2".as_slice(), &part].concat()),
        ("poi/c.json", [b"3".as_slice(), &part].concat()),
    ];
    let err = prepare(config, &make_zip(&entries, None), &small).err().unwrap();
    assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);

    // An archive that lies about its uncompressed size (headers and
    // manifest claim 2 bytes, the data inflates to 4 MiB): the real bytes
    // are capped, so extraction stops at 3 bytes and fails.
    let entries = [("poi/liar.json", bomb)];
    let mut manifest = manifest_for(&entries);
    manifest.files[0].size = 2;
    let mut zipped = make_zip(&entries, Some(manifest));
    patch_uncompressed_size(&mut zipped, "poi/liar.json", 2);
    let err = prepare(config, &zipped, &Limits::default()).err().unwrap();
    assert!(err.message.contains("manifest size") || err.message.contains("corrupt"), "{}", err.message);
    assert!(!config.poi_dir().join("liar.json").exists());

    // copy_capped never reads past cap + 1.
    let mut sink = Vec::new();
    assert!(copy_capped(&mut io::repeat(0).take(10_000), &mut sink, 100).is_err());
    assert!(sink.len() <= 100);
}

#[tokio::test]
async fn restore_upload_size_is_capped() {
    let state = state_with("open");
    let _cleanup = Cleanup(state.config.data_dir.clone());
    let app = crate::create_router(state);
    let oversized = vec![0u8; (MAX_UPLOAD_BYTES + 1) as usize];
    let response = post_restore(&app, &oversized, None).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = body_json(response).await;
    assert!(body["message"].as_str().unwrap().contains("larger than"));

    let empty = post_restore(&app, b"", None).await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backup_and_restore_access_matrix() {
    // Password mode: anonymous 401, admin OK.
    let state = state_with("password");
    let _cleanup = Cleanup(state.config.data_dir.clone());
    let data_dir_str = state.config.data_dir.display().to_string();
    let token = state.auth_manager.create_session();
    let app = crate::create_router(state);

    let anonymous = get_backup(&app, None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let bogus = get_backup(&app, Some("not-a-session")).await;
    assert_eq!(bogus.status(), StatusCode::UNAUTHORIZED);
    let admin = get_backup(&app, Some(&token)).await;
    assert_eq!(admin.status(), StatusCode::OK);
    let zip_bytes = body_bytes(admin).await;

    let anonymous = post_restore(&app, &zip_bytes, None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let admin = post_restore(&app, &zip_bytes, Some(&token)).await;
    assert_eq!(admin.status(), StatusCode::OK);
    let text = String::from_utf8(body_bytes(admin).await).unwrap();
    assert!(!text.contains(&data_dir_str), "{text}");

    // Errors do not leak host paths either.
    let bad = post_restore(&app, b"garbage", Some(&token)).await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let text = String::from_utf8(body_bytes(bad).await).unwrap();
    assert!(!text.contains(&data_dir_str), "{text}");

    // Read-only mode: 403 for everyone, even with a session cookie.
    let state = state_with("readonly");
    let _cleanup_ro = Cleanup(state.config.data_dir.clone());
    let token = state.auth_manager.create_session();
    let app = crate::create_router(state);
    for cookie in [None, Some(token.as_str())] {
        assert_eq!(get_backup(&app, cookie).await.status(), StatusCode::FORBIDDEN);
        assert_eq!(post_restore(&app, &zip_bytes, cookie).await.status(), StatusCode::FORBIDDEN);
    }
}

#[test]
fn restored_download_tasks_are_sanitized() {
    let raw = serde_json::json!({
        "good": {
            "id": "good", "source": { "type": "Url", "url": "https://example.org/a.pmtiles" },
            "status": "downloading", "progress": 0.5, "bytes_downloaded": 10, "total_bytes": 20,
            "error": null, "content_type": null, "created_at": "x", "updated_at": "x",
            "resumable": true, "resumed_from": 5
        },
        "../../etc/x": {
            "id": "../../etc/x", "source": { "type": "Url", "url": "https://example.org/b" },
            "status": "failed", "progress": 0.0, "bytes_downloaded": 0, "total_bytes": null,
            "error": null, "content_type": null, "created_at": "x", "updated_at": "x"
        },
        "local": {
            "id": "local", "source": { "type": "LocalFile", "path": "/etc/shadow" },
            "status": "completed", "progress": 1.0, "bytes_downloaded": 0, "total_bytes": null,
            "error": null, "content_type": null, "created_at": "x", "updated_at": "x"
        },
        "mismatch": {
            "id": "other", "source": { "type": "Url", "url": "https://example.org/c" },
            "status": "completed", "progress": 1.0, "bytes_downloaded": 0, "total_bytes": null,
            "error": null, "content_type": null, "created_at": "x", "updated_at": "x"
        },
        "file-url": {
            "id": "file-url", "source": { "type": "Url", "url": "file:///etc/passwd" },
            "status": "completed", "progress": 1.0, "bytes_downloaded": 0, "total_bytes": null,
            "error": null, "content_type": null, "created_at": "x", "updated_at": "x"
        }
    });
    let cleaned = sanitize_download_tasks(raw.to_string().as_bytes()).unwrap();
    let tasks: HashMap<String, DownloadTask> = serde_json::from_slice(&cleaned).unwrap();
    assert_eq!(tasks.keys().collect::<Vec<_>>(), vec!["good"]);
    let good = &tasks["good"];
    assert_eq!(good.status, DownloadStatus::Failed);
    assert!(!good.resumable);
    assert_eq!(good.resumed_from, None);
}

#[test]
fn messages_db_must_match_the_expected_schema() {
    let state = state_with("open");
    let config = &state.config;
    let _cleanup = Cleanup(config.data_dir.clone());
    let scratch = config.data_dir.join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let good = valid_messages_db(&scratch);
    let prepared = prepare(config, &make_zip(&[("messages.sqlite", good.clone())], None), &Limits::default())
        .expect("valid messages db accepted");
    assert!(prepared.messages);
    drop(prepared);

    let tamper = |sql: &str| -> Vec<u8> {
        let path = scratch.join(format!("t-{}.sqlite", Uuid::new_v4()));
        fs::write(&path, &good).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(sql).unwrap();
        conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
        drop(conn);
        fs::read(&path).unwrap()
    };
    for (label, bytes) in [
        (
            "trigger",
            tamper("CREATE TRIGGER t AFTER INSERT ON messages BEGIN DELETE FROM channels; END;"),
        ),
        ("view", tamper("CREATE VIEW v AS SELECT * FROM messages;")),
        ("extra table", tamper("CREATE TABLE extra (x);")),
        ("newer schema", tamper("UPDATE schema_version SET version = 99;")),
    ] {
        let err = prepare(config, &make_zip(&[("messages.sqlite", bytes)], None), &Limits::default())
            .err()
            .unwrap_or_else(|| panic!("{label} accepted"));
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY, "{label}");
    }
}

#[test]
fn pending_restore_drops_stale_wal_before_swapping_the_database() {
    let state = state_with("open");
    let config = Arc::clone(&state.config);
    let _cleanup = Cleanup(config.data_dir.clone());
    drop(state);

    let pending = pending_dir(&config);
    fs::create_dir_all(&pending).unwrap();
    let scratch = config.data_dir.join("scratch");
    fs::create_dir_all(&scratch).unwrap();
    fs::write(pending.join(MESSAGES), valid_messages_db(&scratch)).unwrap();
    let wal = PathBuf::from(format!("{}-wal", config.messages_db_path().display()));
    fs::write(&wal, b"stale wal from the old database").unwrap();

    assert_eq!(apply_pending_restore(&config).unwrap(), vec![MESSAGES.to_string()]);
    assert!(!wal.exists());
    assert!(!pending.exists());
    let messaging = messaging::Messaging::open(&config).unwrap();
    assert_eq!(messaging.server_salt_hex(), hex::encode([9u8; 32]));
    // Nothing pending: a no-op.
    assert!(apply_pending_restore(&config).unwrap().is_empty());
}

#[test]
fn a_newer_restore_replaces_an_older_pending_one() {
    let state = state_with("open");
    let config = &state.config;
    let _cleanup = Cleanup(config.data_dir.clone());
    let scratch = config.data_dir.join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let with_db = make_zip(&[("messages.sqlite", valid_messages_db(&scratch))], None);
    let prepared = prepare(config, &with_db, &Limits::default()).unwrap();
    let (_, pending) = apply_files(config, &prepared).unwrap();
    assert_eq!(pending, vec![MESSAGES.to_string()]);
    assert!(pending_dir(config).join(MESSAGES).exists());

    let settings_only = make_zip(&[("settings.json", b"{}".to_vec())], None);
    let prepared = prepare(config, &settings_only, &Limits::default()).unwrap();
    let (_, pending) = apply_files(config, &prepared).unwrap();
    assert!(pending.is_empty());
    assert!(!restart_pending(config));
}

#[test]
fn poi_names_are_validated() {
    for ok in ["camps.geojson", "Water Points (2024).json", "routes.FGB", "åäö.geojson"] {
        assert!(is_safe_poi_name(ok), "{ok}");
    }
    for bad in ["", ".hidden.json", "a/b.json", "a\\b.json", "c:x.json", "x.txt", "x.json ", "a\nb.json", "a|b.json"] {
        assert!(!is_safe_poi_name(bad), "{bad:?}");
    }
}
