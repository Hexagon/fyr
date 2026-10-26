//! Built-in HTTPS: certificate loading, self-signed certificate management
//! and the optional HTTP → HTTPS redirect listener.
//!
//! * `FYR_TLS_CERT` + `FYR_TLS_KEY`: serve an operator-provided PEM chain.
//! * `FYR_TLS=self-signed`: generate an ECDSA P-256 certificate under
//!   `DATA_DIR/tls/` and reuse it across restarts. It is regenerated when a
//!   required name (localhost, hostname, interface IPs, `FYR_TLS_SANS`) is
//!   missing from it, when it expires within [`RENEW_BEFORE_SECS`], or when
//!   the files on disk do not match the recorded metadata.
//!
//! TLS uses rustls with the `ring` provider (no OpenSSL). The private key is
//! never logged; only the certificate's SHA-256 fingerprint is.

use anyhow::{anyhow, Context, Result};
use axum::{
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    Router,
};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};
use types::{Config, TlsMode};

/// Validity of a generated certificate. Apple platforms reject TLS server
/// certificates valid for more than 825 days.
const VALIDITY_DAYS: i64 = 825;
/// Regenerate the self-signed certificate when it expires within 30 days.
pub const RENEW_BEFORE_SECS: i64 = 30 * 86_400;
/// Earliest plausible "now" (2026-01-01T00:00:00Z). Devices without a
/// real-time clock (e.g. a Raspberry Pi booted offline) may start in 1970;
/// a certificate generated then would already be expired for clients with a
/// correct clock, so generation never uses a time before this floor.
const CLOCK_FLOOR_UNIX: i64 = 1_767_225_600;
/// `Strict-Transport-Security` value sent with operator-provided
/// certificates (180 days, no includeSubDomains: other hosts in the same
/// domain may still be plain HTTP).
pub const HSTS_VALUE: &str = "max-age=15552000";

const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
const META_FILE: &str = "meta.json";

/// Ready-to-serve TLS material.
pub struct TlsSetup {
    pub server_config: Arc<rustls::ServerConfig>,
    /// SHA-256 fingerprint of the leaf certificate, `AB:CD:...`.
    pub fingerprint_sha256: String,
}

/// Build the TLS server configuration for `config`, generating or loading
/// certificates as needed. Returns `None` for plain HTTP.
pub fn prepare(config: &Config) -> Result<Option<TlsSetup>> {
    match &config.server.tls.mode {
        TlsMode::Off => Ok(None),
        TlsMode::Provided { cert_path, key_path } => {
            let (certs, key) = load_pem_files(cert_path, key_path)?;
            warn_if_key_is_world_readable(key_path);
            let fingerprint = fingerprint_sha256(&certs[0]);
            let server_config = server_config(certs, key).with_context(|| {
                format!(
                    "Cannot use certificate {} with key {}",
                    cert_path.display(),
                    key_path.display()
                )
            })?;
            info!(
                "HTTPS enabled with the provided certificate {} (SHA-256 fingerprint {})",
                cert_path.display(),
                fingerprint
            );
            Ok(Some(TlsSetup {
                server_config,
                fingerprint_sha256: fingerprint,
            }))
        }
        TlsMode::SelfSigned => {
            let required = compute_sans(
                system_hostname().as_deref(),
                &config.server.host,
                &interface_ips(),
                &config.server.tls.extra_sans,
            );
            let cert = ensure_self_signed(&config.tls_dir(), &required, unix_now())?;
            let server_config = server_config(vec![cert.cert_der.clone()], cert.key_der)
                .context("Cannot use the generated self-signed certificate")?;
            info!(
                "HTTPS enabled with a self-signed certificate ({}), valid until {} for: {}",
                config.tls_dir().join(CERT_FILE).display(),
                format_unix(cert.meta.not_after_unix),
                cert.meta.sans.join(", ")
            );
            info!(
                "Self-signed certificate SHA-256 fingerprint: {} (compare it with the one your browser shows before trusting this server)",
                cert.meta.fingerprint_sha256
            );
            Ok(Some(TlsSetup {
                server_config,
                fingerprint_sha256: cert.meta.fingerprint_sha256,
            }))
        }
    }
}

/// rustls server config with the ring provider, safe default protocol
/// versions (TLS 1.2 + 1.3) and ALPN for HTTP/2 and HTTP/1.1.
fn server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ServerConfig>> {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| anyhow!("TLS protocol setup failed: {e}"))?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|e| anyhow!("{e}"))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Read a PEM certificate chain and private key. Error messages name the
/// file and the environment variable but never include file contents.
pub fn load_pem_files(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert_pem = std::fs::read(cert_path).with_context(|| {
        format!(
            "Cannot read TLS certificate {} (FYR_TLS_CERT)",
            cert_path.display()
        )
    })?;
    let key_pem = std::fs::read(key_path).with_context(|| {
        format!(
            "Cannot read TLS private key {} (FYR_TLS_KEY); check the path and that the Fyr user may read it",
            key_path.display()
        )
    })?;
    let certs = parse_certs(&cert_pem)
        .with_context(|| format!("Invalid TLS certificate {} (FYR_TLS_CERT)", cert_path.display()))?;
    let key = parse_key(&key_pem)
        .with_context(|| format!("Invalid TLS private key {} (FYR_TLS_KEY)", key_path.display()))?;
    Ok((certs, key))
}

fn parse_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("malformed PEM: {e}"))?;
    if certs.is_empty() {
        anyhow::bail!("no PEM \"CERTIFICATE\" block found");
    }
    Ok(certs)
}

fn parse_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_slice(pem).map_err(|e| match e {
        rustls::pki_types::pem::Error::NoItemsFound => anyhow!(
            "no PEM private key found (expected PKCS#8 \"PRIVATE KEY\", \"RSA PRIVATE KEY\" or \"EC PRIVATE KEY\")"
        ),
        other => anyhow!("malformed PEM: {other}"),
    })
}

/// SHA-256 over the DER certificate, formatted as browsers show it.
pub fn fingerprint_sha256(cert: &CertificateDer<'_>) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, cert.as_ref());
    digest
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

// ---------------------------------------------------------------------------
// Subject alternative names
// ---------------------------------------------------------------------------

/// Names a self-signed certificate must cover: loopback names, the
/// hostname (plus `<hostname>.local` for mDNS), a specific bind host,
/// usable interface addresses and `FYR_TLS_SANS`.
pub fn compute_sans(
    hostname: Option<&str>,
    bind_host: &str,
    interface_ips: &[IpAddr],
    extra: &[String],
) -> BTreeSet<String> {
    let mut sans: BTreeSet<String> = ["localhost", "127.0.0.1", "::1"]
        .into_iter()
        .map(String::from)
        .collect();
    if let Some(name) = hostname.and_then(types::tls::normalize_san) {
        if name.parse::<IpAddr>().is_err() {
            if !name.contains('.') && name != "localhost" {
                sans.insert(format!("{name}.local"));
            }
            sans.insert(name);
        }
    }
    if let Some(host) = types::tls::normalize_san(bind_host) {
        let unspecified = host
            .parse::<IpAddr>()
            .map(|ip| ip.is_unspecified())
            .unwrap_or(false);
        if !unspecified {
            sans.insert(host);
        }
    }
    for ip in interface_ips {
        if let Some(ip) = usable_interface_ip(*ip) {
            sans.insert(ip.to_string());
        }
    }
    sans.extend(extra.iter().cloned());
    sans
}

/// Interface addresses worth putting in a certificate. Loopback is always
/// covered separately; link-local addresses cannot be used in URLs without
/// a zone; global IPv6 addresses are skipped because privacy addresses
/// rotate and would force a new certificate (and a new browser warning) on
/// every restart; add stable ones with `FYR_TLS_SANS`.
fn usable_interface_ip(ip: IpAddr) -> Option<IpAddr> {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => (!v4.is_loopback()
            && !v4.is_unspecified()
            && !v4.is_link_local()
            && !v4.is_multicast()
            && !v4.is_broadcast())
        .then_some(ip),
        // Unique local addresses (fc00::/7) are stable LAN addresses.
        IpAddr::V6(v6) => ((v6.segments()[0] & 0xfe00) == 0xfc00).then_some(ip),
    }
}

fn interface_ips() -> Vec<IpAddr> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces.into_iter().map(|iface| iface.ip()).collect(),
        Err(error) => {
            warn!("Could not list network interfaces for the certificate: {error}");
            Vec::new()
        }
    }
}

fn system_hostname() -> Option<String> {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: the buffer is valid for `buf.len()` bytes; gethostname
        // writes at most that many and we only read up to the first NUL.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if rc != 0 {
            return None;
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..end].to_vec()).ok().filter(|s| !s.is_empty())
    }
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").ok().filter(|s| !s.is_empty())
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Self-signed certificate lifecycle
// ---------------------------------------------------------------------------

/// Metadata stored next to the generated certificate (`meta.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfSignedMeta {
    pub sans: Vec<String>,
    pub not_before_unix: i64,
    pub not_after_unix: i64,
    pub fingerprint_sha256: String,
}

/// Why a new self-signed certificate is (re)generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegenerateReason {
    Missing,
    /// Files on disk are unreadable or do not match `meta.json`.
    Inconsistent,
    /// Names the current certificate lacks.
    NamesAdded(Vec<String>),
    ExpiresSoon,
}

/// Decide whether the stored certificate can be reused. Pure so it can be
/// tested without touching the clock or the filesystem. The certificate is
/// kept as long as it covers every required name: an interface that is
/// temporarily down (e.g. a Wi-Fi hotspot that starts after Fyr) must not
/// cause a new certificate and a new browser warning on every restart.
pub fn regeneration_reason(
    stored: Option<&SelfSignedMeta>,
    actual_fingerprint: Option<&str>,
    required: &BTreeSet<String>,
    now_unix: i64,
) -> Option<RegenerateReason> {
    let Some(meta) = stored else {
        return Some(RegenerateReason::Missing);
    };
    if actual_fingerprint != Some(meta.fingerprint_sha256.as_str()) {
        return Some(RegenerateReason::Inconsistent);
    }
    let have: BTreeSet<&str> = meta.sans.iter().map(String::as_str).collect();
    let added: Vec<String> = required
        .iter()
        .filter(|name| !have.contains(name.as_str()))
        .cloned()
        .collect();
    if !added.is_empty() {
        return Some(RegenerateReason::NamesAdded(added));
    }
    if meta.not_after_unix - now_unix < RENEW_BEFORE_SECS {
        return Some(RegenerateReason::ExpiresSoon);
    }
    None
}

pub struct SelfSignedCert {
    pub cert_der: CertificateDer<'static>,
    pub key_der: PrivateKeyDer<'static>,
    pub meta: SelfSignedMeta,
}

/// Load the certificate from `dir`, or generate (and persist) a new one when
/// [`regeneration_reason`] says so.
pub fn ensure_self_signed(
    dir: &Path,
    required: &BTreeSet<String>,
    now_unix: i64,
) -> Result<SelfSignedCert> {
    create_private_dir(dir)?;
    let existing = load_self_signed(dir);
    let (meta, fingerprint) = match &existing {
        Some((cert, _, meta)) => (Some(meta), Some(fingerprint_sha256(cert))),
        None => (None, None),
    };
    let mut reason = regeneration_reason(meta, fingerprint.as_deref(), required, now_unix);
    if existing.is_none() && dir.join(META_FILE).exists() {
        // Metadata exists but the certificate or key could not be loaded.
        reason = Some(RegenerateReason::Inconsistent);
    }
    match reason {
        None => {
            let (cert_der, key_der, meta) = existing.expect("reuse implies existing");
            restrict_key_permissions(&dir.join(KEY_FILE));
            Ok(SelfSignedCert { cert_der, key_der, meta })
        }
        Some(reason) => {
            match &reason {
                RegenerateReason::Missing => info!("Generating a self-signed TLS certificate"),
                RegenerateReason::Inconsistent => warn!(
                    "Stored self-signed certificate in {} is unreadable or does not match meta.json; generating a new one",
                    dir.display()
                ),
                RegenerateReason::NamesAdded(names) => info!(
                    "Regenerating the self-signed certificate to add: {} (browsers will ask to accept the new certificate)",
                    names.join(", ")
                ),
                RegenerateReason::ExpiresSoon => {
                    info!("Self-signed certificate expires within 30 days; generating a new one")
                }
            }
            // When adding names, keep those the previous certificate already
            // covered so a temporarily missing interface does not drop them.
            // Renewal starts from the current set, so stale DHCP addresses
            // do not accumulate forever.
            let mut names = required.clone();
            if let (Some(meta), RegenerateReason::NamesAdded(_)) = (meta, &reason) {
                names.extend(meta.sans.iter().cloned());
            }
            let generated = generate_self_signed(&names, now_unix)?;
            persist_self_signed(dir, &generated)?;
            Ok(generated.cert)
        }
    }
}

struct Generated {
    cert: SelfSignedCert,
    cert_pem: String,
    key_pem: String,
}

fn generate_self_signed(names: &BTreeSet<String>, now_unix: i64) -> Result<Generated> {
    use rcgen::{
        CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
        KeyUsagePurpose, SanType, SerialNumber,
    };
    if now_unix < CLOCK_FLOOR_UNIX {
        warn!(
            "System clock reads {}, which looks wrong; dating the certificate from {} instead. Set the clock (or enable NTP) so browsers accept it.",
            format_unix(now_unix),
            format_unix(CLOCK_FLOOR_UNIX)
        );
    }
    let base = now_unix.max(CLOCK_FLOOR_UNIX);
    // Backdate one day for clients whose clock runs slightly behind.
    let not_before_unix = base - 86_400;
    let not_after_unix = not_before_unix + VALIDITY_DAYS * 86_400;

    let mut params = CertificateParams::default();
    params.not_before = time::OffsetDateTime::from_unix_timestamp(not_before_unix)?;
    params.not_after = time::OffsetDateTime::from_unix_timestamp(not_after_unix)?;
    // Random serial: Firefox rejects a new certificate that reuses the
    // issuer+serial of one it has seen before.
    let mut serial = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut serial)
        .map_err(|_| anyhow!("system random number generator failed"))?;
    serial[0] &= 0x7f;
    serial[0] |= 0x01;
    params.serial_number = Some(SerialNumber::from_slice(&serial));
    let mut dn = DistinguishedName::new();
    dn.push(DnType::OrganizationName, "Fyr");
    dn.push(DnType::CommonName, "Fyr self-signed certificate");
    params.distinguished_name = dn;
    // A leaf, not a CA: browsers reject CA certificates used as server certs.
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.subject_alt_names = names
        .iter()
        .map(|name| match name.parse::<IpAddr>() {
            Ok(ip) => Ok(SanType::IpAddress(ip)),
            Err(_) => Ok(SanType::DnsName(name.clone().try_into()?)),
        })
        .collect::<Result<Vec<_>, rcgen::Error>>()?;

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    let cert_der = cert.der().clone();
    let key_der = PrivateKeyDer::from(rustls::pki_types::PrivatePkcs8KeyDer::from(
        key_pair.serialize_der(),
    ));
    let meta = SelfSignedMeta {
        sans: names.iter().cloned().collect(),
        not_before_unix,
        not_after_unix,
        fingerprint_sha256: fingerprint_sha256(&cert_der),
    };
    Ok(Generated {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        cert: SelfSignedCert {
            cert_der,
            key_der,
            meta,
        },
    })
}

fn load_self_signed(
    dir: &Path,
) -> Option<(CertificateDer<'static>, PrivateKeyDer<'static>, SelfSignedMeta)> {
    let meta: SelfSignedMeta =
        serde_json::from_slice(&std::fs::read(dir.join(META_FILE)).ok()?).ok()?;
    let (mut certs, key) = load_pem_files(&dir.join(CERT_FILE), &dir.join(KEY_FILE)).ok()?;
    if certs.is_empty() {
        return None;
    }
    Some((certs.swap_remove(0), key, meta))
}

/// Write key (0600), certificate and metadata. Each file is written to a
/// temporary sibling and renamed, so a crash never leaves a half-written
/// key; meta.json goes last, making the set consistent only when complete.
fn persist_self_signed(dir: &Path, generated: &Generated) -> Result<()> {
    write_atomic(&dir.join(KEY_FILE), generated.key_pem.as_bytes(), true)?;
    write_atomic(&dir.join(CERT_FILE), generated.cert_pem.as_bytes(), false)?;
    let meta = serde_json::to_vec_pretty(&generated.cert.meta)?;
    write_atomic(&dir.join(META_FILE), &meta, false)?;
    Ok(())
}

fn write_atomic(path: &Path, contents: &[u8], private: bool) -> Result<()> {
    let tmp: PathBuf = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options
        .open(&tmp)
        .with_context(|| format!("Cannot write {}", tmp.display()))?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path).with_context(|| format!("Cannot write {}", path.display()))?;
    Ok(())
}

fn create_private_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if !dir.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .with_context(|| format!("Cannot create {}", dir.display()))?;
        } else if let Err(error) =
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        {
            warn!("Could not restrict permissions of {}: {error}", dir.display());
        }
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir).with_context(|| format!("Cannot create {}", dir.display()))?;
    Ok(())
}

/// Tighten a reused generated key to 0600 if something loosened it.
fn restrict_key_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.permissions().mode() & 0o077 != 0 {
                warn!("{} was readable by other users; restricting it to 0600", path.display());
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Operator-provided keys are not modified, only flagged.
fn warn_if_key_is_world_readable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.permissions().mode() & 0o004 != 0 {
                warn!(
                    "TLS private key {} is readable by all users; consider chmod 600 (or 640 with a dedicated group)",
                    path.display()
                );
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_unix(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| secs.to_string())
}

// ---------------------------------------------------------------------------
// HTTP → HTTPS redirect listener
// ---------------------------------------------------------------------------

/// Router for `FYR_HTTP_REDIRECT_PORT`: every request gets a 308 to the same
/// host, path and query on the HTTPS port. Nothing else is served.
pub fn redirect_router(https_port: u16) -> Router {
    Router::new().fallback(move |uri: Uri, headers: HeaderMap| async move {
        redirect_response(&uri, &headers, https_port)
    })
}

fn redirect_response(uri: &Uri, headers: &HeaderMap, https_port: u16) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| uri.host())
        .and_then(host_without_port);
    let Some(host) = host else {
        return (
            StatusCode::BAD_REQUEST,
            "This port only redirects to HTTPS; the request has no valid Host header.",
        )
            .into_response();
    };
    let port = if https_port == 443 {
        String::new()
    } else {
        format!(":{https_port}")
    };
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let location = format!("https://{host}{port}{path}");
    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
        "Redirecting to HTTPS",
    )
        .into_response()
}

/// Strip the port from a `Host` header and validate what remains as a DNS
/// name or IP literal (IPv6 is returned bracketed). Anything else is
/// rejected so the redirect cannot be bent into a different URL shape.
fn host_without_port(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('[') {
        let (inside, after) = rest.split_once(']')?;
        if !(after.is_empty() || after.strip_prefix(':').is_some_and(is_port)) {
            return None;
        }
        let ip: std::net::Ipv6Addr = inside.parse().ok()?;
        return Some(format!("[{ip}]"));
    }
    let host = match raw.rsplit_once(':') {
        Some((host, port)) if is_port(port) => host,
        Some(_) => return None,
        None => raw,
    };
    let name = types::tls::normalize_san(host)?;
    if name.starts_with("*.") || name.contains(':') {
        return None;
    }
    Some(name)
}

fn is_port(raw: &str) -> bool {
    !raw.is_empty() && raw.len() <= 5 && raw.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fyr-tls-{tag}-{unique}"))
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    const NOW: i64 = 1_790_000_000; // 2026-09-21

    #[test]
    fn sans_cover_loopback_hostname_interfaces_and_extras() {
        let ips = [
            IpAddr::V4(Ipv4Addr::new(192, 168, 4, 1)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(169, 254, 1, 2)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            "fe80::1".parse().unwrap(),
            "fd12:3456::1".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
            "::ffff:10.0.0.5".parse().unwrap(),
        ];
        let sans = compute_sans(Some("RaspberryPi"), "0.0.0.0", &ips, &["fyr.lan".to_string()]);
        assert_eq!(
            sans,
            set(&[
                "localhost", "127.0.0.1", "::1", "raspberrypi", "raspberrypi.local",
                "192.168.4.1", "fd12:3456::1", "10.0.0.5", "fyr.lan"
            ])
        );
        // A specific bind host is included; a dotted hostname gets no .local.
        let sans = compute_sans(Some("box.example.org"), "10.1.2.3", &[], &[]);
        assert!(sans.contains("10.1.2.3") && sans.contains("box.example.org"));
        assert!(!sans.iter().any(|s| s.ends_with(".local")));
        // Invalid hostnames are skipped instead of failing.
        let sans = compute_sans(Some("bad host"), "::", &[], &[]);
        assert_eq!(sans, set(&["localhost", "127.0.0.1", "::1"]));
    }

    fn meta(sans: &[&str], not_after: i64) -> SelfSignedMeta {
        SelfSignedMeta {
            sans: sans.iter().map(|s| s.to_string()).collect(),
            not_before_unix: 0,
            not_after_unix: not_after,
            fingerprint_sha256: "AA".into(),
        }
    }

    #[test]
    fn regeneration_decision() {
        let required = set(&["localhost", "192.168.4.1"]);
        let far = NOW + 400 * 86_400;
        assert_eq!(regeneration_reason(None, None, &required, NOW), Some(RegenerateReason::Missing));
        let m = meta(&["localhost", "192.168.4.1", "10.0.0.9"], far);
        // Superset of the required names, not expiring: reuse.
        assert_eq!(regeneration_reason(Some(&m), Some("AA"), &required, NOW), None);
        // Files do not match metadata.
        assert_eq!(
            regeneration_reason(Some(&m), Some("BB"), &required, NOW),
            Some(RegenerateReason::Inconsistent)
        );
        assert_eq!(
            regeneration_reason(Some(&m), None, &required, NOW),
            Some(RegenerateReason::Inconsistent)
        );
        // A new interface address appears.
        let more = set(&["localhost", "192.168.4.1", "192.168.1.20"]);
        assert_eq!(
            regeneration_reason(Some(&m), Some("AA"), &more, NOW),
            Some(RegenerateReason::NamesAdded(vec!["192.168.1.20".into()]))
        );
        // Expiry window: 31 days left is fine, 29 days is not, expired is not.
        let m31 = meta(&["localhost", "192.168.4.1"], NOW + 31 * 86_400);
        assert_eq!(regeneration_reason(Some(&m31), Some("AA"), &required, NOW), None);
        let m29 = meta(&["localhost", "192.168.4.1"], NOW + 29 * 86_400);
        assert_eq!(
            regeneration_reason(Some(&m29), Some("AA"), &required, NOW),
            Some(RegenerateReason::ExpiresSoon)
        );
        let expired = meta(&["localhost", "192.168.4.1"], NOW - 1);
        assert_eq!(
            regeneration_reason(Some(&expired), Some("AA"), &required, NOW),
            Some(RegenerateReason::ExpiresSoon)
        );
        // Clock running far behind (RTC-less boot) never forces churn.
        assert_eq!(regeneration_reason(Some(&m), Some("AA"), &required, 0), None);
    }

    #[test]
    fn self_signed_is_persisted_reused_and_regenerated() {
        let dir = temp_dir("lifecycle");
        let names = set(&["localhost", "127.0.0.1", "::1", "192.168.4.1"]);
        let first = ensure_self_signed(&dir, &names, NOW).unwrap();
        assert_eq!(first.meta.sans, names.iter().cloned().collect::<Vec<_>>());
        assert_eq!(first.meta.fingerprint_sha256, fingerprint_sha256(&first.cert_der));
        assert_eq!(first.meta.fingerprint_sha256.len(), 32 * 3 - 1);
        assert_eq!(first.meta.not_after_unix - first.meta.not_before_unix, VALIDITY_DAYS * 86_400);
        assert!(first.meta.not_before_unix < NOW);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        // The generated files load as a valid rustls server config.
        let (certs, key) = load_pem_files(&dir.join(CERT_FILE), &dir.join(KEY_FILE)).unwrap();
        server_config(certs, key).unwrap();

        // Same names: reused.
        let again = ensure_self_signed(&dir, &names, NOW + 86_400).unwrap();
        assert_eq!(again.meta, first.meta);
        // Fewer names (interface down): still reused.
        let fewer = set(&["localhost", "127.0.0.1", "::1"]);
        assert_eq!(ensure_self_signed(&dir, &fewer, NOW).unwrap().meta, first.meta);

        // New name: regenerated, keeps the old names, new fingerprint/serial.
        let more = set(&["localhost", "127.0.0.1", "::1", "fyr.lan"]);
        let grown = ensure_self_signed(&dir, &more, NOW).unwrap();
        assert_ne!(grown.meta.fingerprint_sha256, first.meta.fingerprint_sha256);
        assert!(grown.meta.sans.contains(&"192.168.4.1".to_string()));
        assert!(grown.meta.sans.contains(&"fyr.lan".to_string()));

        // Near expiry: regenerated.
        let late = grown.meta.not_after_unix - 10 * 86_400;
        let renewed = ensure_self_signed(&dir, &more, late).unwrap();
        assert!(renewed.meta.not_after_unix > grown.meta.not_after_unix);

        // Corrupted certificate file: regenerated rather than failing.
        std::fs::write(dir.join(CERT_FILE), "garbage").unwrap();
        let repaired = ensure_self_signed(&dir, &more, late).unwrap();
        assert_ne!(repaired.meta.fingerprint_sha256, renewed.meta.fingerprint_sha256);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_clock_is_floored() {
        let dir = temp_dir("clock");
        let cert = ensure_self_signed(&dir, &set(&["localhost"]), 0).unwrap();
        assert_eq!(cert.meta.not_before_unix, CLOCK_FLOOR_UNIX - 86_400);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pem_loading_errors_are_clear_and_do_not_leak_contents() {
        let dir = temp_dir("pem");
        std::fs::create_dir_all(&dir).unwrap();
        let generated = ensure_self_signed(&dir.join("gen"), &set(&["localhost"]), NOW).unwrap();
        let good_cert = dir.join("gen").join(CERT_FILE);
        let good_key = dir.join("gen").join(KEY_FILE);
        let key_pem = std::fs::read_to_string(&good_key).unwrap();

        let missing = dir.join("missing.pem");
        let err = format!("{:#}", load_pem_files(&missing, &good_key).unwrap_err());
        assert!(err.contains("FYR_TLS_CERT") && err.contains("missing.pem"), "{err}");
        let err = format!("{:#}", load_pem_files(&good_cert, &missing).unwrap_err());
        assert!(err.contains("FYR_TLS_KEY"), "{err}");

        let empty = dir.join("empty.pem");
        std::fs::write(&empty, "not pem at all").unwrap();
        let err = format!("{:#}", load_pem_files(&empty, &good_key).unwrap_err());
        assert!(err.contains("no PEM \"CERTIFICATE\""), "{err}");
        let err = format!("{:#}", load_pem_files(&good_cert, &empty).unwrap_err());
        assert!(err.contains("no PEM private key"), "{err}");

        // Swapped files: the key file holds no certificate and vice versa.
        let err = format!("{:#}", load_pem_files(&good_key, &good_cert).unwrap_err());
        assert!(err.contains("CERTIFICATE"), "{err}");
        assert!(!err.contains(key_pem.lines().nth(1).unwrap()), "key material leaked");

        // A key that does not belong to the certificate is rejected.
        let other = ensure_self_signed(&dir.join("other"), &set(&["localhost"]), NOW).unwrap();
        let err = server_config(vec![generated.cert_der.clone()], other.key_der).unwrap_err();
        assert!(!format!("{err:#}").is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Unreadable key (skipped when running as root, which bypasses modes).
            let locked = dir.join("locked.pem");
            std::fs::write(&locked, &key_pem).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            if std::fs::read(&locked).is_err() {
                let err = format!("{:#}", load_pem_files(&good_cert, &locked).unwrap_err());
                assert!(err.contains("Cannot read TLS private key"), "{err}");
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn redirect_targets_https_port_and_keeps_path() {
        let mut headers = HeaderMap::new();
        let uri: Uri = "/maps?x=1".parse().unwrap();
        for (host, port, expected) in [
            ("fyr.local:8080", 8443, "https://fyr.local:8443/maps?x=1"),
            ("192.168.4.1", 443, "https://192.168.4.1/maps?x=1"),
            ("[fd00::1]:80", 8443, "https://[fd00::1]:8443/maps?x=1"),
            ("FYR.LAN", 443, "https://fyr.lan/maps?x=1"),
        ] {
            headers.insert(header::HOST, host.parse().unwrap());
            let response = redirect_response(&uri, &headers, port);
            assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
            assert_eq!(response.headers()[header::LOCATION], expected);
        }
        for bad in ["evil.com/x", "a b", "[::1", "host:port", "user@host", "[::1]x"] {
            headers.insert(header::HOST, bad.parse().unwrap());
            let response = redirect_response(&uri, &headers, 443);
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{bad}");
        }
        let response = redirect_response(&uri, &HeaderMap::new(), 443);
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
