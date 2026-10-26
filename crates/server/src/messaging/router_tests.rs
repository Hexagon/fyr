//! Router-level tests for the messaging access matrix.

use super::{sha256, MAX_STREAMS_PER_IP};
use crate::{ai::ModelManager, auth::AuthManager, settings::SettingsManager, AppState};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::Engine;
use downloader::DownloadManager;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use types::Config;

struct Mode {
    readonly: bool,
    password: bool,
    guest: bool,
    enabled: bool,
}

const OPEN: Mode = Mode { readonly: false, password: false, guest: false, enabled: true };

fn app_with(mode: Mode) -> (Router, Option<String>) {
    let data_dir = std::env::temp_dir().join(format!("fyr-msg-router-{}", uuid::Uuid::new_v4()));
    let mut config = Config::default_with_data_dir(&data_dir);
    config.initialize_directories().expect("init dirs");
    config.auth.readonly = mode.readonly;
    config.auth.admin_password = mode.password.then(|| "secret".to_string());
    config.auth.guest_messaging = mode.guest;
    config.messaging.enabled = mode.enabled;
    let config = Arc::new(config);
    let auth_manager = Arc::new(AuthManager::new());
    let token = mode.password.then(|| auth_manager.create_session());
    let state = AppState {
        config: Arc::clone(&config),
        static_dir: data_dir.join("static"),
        download_manager: Arc::new(DownloadManager::new(&data_dir)),
        model_manager: Arc::new(ModelManager::new(Arc::clone(&config))),
        settings_manager: Arc::new(SettingsManager::new(&data_dir)),
        auth_manager,
        mbtiles_pool: Arc::new(crate::mbtiles::MbtilesPool::new()),
        messaging: Arc::new(super::Messaging::open(&config).expect("open db")),
    };
    (crate::create_router(state), token)
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = match body {
        Some(value) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn public_body(text: &str) -> Value {
    json!({ "nick": "Ada", "device_tag": "a1b2c3", "text": text, "category": "info" })
}

fn cookie(token: &Option<String>) -> String {
    format!("fyr_session={}", token.as_deref().unwrap_or(""))
}

struct Channel {
    id: String,
    key_hex: String,
}

fn channel(seed: u8) -> Channel {
    let key = [seed; 32];
    Channel { id: hex::encode([seed; 16]), key_hex: hex::encode(key) }
}

fn create_body(ch: &Channel) -> Value {
    let key = hex::decode(&ch.key_hex).unwrap();
    json!({ "channel_id": ch.id, "verifier": hex::encode(sha256(&key)) })
}

fn encrypted_body(len: usize) -> Value {
    let b64 = base64::engine::general_purpose::STANDARD;
    json!({ "nonce": b64.encode([3u8; 24]), "ciphertext": b64.encode(vec![4u8; len]) })
}

#[tokio::test]
async fn open_mode_public_channel_round_trip() {
    let (app, _) = app_with(OPEN);
    let (status, cfg) = call(&app, "GET", "/api/messages/config", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cfg["enabled"], true);
    assert_eq!(cfg["can_post"], true);
    assert_eq!(cfg["can_moderate"], true);
    assert_eq!(cfg["server_salt"].as_str().unwrap().len(), 64);

    let (status, posted) = call(&app, "POST", "/api/channels/public/messages", Some(public_body("hello")), &[]).await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    assert_eq!(posted["text"], "hello");

    let (status, list) = call(&app, "GET", "/api/channels/public/messages", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["kind"], "public");
    assert_eq!(list["messages"][0]["text"], "hello");

    let since = posted["id"].as_i64().unwrap();
    let (_, list) = call(&app, "GET", &format!("/api/channels/public/messages?since={since}"), None, &[]).await;
    assert_eq!(list["messages"].as_array().unwrap().len(), 0);

    // Ciphertext is not accepted on the public channel, and vice versa.
    let (status, _) = call(&app, "POST", "/api/channels/public/messages", Some(encrypted_body(32)), &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(&app, "GET", "/api/channels/public/messages?category=bogus", None, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn encrypted_channel_key_checks() {
    let (app, _) = app_with(OPEN);
    let ch = channel(1);
    let key = [("x-fyr-channel-key", ch.key_hex.as_str())];
    let uri = format!("/api/channels/{}/messages", ch.id);

    // Unknown channel → 404 for read, write and stream.
    assert_eq!(call(&app, "GET", &uri, None, &key).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "POST", &uri, Some(encrypted_body(32)), &key).await.0, StatusCode::NOT_FOUND);
    let stream_uri = format!("/api/channels/{}/stream", ch.id);
    assert_eq!(call(&app, "GET", &stream_uri, None, &key).await.0, StatusCode::NOT_FOUND);

    assert_eq!(call(&app, "POST", "/api/channels", Some(create_body(&ch)), &[]).await.0, StatusCode::CREATED);
    // Re-creating (even with a different verifier) never overwrites.
    let hijack = json!({ "channel_id": ch.id, "verifier": "00".repeat(32) });
    assert_eq!(call(&app, "POST", "/api/channels", Some(hijack), &[]).await.0, StatusCode::CONFLICT);
    for bad in [
        json!({ "channel_id": "public", "verifier": "00".repeat(32) }),
        json!({ "channel_id": "XYZ", "verifier": "00".repeat(32) }),
        json!({ "channel_id": hex::encode([9u8; 16]), "verifier": "short" }),
    ] {
        assert_eq!(call(&app, "POST", "/api/channels", Some(bad), &[]).await.0, StatusCode::BAD_REQUEST);
    }

    let wrong = channel(2);
    let wrong_key = [("x-fyr-channel-key", wrong.key_hex.as_str())];
    assert_eq!(call(&app, "GET", &uri, None, &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "GET", &uri, None, &[("x-fyr-channel-key", "nothex")]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "GET", &uri, None, &wrong_key).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "POST", &uri, Some(encrypted_body(32)), &wrong_key).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "POST", &uri, Some(encrypted_body(32)), &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "GET", &stream_uri, None, &wrong_key).await.0, StatusCode::FORBIDDEN);

    let (status, row) = call(&app, "POST", &uri, Some(encrypted_body(64)), &key).await;
    assert_eq!(status, StatusCode::CREATED, "{row}");
    assert!(row.get("text").is_none());
    let (status, list) = call(&app, "GET", &uri, None, &key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["kind"], "encrypted");
    assert_eq!(list["messages"][0]["ciphertext"], row["ciphertext"]);

    // Plaintext, extra fields and size violations are rejected.
    let plaintext = json!({ "nick": "a", "device_tag": "abcd", "text": "secret" });
    assert_eq!(call(&app, "POST", &uri, Some(plaintext), &key).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let mut mixed = encrypted_body(32);
    mixed["text"] = json!("secret");
    assert_eq!(call(&app, "POST", &uri, Some(mixed), &key).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(call(&app, "POST", &uri, Some(encrypted_body(8193)), &key).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(call(&app, "POST", &uri, Some(encrypted_body(8)), &key).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let huge = json!({ "nonce": "a", "ciphertext": "A".repeat(64 * 1024) });
    assert_eq!(call(&app, "POST", &uri, Some(huge), &key).await.0, StatusCode::PAYLOAD_TOO_LARGE);

    // There is no channel listing endpoint.
    let (status, _) = call(&app, "GET", "/api/channels", None, &[]).await;
    assert!(status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED, "{status}");
}

#[tokio::test]
async fn password_mode_access_matrix() {
    let (app, token) = app_with(Mode { readonly: false, password: true, guest: false, enabled: true });
    let admin = cookie(&token);
    let admin_h = [("cookie", admin.as_str())];

    let (_, cfg) = call(&app, "GET", "/api/messages/config", None, &[]).await;
    assert_eq!(cfg["can_post"], false);
    assert_eq!(cfg["can_moderate"], false);
    let (_, cfg) = call(&app, "GET", "/api/messages/config", None, &admin_h).await;
    assert_eq!(cfg["can_post"], true);
    assert_eq!(cfg["can_moderate"], true);

    // Guests can read but not write.
    assert_eq!(call(&app, "GET", "/api/channels/public/messages", None, &[]).await.0, StatusCode::OK);
    assert_eq!(
        call(&app, "POST", "/api/channels/public/messages", Some(public_body("x")), &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(call(&app, "POST", "/api/channels", Some(create_body(&channel(1))), &[]).await.0, StatusCode::UNAUTHORIZED);

    // Admin can write and moderate.
    let (status, posted) = call(&app, "POST", "/api/channels/public/messages", Some(public_body("x")), &admin_h).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = posted["id"].as_i64().unwrap();
    let pin_uri = format!("/api/channels/public/messages/{id}/pin");
    let del_uri = format!("/api/channels/public/messages/{id}");
    assert_eq!(call(&app, "POST", &pin_uri, Some(json!({ "pinned": true })), &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "DELETE", &del_uri, None, &[]).await.0, StatusCode::UNAUTHORIZED);
    let (status, pinned) = call(&app, "POST", &pin_uri, Some(json!({ "pinned": true })), &admin_h).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pinned["pinned"], true);
    let (_, list) = call(&app, "GET", "/api/channels/public/messages", None, &[]).await;
    assert_eq!(list["pinned"][0]["id"], id);
    assert_eq!(call(&app, "DELETE", &del_uri, None, &admin_h).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "DELETE", &del_uri, None, &admin_h).await.0, StatusCode::NOT_FOUND);

    let ch = channel(5);
    assert_eq!(call(&app, "POST", "/api/channels", Some(create_body(&ch)), &admin_h).await.0, StatusCode::CREATED);
    let ch_uri = format!("/api/channels/{}", ch.id);
    assert_eq!(call(&app, "DELETE", &ch_uri, None, &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "DELETE", &ch_uri, None, &admin_h).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "DELETE", "/api/channels/public", None, &admin_h).await.0, StatusCode::BAD_REQUEST);
    let key = [("x-fyr-channel-key", ch.key_hex.as_str())];
    assert_eq!(
        call(&app, "GET", &format!("/api/channels/{}/messages", ch.id), None, &key).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn guest_messaging_allows_guest_writes_but_not_moderation() {
    let (app, _) = app_with(Mode { readonly: false, password: true, guest: true, enabled: true });
    let (_, cfg) = call(&app, "GET", "/api/messages/config", None, &[]).await;
    assert_eq!(cfg["can_post"], true);
    assert_eq!(cfg["can_moderate"], false);
    assert_eq!(cfg["guest_messaging"], true);
    let (status, posted) = call(&app, "POST", "/api/channels/public/messages", Some(public_body("hi")), &[]).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(call(&app, "POST", "/api/channels", Some(create_body(&channel(1))), &[]).await.0, StatusCode::CREATED);
    let del_uri = format!("/api/channels/public/messages/{}", posted["id"]);
    assert_eq!(call(&app, "DELETE", &del_uri, None, &[]).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn readonly_mode_is_absolute() {
    let (app, token) = app_with(Mode { readonly: true, password: true, guest: true, enabled: true });
    let admin = cookie(&token);
    let admin_h = [("cookie", admin.as_str())];
    let (_, cfg) = call(&app, "GET", "/api/messages/config", None, &admin_h).await;
    assert_eq!(cfg["can_post"], false);
    assert_eq!(cfg["can_moderate"], false);
    assert_eq!(call(&app, "GET", "/api/channels/public/messages", None, &[]).await.0, StatusCode::OK);
    for headers in [&[][..], &admin_h[..]] {
        assert_eq!(
            call(&app, "POST", "/api/channels/public/messages", Some(public_body("x")), headers).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(call(&app, "POST", "/api/channels", Some(create_body(&channel(1))), headers).await.0, StatusCode::FORBIDDEN);
        assert_eq!(call(&app, "DELETE", "/api/channels/public/messages/1", None, headers).await.0, StatusCode::FORBIDDEN);
        assert_eq!(
            call(&app, "POST", "/api/channels/public/messages/1/pin", Some(json!({ "pinned": true })), headers).await.0,
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn disabled_messaging_exposes_only_config() {
    let (app, _) = app_with(Mode { readonly: false, password: false, guest: false, enabled: false });
    let (status, cfg) = call(&app, "GET", "/api/messages/config", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cfg["enabled"], false);
    assert_eq!(cfg["can_post"], false);
    assert_eq!(call(&app, "GET", "/api/channels/public/messages", None, &[]).await.0, StatusCode::NOT_FOUND);
    // Unrouted POSTs hit the SPA fallback (GET only) → 405.
    let status = call(&app, "POST", "/api/channels/public/messages", Some(public_body("x")), &[]).await.0;
    assert!(status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED, "{status}");
}

#[tokio::test]
async fn post_rate_limit_applies_per_ip() {
    let (app, _) = app_with(OPEN);
    let mut statuses = Vec::new();
    for i in 0..12 {
        let body = public_body(&format!("spam {i}"));
        statuses.push(call(&app, "POST", "/api/channels/public/messages", Some(body), &[]).await.0);
    }
    assert!(statuses[..8].iter().all(|s| *s == StatusCode::CREATED), "{statuses:?}");
    assert!(statuses[8..].iter().all(|s| *s == StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
}

#[tokio::test]
async fn sse_stream_sends_ready_then_events_and_is_capped() {
    use tokio_stream::StreamExt;
    let (app, _) = app_with(OPEN);
    let response = app
        .clone()
        .oneshot(Request::get("/api/channels/public/stream").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/event-stream");
    let mut body = response.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first).contains("event: ready"));

    call(&app, "POST", "/api/channels/public/messages", Some(public_body("live")), &[]).await;
    let next = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
        .await
        .expect("event in time")
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&next).to_string();
    assert!(text.contains("event: message") && text.contains("\"text\":\"live\""), "{text}");

    // Per-IP concurrency cap (this test already holds one stream).
    let mut held = Vec::new();
    for _ in 1..MAX_STREAMS_PER_IP {
        let r = app
            .clone()
            .oneshot(Request::get("/api/channels/public/stream").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        held.push(r);
    }
    let over = app
        .clone()
        .oneshot(Request::get("/api/channels/public/stream").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(over.status(), StatusCode::TOO_MANY_REQUESTS);
}
