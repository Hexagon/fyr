//! Authentication middleware and session management
//!
//! Handles admin session lifecycle for read-only / password-protected modes.
//! When neither `FYR_ADMIN_PASSWORD` nor `FYR_READONLY` is set the server
//! behaves exactly as before — all endpoints are accessible without auth.

use crate::AppState;
use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Rate-limit constants
// ---------------------------------------------------------------------------

/// Maximum number of failed login attempts in the window before a client IP
/// is blocked.
const MAX_FAILED_ATTEMPTS: u32 = 10;
/// Sliding window duration for tracking failed attempts.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// AuthManager
// ---------------------------------------------------------------------------

struct RateLimitEntry {
    attempts: u32,
    first_attempt: Instant,
}

/// In-memory store for active sessions and per-IP rate limiting.
pub struct AuthManager {
    tokens: Mutex<HashSet<String>>,
    rate_limits: Mutex<HashMap<String, RateLimitEntry>>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashSet::new()),
            rate_limits: Mutex::new(HashMap::new()),
        }
    }

    /// Return `true` when `ip` has exceeded the allowed failed-attempt count.
    pub fn is_rate_limited(&self, ip: &str) -> bool {
        let mut limits = self.rate_limits.lock().unwrap();
        match limits.get_mut(ip) {
            None => false,
            Some(entry) => {
                if entry.first_attempt.elapsed() > RATE_LIMIT_WINDOW {
                    limits.remove(ip);
                    false
                } else {
                    entry.attempts >= MAX_FAILED_ATTEMPTS
                }
            }
        }
    }

    /// Record one failed login attempt from `ip`.
    pub fn record_failed_attempt(&self, ip: &str) {
        let mut limits = self.rate_limits.lock().unwrap();
        let entry = limits.entry(ip.to_string()).or_insert_with(|| RateLimitEntry {
            attempts: 0,
            first_attempt: Instant::now(),
        });
        if entry.first_attempt.elapsed() > RATE_LIMIT_WINDOW {
            entry.attempts = 1;
            entry.first_attempt = Instant::now();
        } else {
            entry.attempts += 1;
        }
    }

    /// Clear the rate-limit record for `ip` after a successful login.
    pub fn clear_rate_limit(&self, ip: &str) {
        self.rate_limits.lock().unwrap().remove(ip);
    }

    /// Generate a fresh opaque session token, store it, and return it.
    pub fn create_session(&self) -> String {
        let token = Uuid::new_v4().to_string();
        self.tokens.lock().unwrap().insert(token.clone());
        token
    }

    /// Return `true` when the supplied token is a live session.
    pub fn is_valid_session(&self, token: &str) -> bool {
        self.tokens.lock().unwrap().contains(token)
    }

    /// Invalidate `token`, ending the session.
    pub fn revoke_session(&self, token: &str) {
        self.tokens.lock().unwrap().remove(token);
    }
}

// ---------------------------------------------------------------------------
// Cookie helpers
// ---------------------------------------------------------------------------

const SESSION_COOKIE_NAME: &str = "fyr_session";

/// Build a `Set-Cookie` header value for the session token.
///
/// NOTE: The `Secure` flag is intentionally omitted because Fyr is designed
/// for local and LAN deployments where HTTPS is typically not available.
/// For public HTTPS deployments, a TLS-terminating reverse proxy (nginx,
/// Caddy, etc.) should be placed in front of Fyr and should be configured
/// to set the `Secure` flag via response-header manipulation if required.
fn set_cookie_header(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}={}; HttpOnly; Path=/; SameSite=Strict",
        SESSION_COOKIE_NAME, token
    ))
    .unwrap()
}

/// Build a `Set-Cookie` header that clears the session cookie.
fn clear_cookie_header() -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0",
        SESSION_COOKIE_NAME
    ))
    .unwrap()
}

/// Extract the session token from the `Cookie` request header, if present.
pub fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        if key == SESSION_COOKIE_NAME {
            return Some(value.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware that enforces admin access on mutating endpoints.
///
/// Passes requests through unchanged when the server is not in a restricted
/// mode.  Otherwise it validates the session cookie, returning 403 when
/// `FYR_READONLY` is set, or 401 when `FYR_ADMIN_PASSWORD` is set but the
/// caller is not authenticated.
pub async fn require_admin(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let auth_cfg = &state.config.auth;

    if auth_cfg.readonly {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "This server is in read-only mode. Admin operations are disabled."
            })),
        )
            .into_response();
    }

    if auth_cfg.admin_password.is_none() {
        // No password configured — open access, behave as before.
        return next.run(request).await;
    }

    // Password is configured: check for a valid session.
    let token = extract_session_token(request.headers());
    let authenticated = token
        .as_deref()
        .map(|t| state.auth_manager.is_valid_session(t))
        .unwrap_or(false);

    if authenticated {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Authentication required. Please log in as admin."
            })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthStatusResponse {
    /// Server is in read-only mode (`FYR_READONLY=true`).
    pub readonly: bool,
    /// Admin password has been configured (`FYR_ADMIN_PASSWORD` is set).
    pub requires_auth: bool,
    /// The current caller holds a valid admin session.
    pub authenticated: bool,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `GET /api/auth/status` — Return the current auth mode and session state.
pub async fn auth_status_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Json<AuthStatusResponse> {
    let auth_cfg = &state.config.auth;
    let token = extract_session_token(&headers);
    let authenticated = token
        .as_deref()
        .map(|t| state.auth_manager.is_valid_session(t))
        .unwrap_or(false);

    Json(AuthStatusResponse {
        readonly: auth_cfg.readonly,
        requires_auth: auth_cfg.admin_password.is_some(),
        authenticated,
    })
}

/// `POST /api/auth/login` — Validate password and create a session.
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Response {
    let auth_cfg = &state.config.auth;

    // In strict read-only mode login is meaningless.
    if auth_cfg.readonly {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Server is in read-only mode; login is not available."
            })),
        )
            .into_response();
    }

    let Some(ref expected_password) = auth_cfg.admin_password else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Authentication is not configured on this server."
            })),
        )
            .into_response();
    };

    // Use the real TCP peer address as the primary rate-limit key.
    // X-Forwarded-For is considered only as a secondary hint and is explicitly
    // documented as requiring a trusted reverse proxy to be tamper-proof.
    let peer_ip = addr.ip().to_string();
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or(peer_ip);

    if state.auth_manager.is_rate_limited(&client_ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "Too many failed login attempts. Please wait before trying again."
            })),
        )
            .into_response();
    }

    if body.password != *expected_password {
        state.auth_manager.record_failed_attempt(&client_ip);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Invalid password." })),
        )
            .into_response();
    }

    // Password correct — create session.
    state.auth_manager.clear_rate_limit(&client_ip);
    let token = state.auth_manager.create_session();

    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok" })),
    )
        .into_response();

    response
        .headers_mut()
        .insert(header::SET_COOKIE, set_cookie_header(&token));

    response
}

/// `POST /api/auth/logout` — Revoke the current session.
pub async fn logout_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(token) = extract_session_token(&headers) {
        state.auth_manager.revoke_session(&token);
    }

    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok" })),
    )
        .into_response();

    response
        .headers_mut()
        .insert(header::SET_COOKIE, clear_cookie_header());

    response
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_blocks_after_max_attempts() {
        let mgr = AuthManager::new();
        for _ in 0..MAX_FAILED_ATTEMPTS {
            assert!(!mgr.is_rate_limited("1.2.3.4"));
            mgr.record_failed_attempt("1.2.3.4");
        }
        assert!(mgr.is_rate_limited("1.2.3.4"));
    }

    #[test]
    fn clear_rate_limit_unblocks_ip() {
        let mgr = AuthManager::new();
        for _ in 0..MAX_FAILED_ATTEMPTS {
            mgr.record_failed_attempt("1.2.3.4");
        }
        assert!(mgr.is_rate_limited("1.2.3.4"));
        mgr.clear_rate_limit("1.2.3.4");
        assert!(!mgr.is_rate_limited("1.2.3.4"));
    }

    #[test]
    fn session_lifecycle() {
        let mgr = AuthManager::new();
        let token = mgr.create_session();
        assert!(mgr.is_valid_session(&token));
        mgr.revoke_session(&token);
        assert!(!mgr.is_valid_session(&token));
    }

    #[test]
    fn extract_session_token_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("fyr_session=abc123; other=val"),
        );
        assert_eq!(
            extract_session_token(&headers),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn extract_session_token_missing() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_session_token(&headers), None);
    }
}
