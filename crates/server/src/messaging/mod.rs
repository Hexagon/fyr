//! Messages: one built-in public channel ("board") plus end-to-end encrypted
//! passphrase channels, sharing one message model, one API shape and one
//! SSE stream format.
//!
//! * The public channel (id `public`) stores plaintext messages that anyone
//!   can read. Messages may carry an optional category (`info|need|offer|
//!   alert`), an optional location and an expiry; admins can pin/delete.
//! * Encrypted channels are identified by a 128-bit id derived client-side
//!   from (server salt, channel name, passphrase). The server stores only
//!   `sha256(auth_key)` as a verifier plus opaque `nonce`/`ciphertext`
//!   blobs. It never receives the passphrase, the encryption key or any
//!   plaintext. See `crates/ui/frontend/src/modules/messaging/crypto.js`.
//!
//! Access control lives in [`crate::auth::require_messaging_write`] (writes)
//! and the admin `protected` router (moderation); see AGENTS.md.

pub mod limits;
#[cfg(test)]
mod router_tests;
pub mod routes;
pub mod store;

use anyhow::Context;
use limits::{RateLimiter, StreamLimiter};
use ring::digest;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use store::{NewPublicMessage, RetentionLimits, Store};
use tokio::sync::broadcast;
use tracing::{info, warn};
use types::config::MessagingConfig;

pub use store::PUBLIC_CHANNEL_ID;

pub const MAX_TEXT_CHARS: usize = 2000;
pub const MAX_NICK_CHARS: usize = 40;
pub const MAX_CIPHERTEXT_BYTES: usize = 8192;
/// XChaCha20-Poly1305 nonce length.
pub const NONCE_BYTES: usize = 24;
/// Poly1305 tag length: the smallest possible ciphertext.
pub const TAG_BYTES: usize = 16;
pub const DEFAULT_TTL_DAYS: u32 = 30;
pub const MAX_TTL_DAYS: u32 = 365;
pub const MAX_CHANNELS: u32 = 5000;
pub const MAX_TOTAL_MESSAGES: u32 = 200_000;
pub const DEFAULT_PAGE: u32 = 100;
pub const MAX_PAGE: u32 = 500;
/// Request body cap for messaging write endpoints.
pub const MAX_BODY_BYTES: usize = 32 * 1024;
pub const MAX_STREAMS_TOTAL: usize = 200;
pub const MAX_STREAMS_PER_IP: usize = 8;
/// Streams are closed after this long; clients reconnect and catch up.
pub const STREAM_MAX_LIFETIME: Duration = Duration::from_secs(30 * 60);
pub const PRUNE_INTERVAL: Duration = Duration::from_secs(10 * 60);
pub const CATEGORIES: [&str; 4] = ["info", "need", "offer", "alert"];

const DAY_MS: i64 = 86_400_000;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One event fanned out to SSE subscribers of `channel_id`.
#[derive(Debug, Clone)]
pub struct ChannelEvent {
    pub channel_id: String,
    /// `message`, `deleted`, `pinned` or `channel_deleted`.
    pub kind: &'static str,
    pub data: String,
}

pub struct Messaging {
    store: Mutex<Store>,
    server_salt: [u8; 32],
    config: MessagingConfig,
    events: broadcast::Sender<Arc<ChannelEvent>>,
    pub post_limiter: RateLimiter,
    pub channel_limiter: RateLimiter,
    pub streams: StreamLimiter,
}

impl Messaging {
    /// Open `DATA_DIR/messages.sqlite`, migrating and seeding the server
    /// salt on first start. Blocking; call once at startup.
    pub fn open(config: &types::Config) -> anyhow::Result<Self> {
        let path = config.messages_db_path();
        let store = Store::open(&path)
            .with_context(|| format!("Failed to open messages database {}", path.display()))?;
        Self::with_store(store, config.messaging.clone())
    }

    pub fn with_store(store: Store, config: MessagingConfig) -> anyhow::Result<Self> {
        let server_salt = store
            .server_salt(random_salt)
            .context("Failed to load or create the messaging server salt")?;
        let (events, _) = broadcast::channel(512);
        Ok(Self {
            store: Mutex::new(store),
            server_salt,
            config,
            events,
            // 8 message burst, then one every 4 s (15/min) per IP.
            post_limiter: RateLimiter::new(8, Duration::from_secs(4)),
            // 3 channel creations burst, then one per minute per IP.
            channel_limiter: RateLimiter::new(3, Duration::from_secs(60)),
            streams: StreamLimiter::new(MAX_STREAMS_TOTAL, MAX_STREAMS_PER_IP),
        })
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn config(&self) -> &MessagingConfig {
        &self.config
    }

    pub fn server_salt_hex(&self) -> String {
        hex::encode(self.server_salt)
    }

    /// Run `work` against the store on the current (blocking) thread.
    pub fn with_store_blocking<T>(&self, work: impl FnOnce(&Store) -> T) -> T {
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        work(&store)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<ChannelEvent>> {
        self.events.subscribe()
    }

    pub fn publish(&self, channel_id: &str, kind: &'static str, data: String) {
        // No receivers is not an error.
        let _ = self.events.send(Arc::new(ChannelEvent {
            channel_id: channel_id.to_string(),
            kind,
            data,
        }));
    }

    pub fn retention(&self) -> RetentionLimits {
        RetentionLimits {
            max_per_channel: self.config.max_messages_per_channel,
            max_total_messages: MAX_TOTAL_MESSAGES,
            idle_channel_ms: i64::from(self.config.max_age_days) * DAY_MS,
        }
    }

    pub fn encrypted_expiry(&self, now: i64) -> i64 {
        now + i64::from(self.config.max_age_days) * DAY_MS
    }

    /// Periodic retention task. Spawned from `main`.
    pub fn spawn_pruner(self: &Arc<Self>) {
        let messaging = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(PRUNE_INTERVAL);
            loop {
                interval.tick().await;
                let worker = Arc::clone(&messaging);
                let result = tokio::task::spawn_blocking(move || {
                    worker.post_limiter.prune();
                    worker.channel_limiter.prune();
                    let limits = worker.retention();
                    worker.with_store_blocking(|store| store.prune(now_ms(), limits))
                })
                .await;
                match result {
                    Ok(Ok(stats)) if stats != Default::default() => info!(
                        "Messages retention: {} expired, {} over channel cap, {} over total cap, {} idle channels removed",
                        stats.expired, stats.over_channel_cap, stats.over_total_cap, stats.idle_channels
                    ),
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => warn!("Messages retention failed: {error}"),
                    Err(error) => warn!("Messages retention task panicked: {error}"),
                }
            }
        });
    }
}

fn random_salt() -> [u8; 32] {
    use ring::rand::SecureRandom;
    let mut salt = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut salt)
        .expect("operating system RNG unavailable");
    salt
}

// ---------------------------------------------------------------------------
// Validation (pure functions, unit tested)
// ---------------------------------------------------------------------------

/// `public`, or a 128-bit channel id as 32 lowercase hex characters.
pub fn is_valid_channel_id(id: &str) -> bool {
    id == PUBLIC_CHANNEL_ID || is_lower_hex(id, 32)
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Parse a 32-byte key given as 64 hex characters (either case).
pub fn parse_key_hex(value: &str) -> Option<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return None;
    }
    let bytes = hex::decode(value).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = digest::digest(&digest::SHA256, data);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Check a presented channel auth key against the stored verifier
/// (`sha256(auth_key)`) in constant time.
pub fn verify_channel_key(verifier: &[u8], auth_key: &[u8; 32]) -> bool {
    crate::auth::constant_time_eq(&sha256(auth_key), verifier)
}

/// Body of `POST /api/channels`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateChannelIn {
    pub channel_id: String,
    /// hex(sha256(auth_key))
    pub verifier: String,
}

/// Body of `POST /api/channels/public/messages` (plaintext).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicMessageIn {
    pub nick: String,
    pub device_tag: String,
    pub text: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lng: Option<f64>,
    #[serde(default)]
    pub ttl_days: Option<u32>,
}

/// Body of `POST /api/channels/:id/messages` for encrypted channels.
/// Unknown fields (for example a stray plaintext `text`) are rejected.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedMessageIn {
    /// base64, 24 bytes
    pub nonce: String,
    /// base64, 16..=8192 bytes
    pub ciphertext: String,
}

/// Device tags are short random hex ids used only to colour messages.
pub fn is_valid_device_tag(tag: &str) -> bool {
    (4..=16).contains(&tag.len()) && tag.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn has_forbidden_controls(value: &str, allow_newlines: bool) -> bool {
    value.chars().any(|c| {
        if allow_newlines && (c == '\n' || c == '\t') {
            return false;
        }
        c.is_control()
            // Bidi overrides can disguise text; zero-width chars are kept.
            || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
    })
}

pub fn validate_public_message(input: PublicMessageIn, now: i64) -> Result<NewPublicMessage, String> {
    let nick = input.nick.trim().to_string();
    if nick.is_empty() || nick.chars().count() > MAX_NICK_CHARS {
        return Err(format!("nick must be 1-{MAX_NICK_CHARS} characters"));
    }
    if has_forbidden_controls(&nick, false) {
        return Err("nick contains control characters".into());
    }
    if !is_valid_device_tag(&input.device_tag) {
        return Err("device_tag must be 4-16 lowercase hex characters".into());
    }
    let text = input.text.replace("\r\n", "\n").trim().to_string();
    if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!("text must be 1-{MAX_TEXT_CHARS} characters"));
    }
    if has_forbidden_controls(&text, true) {
        return Err("text contains control characters".into());
    }
    let category = match input.category.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(c) if CATEGORIES.contains(&c) => Some(c.to_string()),
        Some(_) => return Err(format!("category must be one of {}", CATEGORIES.join(", "))),
    };
    let location = match (input.lat, input.lng) {
        (None, None) => None,
        (Some(lat), Some(lng))
            if lat.is_finite() && lng.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lng) =>
        {
            Some((lat, lng))
        }
        _ => return Err("lat and lng must both be given and in range".into()),
    };
    let ttl = input.ttl_days.unwrap_or(DEFAULT_TTL_DAYS);
    if !(1..=MAX_TTL_DAYS).contains(&ttl) {
        return Err(format!("ttl_days must be 1-{MAX_TTL_DAYS}"));
    }
    Ok(NewPublicMessage {
        nick,
        device_tag: input.device_tag,
        category,
        text,
        location,
        expires_at: now + i64::from(ttl) * DAY_MS,
    })
}

/// Decode and size-check an encrypted message. Returns (nonce, ciphertext).
pub fn validate_encrypted_message(input: &EncryptedMessageIn) -> Result<(Vec<u8>, Vec<u8>), String> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    // Reject oversize input before decoding.
    if input.ciphertext.len() > MAX_CIPHERTEXT_BYTES.div_ceil(3) * 4 || input.nonce.len() > 64 {
        return Err(format!("ciphertext must be at most {MAX_CIPHERTEXT_BYTES} bytes"));
    }
    let nonce = engine
        .decode(input.nonce.as_bytes())
        .map_err(|_| "nonce must be base64".to_string())?;
    if nonce.len() != NONCE_BYTES {
        return Err(format!("nonce must be {NONCE_BYTES} bytes"));
    }
    let ciphertext = engine
        .decode(input.ciphertext.as_bytes())
        .map_err(|_| "ciphertext must be base64".to_string())?;
    if ciphertext.len() < TAG_BYTES || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(format!("ciphertext must be {TAG_BYTES}-{MAX_CIPHERTEXT_BYTES} bytes"));
    }
    Ok((nonce, ciphertext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn public_in(text: &str) -> PublicMessageIn {
        PublicMessageIn {
            nick: "Ada".into(),
            device_tag: "a1b2c3".into(),
            text: text.into(),
            category: None,
            lat: None,
            lng: None,
            ttl_days: None,
        }
    }

    #[test]
    fn channel_ids() {
        assert!(is_valid_channel_id("public"));
        assert!(is_valid_channel_id(&"ab".repeat(16)));
        assert!(!is_valid_channel_id(&"AB".repeat(16)));
        assert!(!is_valid_channel_id(&"ab".repeat(15)));
        assert!(!is_valid_channel_id("../etc/passwd"));
        assert!(!is_valid_channel_id(""));
    }

    #[test]
    fn key_verification_is_exact() {
        let key = [7u8; 32];
        let verifier = sha256(&key);
        assert!(verify_channel_key(&verifier, &key));
        let mut other = key;
        other[31] ^= 1;
        assert!(!verify_channel_key(&verifier, &other));
        assert!(!verify_channel_key(&verifier[..31], &key));
        assert!(!verify_channel_key(&[], &key));
        assert_eq!(parse_key_hex(&hex::encode(key)), Some(key));
        assert_eq!(parse_key_hex(&hex::encode(key).to_uppercase()), Some(key));
        assert_eq!(parse_key_hex("zz"), None);
        assert_eq!(parse_key_hex(&"g".repeat(64)), None);
    }

    #[test]
    fn public_message_validation() {
        let now = 1_000;
        let ok = validate_public_message(public_in("  hello\r\nworld  "), now).unwrap();
        assert_eq!(ok.text, "hello\nworld");
        assert_eq!(ok.expires_at, now + 30 * DAY_MS);

        assert!(validate_public_message(public_in(""), now).is_err());
        assert!(validate_public_message(public_in(&"x".repeat(2001)), now).is_err());
        assert!(validate_public_message(public_in(&"ä".repeat(2000)), now).is_ok());
        assert!(validate_public_message(public_in("bell\u{7}"), now).is_err());
        assert!(validate_public_message(public_in("rtl\u{202E}txt"), now).is_err());

        let mut long_nick = public_in("x");
        long_nick.nick = "n".repeat(41);
        assert!(validate_public_message(long_nick, now).is_err());

        let mut bad_tag = public_in("x");
        bad_tag.device_tag = "<script>".into();
        assert!(validate_public_message(bad_tag, now).is_err());

        let mut cat = public_in("x");
        cat.category = Some("alert".into());
        assert_eq!(validate_public_message(cat, now).unwrap().category.as_deref(), Some("alert"));
        let mut cat = public_in("x");
        cat.category = Some("spam".into());
        assert!(validate_public_message(cat, now).is_err());

        let mut loc = public_in("x");
        loc.lat = Some(59.3);
        assert!(validate_public_message(loc, now).is_err(), "lat without lng");
        let mut loc = public_in("x");
        loc.lat = Some(91.0);
        loc.lng = Some(18.0);
        assert!(validate_public_message(loc, now).is_err());
        let mut loc = public_in("x");
        loc.lat = Some(59.3);
        loc.lng = Some(18.0);
        assert_eq!(validate_public_message(loc, now).unwrap().location, Some((59.3, 18.0)));

        let mut ttl = public_in("x");
        ttl.ttl_days = Some(366);
        assert!(validate_public_message(ttl, now).is_err());
        let mut ttl = public_in("x");
        ttl.ttl_days = Some(0);
        assert!(validate_public_message(ttl, now).is_err());
    }

    #[test]
    fn encrypted_message_validation() {
        let b64 = |n: usize| base64::engine::general_purpose::STANDARD.encode(vec![1u8; n]);
        let ok = EncryptedMessageIn { nonce: b64(24), ciphertext: b64(100) };
        assert!(validate_encrypted_message(&ok).is_ok());
        let max = EncryptedMessageIn { nonce: b64(24), ciphertext: b64(MAX_CIPHERTEXT_BYTES) };
        assert!(validate_encrypted_message(&max).is_ok());
        for bad in [
            EncryptedMessageIn { nonce: b64(12), ciphertext: b64(100) },
            EncryptedMessageIn { nonce: b64(24), ciphertext: b64(15) },
            EncryptedMessageIn { nonce: b64(24), ciphertext: b64(MAX_CIPHERTEXT_BYTES + 1) },
            EncryptedMessageIn { nonce: b64(24), ciphertext: "hello, this is plaintext!".into() },
            EncryptedMessageIn { nonce: "not base64!".into(), ciphertext: b64(100) },
        ] {
            assert!(validate_encrypted_message(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn plaintext_fields_are_rejected_for_encrypted_messages() {
        let body = serde_json::json!({ "nonce": "x", "ciphertext": "y", "text": "secret" });
        assert!(serde_json::from_value::<EncryptedMessageIn>(body).is_err());
        let body = serde_json::json!({ "text": "secret", "nick": "a", "device_tag": "abcd" });
        assert!(serde_json::from_value::<EncryptedMessageIn>(body).is_err());
    }
}
