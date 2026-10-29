//! HTTP handlers and routers for messaging.
//!
//! Route groups (mounted from `main.rs`):
//! * [`public_routes`]: reads and SSE streams (encrypted channels require
//!   the `X-Fyr-Channel-Key` header).
//! * [`write_routes`]: posting and channel creation, behind
//!   [`crate::auth::require_messaging_write`].
//! * [`moderation_routes`]: admin-only, merged into the `protected` router.

use super::store::{ChannelKind, ChannelRecord, CreateChannel, ListQuery};
use super::{
    is_valid_channel_id, now_ms, parse_key_hex, validate_encrypted_message,
    validate_public_message, verify_channel_key, CreateChannelIn, EncryptedMessageIn, Messaging,
    PublicMessageIn, DEFAULT_PAGE, DEFAULT_TTL_DAYS, MAX_CIPHERTEXT_BYTES, MAX_NICK_CHARS,
    MAX_PAGE, MAX_TEXT_CHARS, MAX_TTL_DAYS, PUBLIC_CHANNEL_ID, STREAM_MAX_LIFETIME,
};
use crate::auth;
use crate::AppState;
use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequestParts, Path, Query, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;

/// Header carrying the hex-encoded channel auth key.
pub const CHANNEL_KEY_HEADER: &str = "x-fyr-channel-key";

type Shared = Arc<AppState>;

pub fn public_routes() -> Router<Shared> {
    Router::new()
        .route("/api/messages/config", get(config_handler))
        .route("/api/channels/:id/messages", get(list_messages))
        .route("/api/channels/:id/stream", get(stream_messages))
}

pub fn write_routes() -> Router<Shared> {
    Router::new()
        .route("/api/channels", post(create_channel))
        .route("/api/channels/:id/messages", post(post_message))
}

pub fn moderation_routes() -> Router<Shared> {
    Router::new()
        .route("/api/channels/:id", delete(delete_channel))
        .route("/api/channels/:id/messages/:msg_id", delete(delete_message))
        .route("/api/channels/:id/messages/:msg_id/pin", post(pin_message))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

fn too_many(retry_after: Duration, message: &str) -> Response {
    let mut response = error(StatusCode::TOO_MANY_REQUESTS, message);
    if let Ok(value) = HeaderValue::from_str(&retry_after.as_secs().max(1).to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// Client address for rate limits and stream caps: the real TCP peer, or
/// the forwarded client when the peer is a trusted reverse proxy
/// (`FYR_TRUSTED_PROXIES`; see `auth::client_ip`). Falls back to 0.0.0.0
/// when connect info is unavailable (tests).
pub struct ClientIp(pub IpAddr);

#[async_trait]
impl FromRequestParts<Shared> for ClientIp {
    type Rejection = Infallible;
    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        Ok(ClientIp(crate::auth::client_ip(state, peer, &parts.headers)))
    }
}

async fn blocking<T: Send + 'static>(
    messaging: &Arc<Messaging>,
    work: impl FnOnce(&Messaging) -> T + Send + 'static,
) -> Result<T, Response> {
    let messaging = Arc::clone(messaging);
    tokio::task::spawn_blocking(move || work(&messaging))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
}

fn db_error(err: rusqlite::Error) -> Response {
    tracing::error!("Messages database error: {err}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "Messages database error")
}

/// Resolve a channel and check read access: the public channel is open,
/// encrypted channels need a matching `X-Fyr-Channel-Key`.
///
/// Order: malformed id → 400, unknown → 404 (UI offers "create"),
/// missing/malformed key → 401, wrong key → 403.
async fn authorize_channel(
    messaging: &Arc<Messaging>,
    id: &str,
    headers: &HeaderMap,
) -> Result<ChannelRecord, Response> {
    if !is_valid_channel_id(id) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid channel id"));
    }
    let lookup = id.to_string();
    let record = blocking(messaging, move |m| m.with_store_blocking(|s| s.channel(&lookup)))
        .await?
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Unknown channel"))?;
    if record.kind == ChannelKind::Public {
        return Ok(record);
    }
    let key = headers
        .get(CHANNEL_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_key_hex)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Channel key required"))?;
    let verifier = record.verifier.as_deref().unwrap_or(&[]);
    if verify_channel_key(verifier, &key) {
        Ok(record)
    } else {
        Err(error(StatusCode::FORBIDDEN, "Wrong channel key"))
    }
}

fn messaging(state: &AppState) -> &Arc<Messaging> {
    &state.messaging
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/messages/config`
pub async fn config_handler(State(state): State<Shared>, headers: HeaderMap) -> Response {
    let m = messaging(&state);
    let enabled = m.enabled();
    let can_post = enabled && auth::messaging_write_access(&state, &headers).is_ok();
    let cfg = m.config();
    Json(serde_json::json!({
        "enabled": enabled,
        "can_post": can_post,
        "can_moderate": enabled && auth::has_admin_access(&state, &headers),
        "guest_messaging": state.config.auth.guest_messaging,
        "server_salt": if enabled { Some(m.server_salt_hex()) } else { None },
        "public_channel_id": PUBLIC_CHANNEL_ID,
        "limits": {
            "max_text_chars": MAX_TEXT_CHARS,
            "max_nick_chars": MAX_NICK_CHARS,
            "max_ciphertext_bytes": MAX_CIPHERTEXT_BYTES,
            "default_ttl_days": DEFAULT_TTL_DAYS,
            "max_ttl_days": MAX_TTL_DAYS,
            "max_messages_per_channel": cfg.max_messages_per_channel,
            "max_age_days": cfg.max_age_days,
        },
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    since: Option<i64>,
    before: Option<i64>,
    limit: Option<u32>,
    category: Option<String>,
}

/// `GET /api/channels/:id/messages?since=&before=&limit=&category=`
pub async fn list_messages(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Query(params): Query<ListParams>,
    headers: HeaderMap,
) -> Response {
    let m = messaging(&state);
    let record = match authorize_channel(m, &id, &headers).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    if let Some(category) = params.category.as_deref() {
        if !super::CATEGORIES.contains(&category) {
            return error(StatusCode::BAD_REQUEST, "Unknown category");
        }
    }
    let limit = params.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let include_pinned = record.kind == ChannelKind::Public && params.since.is_none() && params.before.is_none();
    let result = blocking(m, move |m| {
        m.with_store_blocking(|store| {
            let query = ListQuery {
                since: params.since,
                before: params.before,
                limit,
                category: params.category.as_deref(),
            };
            let (messages, has_more) = store.list(&id, query, now_ms())?;
            let pinned = if include_pinned { store.pinned(&id)? } else { Vec::new() };
            Ok::<_, rusqlite::Error>((id, messages, has_more, pinned))
        })
    })
    .await;
    match result {
        Ok(Ok((id, messages, has_more, pinned))) => Json(serde_json::json!({
            "channel_id": id,
            "kind": record.kind,
            "messages": messages,
            "pinned": pinned,
            "has_more": has_more,
        }))
        .into_response(),
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}

/// `POST /api/channels` — register an encrypted channel.
pub async fn create_channel(
    State(state): State<Shared>,
    ClientIp(ip): ClientIp,
    Json(body): Json<CreateChannelIn>,
) -> Response {
    let m = messaging(&state);
    if body.channel_id == PUBLIC_CHANNEL_ID || !is_valid_channel_id(&body.channel_id) {
        return error(StatusCode::BAD_REQUEST, "channel_id must be 32 lowercase hex characters");
    }
    let Some(verifier) = parse_key_hex(&body.verifier) else {
        return error(StatusCode::BAD_REQUEST, "verifier must be 64 hex characters");
    };
    if let Err(retry) = m.channel_limiter.check(ip) {
        return too_many(retry, "Too many channels created. Please wait.");
    }
    let id = body.channel_id.clone();
    let result = blocking(m, move |m| {
        m.with_store_blocking(|s| s.create_channel(&id, &verifier, now_ms(), super::MAX_CHANNELS))
    })
    .await;
    match result {
        Ok(Ok(CreateChannel::Created)) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "channel_id": body.channel_id })),
        )
            .into_response(),
        Ok(Ok(CreateChannel::AlreadyExists)) => error(StatusCode::CONFLICT, "Channel already exists"),
        Ok(Ok(CreateChannel::TooManyChannels)) => {
            error(StatusCode::INSUFFICIENT_STORAGE, "Channel limit reached on this server")
        }
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}

/// `POST /api/channels/:id/messages` — plaintext body for `public`,
/// `{nonce, ciphertext}` for encrypted channels.
pub async fn post_message(
    State(state): State<Shared>,
    Path(id): Path<String>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let m = messaging(&state);
    let record = match authorize_channel(m, &id, &headers).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    let now = now_ms();
    enum Payload {
        Public(super::store::NewPublicMessage),
        Encrypted(Vec<u8>, Vec<u8>),
    }
    let payload = match record.kind {
        ChannelKind::Public => {
            let input: PublicMessageIn = match serde_json::from_value(body) {
                Ok(input) => input,
                Err(err) => return error(StatusCode::UNPROCESSABLE_ENTITY, err.to_string()),
            };
            match validate_public_message(input, now) {
                Ok(valid) => Payload::Public(valid),
                Err(message) => return error(StatusCode::UNPROCESSABLE_ENTITY, message),
            }
        }
        ChannelKind::Encrypted => {
            let input: EncryptedMessageIn = match serde_json::from_value(body) {
                Ok(input) => input,
                Err(_) => {
                    return error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Encrypted channels accept only {nonce, ciphertext}",
                    )
                }
            };
            match validate_encrypted_message(&input) {
                Ok((nonce, ciphertext)) => Payload::Encrypted(nonce, ciphertext),
                Err(message) => return error(StatusCode::UNPROCESSABLE_ENTITY, message),
            }
        }
    };
    if let Err(retry) = m.post_limiter.check(ip) {
        return too_many(retry, "You are posting too fast. Please wait.");
    }
    let channel_id = id.clone();
    let result = blocking(m, move |m| {
        let cap = m.config().max_messages_per_channel;
        let expiry = m.encrypted_expiry(now);
        m.with_store_blocking(|store| match payload {
            Payload::Public(message) => store.insert_public(&message, now, cap),
            Payload::Encrypted(nonce, ct) => store.insert_encrypted(&channel_id, &nonce, &ct, now, expiry, cap),
        })
    })
    .await;
    match result {
        Ok(Ok(row)) => {
            let json = serde_json::to_string(&row).unwrap_or_default();
            m.publish(&id, "message", json);
            (StatusCode::CREATED, Json(row)).into_response()
        }
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}

/// `GET /api/channels/:id/stream` — Server-Sent Events.
///
/// Emits `ready` first (clients then catch up with `?since=`), followed by
/// `message`, `deleted`, `pinned`, `resync` (events were dropped: catch up)
/// and `channel_deleted`. Streams are capped globally and per IP and are
/// closed after [`STREAM_MAX_LIFETIME`].
pub async fn stream_messages(
    State(state): State<Shared>,
    Path(id): Path<String>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
) -> Response {
    let m = messaging(&state);
    if let Err(response) = authorize_channel(m, &id, &headers).await {
        return response;
    }
    let Some(permit) = m.streams.acquire(ip) else {
        return too_many(Duration::from_secs(10), "Too many open message streams");
    };
    let mut events = m.subscribe();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);
    tokio::spawn(async move {
        let _permit = permit;
        if tx.send(Ok(Event::default().event("ready").data("{}"))).await.is_err() {
            return;
        }
        let deadline = tokio::time::sleep(STREAM_MAX_LIFETIME);
        tokio::pin!(deadline);
        loop {
            let event = tokio::select! {
                _ = tx.closed() => break,
                _ = &mut deadline => break,
                received = events.recv() => received,
            };
            let (outgoing, last) = match event {
                Ok(event) if event.channel_id == id => (
                    Event::default().event(event.kind).data(event.data.clone()),
                    event.kind == "channel_deleted",
                ),
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => (Event::default().event("resync").data("{}"), false),
                Err(RecvError::Closed) => break,
            };
            if tx.send(Ok(outgoing)).await.is_err() || last {
                break;
            }
        }
    });
    let mut response = Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
        .into_response();
    // Disable proxy buffering (nginx) so events arrive immediately.
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

/// `DELETE /api/channels/:id/messages/:msg_id` (admin)
pub async fn delete_message(
    State(state): State<Shared>,
    Path((id, msg_id)): Path<(String, i64)>,
) -> Response {
    let m = messaging(&state);
    if !is_valid_channel_id(&id) {
        return error(StatusCode::BAD_REQUEST, "Invalid channel id");
    }
    let channel = id.clone();
    match blocking(m, move |m| m.with_store_blocking(|s| s.delete_message(&channel, msg_id))).await {
        Ok(Ok(true)) => {
            m.publish(&id, "deleted", serde_json::json!({ "id": msg_id }).to_string());
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Ok(false)) => error(StatusCode::NOT_FOUND, "Unknown message"),
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinIn {
    pinned: bool,
}

/// `POST /api/channels/public/messages/:msg_id/pin` `{pinned}` (admin;
/// public channel only).
pub async fn pin_message(
    State(state): State<Shared>,
    Path((id, msg_id)): Path<(String, i64)>,
    Json(body): Json<PinIn>,
) -> Response {
    let m = messaging(&state);
    if id != PUBLIC_CHANNEL_ID {
        return error(StatusCode::BAD_REQUEST, "Only public channel messages can be pinned");
    }
    let pinned = body.pinned;
    match blocking(m, move |m| {
        m.with_store_blocking(|s| s.set_pinned(PUBLIC_CHANNEL_ID, msg_id, pinned, now_ms()))
    })
    .await
    {
        Ok(Ok(Some(row))) => {
            m.publish(&id, "pinned", serde_json::to_string(&row).unwrap_or_default());
            Json(row).into_response()
        }
        Ok(Ok(None)) => error(StatusCode::NOT_FOUND, "Unknown message"),
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}

/// `DELETE /api/channels/:id` (admin; encrypted channels only). Removes the
/// channel and all its ciphertext.
pub async fn delete_channel(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    let m = messaging(&state);
    if id == PUBLIC_CHANNEL_ID || !is_valid_channel_id(&id) {
        return error(StatusCode::BAD_REQUEST, "Only encrypted channels can be deleted");
    }
    let channel = id.clone();
    match blocking(m, move |m| m.with_store_blocking(|s| s.delete_channel(&channel))).await {
        Ok(Ok(true)) => {
            m.publish(&id, "channel_deleted", "{}".to_string());
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Ok(false)) => error(StatusCode::NOT_FOUND, "Unknown channel"),
        Ok(Err(err)) => db_error(err),
        Err(response) => response,
    }
}
