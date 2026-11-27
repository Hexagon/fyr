//! Built-in HTTPS configuration (`FYR_TLS`, `FYR_TLS_CERT`, `FYR_TLS_KEY`,
//! `FYR_TLS_SANS`, `FYR_HTTP_REDIRECT_PORT`).
//!
//! Parsing is pure (it takes an environment lookup function) so every
//! combination can be unit tested. Invalid combinations are reported as an
//! error string; the server refuses to start with it instead of silently
//! falling back to plain HTTP.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

/// How (and whether) Fyr terminates TLS itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum TlsMode {
    /// Plain HTTP (default).
    #[default]
    Off,
    /// Generate and persist a self-signed certificate under `DATA_DIR/tls/`.
    SelfSigned,
    /// Serve an operator-provided certificate chain and private key (PEM).
    Provided { cert_path: PathBuf, key_path: PathBuf },
}

/// TLS settings derived from the environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    #[serde(flatten)]
    pub mode: TlsMode,
    /// Extra subject alternative names for the self-signed certificate
    /// (`FYR_TLS_SANS`), normalised: lowercase DNS names or IP addresses.
    #[serde(default)]
    pub extra_sans: Vec<String>,
    /// Optional plain-HTTP port that only answers with a 308 redirect to
    /// HTTPS (`FYR_HTTP_REDIRECT_PORT`). Off by default.
    #[serde(default)]
    pub http_redirect_port: Option<u16>,
    /// SHA-256 fingerprint (`AB:CD:...`) of the certificate being served.
    /// Filled in by the server at startup; never read from the environment.
    #[serde(skip)]
    pub fingerprint_sha256: Option<String>,
    /// Non-fatal configuration notes, logged as warnings at startup.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

impl TlsConfig {
    /// True when Fyr serves HTTPS itself.
    pub fn is_enabled(&self) -> bool {
        !matches!(self.mode, TlsMode::Off)
    }

    /// True for the generated self-signed certificate mode.
    pub fn is_self_signed(&self) -> bool {
        matches!(self.mode, TlsMode::SelfSigned)
    }

    /// `Strict-Transport-Security` is only safe to send with a certificate
    /// browsers trust without an exception, i.e. an operator-provided one.
    pub fn sends_hsts(&self) -> bool {
        matches!(self.mode, TlsMode::Provided { .. })
    }

    /// Short label for logs and the status API.
    pub fn mode_label(&self) -> &'static str {
        match self.mode {
            TlsMode::Off => "off",
            TlsMode::SelfSigned => "self-signed",
            TlsMode::Provided { .. } => "provided",
        }
    }

    /// Parse TLS settings. `lookup` returns the (raw) value of an
    /// environment variable; `https_port` is the main listening port
    /// (`FYR_PORT`), used to reject a clashing redirect port.
    pub fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
        https_port: u16,
    ) -> Result<Self, String> {
        let get = |name: &str| lookup(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

        let mode_raw = get("FYR_TLS").map(|v| v.to_lowercase());
        let cert = get("FYR_TLS_CERT");
        let key = get("FYR_TLS_KEY");
        let sans_raw = get("FYR_TLS_SANS");
        let redirect_raw = get("FYR_HTTP_REDIRECT_PORT");
        let mut warnings = Vec::new();

        let explicit_off = matches!(
            mode_raw.as_deref(),
            Some("off" | "false" | "0" | "no" | "none" | "disabled")
        );

        let mode = match mode_raw.as_deref() {
            None => match (cert, key) {
                (None, None) => TlsMode::Off,
                (Some(cert), Some(key)) => TlsMode::Provided {
                    cert_path: PathBuf::from(cert),
                    key_path: PathBuf::from(key),
                },
                (Some(_), None) => {
                    return Err("FYR_TLS_CERT is set but FYR_TLS_KEY is missing; set both to serve HTTPS with your own certificate".into())
                }
                (None, Some(_)) => {
                    return Err("FYR_TLS_KEY is set but FYR_TLS_CERT is missing; set both to serve HTTPS with your own certificate".into())
                }
            },
            Some(_) if explicit_off => {
                if cert.is_some() || key.is_some() {
                    warnings.push(
                        "FYR_TLS is off; ignoring FYR_TLS_CERT/FYR_TLS_KEY and serving plain HTTP"
                            .to_string(),
                    );
                }
                TlsMode::Off
            }
            Some("self-signed" | "selfsigned" | "self_signed") => {
                if cert.is_some() || key.is_some() {
                    return Err("FYR_TLS=self-signed cannot be combined with FYR_TLS_CERT/FYR_TLS_KEY; unset FYR_TLS to use your own certificate, or unset the certificate paths".into());
                }
                TlsMode::SelfSigned
            }
            Some(other) => {
                return Err(format!(
                    "Unsupported FYR_TLS value {other:?}; use \"self-signed\" (or set FYR_TLS_CERT and FYR_TLS_KEY), or \"off\""
                ))
            }
        };

        let mut extra_sans = Vec::new();
        if let Some(raw) = sans_raw {
            if mode != TlsMode::SelfSigned {
                warnings.push(
                    "FYR_TLS_SANS only applies with FYR_TLS=self-signed; ignoring it".to_string(),
                );
            } else {
                for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                    let name = normalize_san(entry).ok_or_else(|| {
                        format!("Invalid FYR_TLS_SANS entry {entry:?}: expected a DNS name or an IP address")
                    })?;
                    if !extra_sans.contains(&name) {
                        extra_sans.push(name);
                    }
                }
            }
        }

        let http_redirect_port = match redirect_raw {
            None => None,
            Some(raw) => {
                let port = raw
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| format!("Invalid FYR_HTTP_REDIRECT_PORT {raw:?}: expected a port number 1-65535"))?;
                if mode == TlsMode::Off {
                    return Err("FYR_HTTP_REDIRECT_PORT requires HTTPS; set FYR_TLS=self-signed or FYR_TLS_CERT/FYR_TLS_KEY, or unset FYR_HTTP_REDIRECT_PORT".into());
                }
                if port == https_port {
                    return Err(format!(
                        "FYR_HTTP_REDIRECT_PORT ({port}) must differ from FYR_PORT ({https_port})"
                    ));
                }
                Some(port)
            }
        };

        Ok(Self {
            mode,
            extra_sans,
            http_redirect_port,
            fingerprint_sha256: None,
            warnings,
        })
    }
}

/// Normalise one subject alternative name: an IP address (IPv6 may be
/// bracketed) or a DNS name (optionally a leading `*.` wildcard). Returns
/// `None` for anything that is neither.
pub fn normalize_san(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let unbracketed = raw
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(raw);
    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    let name = raw.trim_end_matches('.').to_ascii_lowercase();
    let rest = name.strip_prefix("*.").unwrap_or(name.as_str());
    if rest.is_empty() || name.len() > 253 {
        return None;
    }
    let labels_ok = rest.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    });
    // Purely numeric "names" (an invalid IP such as 999.1.1.1) are rejected.
    let all_numeric = rest.split('.').all(|l| l.bytes().all(|b| b.is_ascii_digit()));
    if !labels_ok || all_numeric {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(vars: &[(&str, &str)], port: u16) -> Result<TlsConfig, String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        TlsConfig::from_lookup(|name| map.get(name).cloned(), port)
    }

    #[test]
    fn defaults_to_plain_http() {
        let tls = parse(&[], 8080).unwrap();
        assert_eq!(tls.mode, TlsMode::Off);
        assert!(!tls.is_enabled());
        assert!(!tls.sends_hsts());
        assert_eq!(tls.http_redirect_port, None);
        // Empty values count as unset.
        let tls = parse(&[("FYR_TLS", " "), ("FYR_TLS_CERT", "")], 8080).unwrap();
        assert_eq!(tls.mode, TlsMode::Off);
    }

    #[test]
    fn self_signed_mode_and_sans() {
        let tls = parse(
            &[
                ("FYR_TLS", "Self-Signed"),
                ("FYR_TLS_SANS", "fyr.lan, 192.168.4.1,[fd00::1],FYR.LAN., *.box.lan"),
            ],
            8443,
        )
        .unwrap();
        assert_eq!(tls.mode, TlsMode::SelfSigned);
        assert!(tls.is_enabled() && tls.is_self_signed() && !tls.sends_hsts());
        assert_eq!(tls.extra_sans, vec!["fyr.lan", "192.168.4.1", "fd00::1", "*.box.lan"]);
    }

    #[test]
    fn provided_certificate_mode() {
        let tls = parse(&[("FYR_TLS_CERT", "/c.pem"), ("FYR_TLS_KEY", "/k.pem")], 443).unwrap();
        assert_eq!(
            tls.mode,
            TlsMode::Provided {
                cert_path: "/c.pem".into(),
                key_path: "/k.pem".into()
            }
        );
        assert!(tls.sends_hsts());
        assert_eq!(tls.mode_label(), "provided");
    }

    #[test]
    fn rejects_invalid_combinations() {
        assert!(parse(&[("FYR_TLS_CERT", "/c.pem")], 8080).unwrap_err().contains("FYR_TLS_KEY"));
        assert!(parse(&[("FYR_TLS_KEY", "/k.pem")], 8080).unwrap_err().contains("FYR_TLS_CERT"));
        assert!(parse(
            &[("FYR_TLS", "self-signed"), ("FYR_TLS_CERT", "/c"), ("FYR_TLS_KEY", "/k")],
            8080
        )
        .is_err());
        assert!(parse(&[("FYR_TLS", "yes please")], 8080).unwrap_err().contains("Unsupported"));
        assert!(parse(&[("FYR_TLS", "on")], 8080).is_err());
        // Redirect port needs TLS, must be a valid non-zero port and differ.
        assert!(parse(&[("FYR_HTTP_REDIRECT_PORT", "8081")], 8080).unwrap_err().contains("requires HTTPS"));
        assert!(parse(&[("FYR_TLS", "self-signed"), ("FYR_HTTP_REDIRECT_PORT", "0")], 8080).is_err());
        assert!(parse(&[("FYR_TLS", "self-signed"), ("FYR_HTTP_REDIRECT_PORT", "http")], 8080).is_err());
        assert!(parse(&[("FYR_TLS", "self-signed"), ("FYR_HTTP_REDIRECT_PORT", "8080")], 8080)
            .unwrap_err()
            .contains("must differ"));
        assert!(parse(&[("FYR_TLS", "self-signed"), ("FYR_TLS_SANS", "bad name!")], 8080)
            .unwrap_err()
            .contains("FYR_TLS_SANS"));
    }

    #[test]
    fn redirect_port_is_parsed() {
        let tls = parse(&[("FYR_TLS", "self-signed"), ("FYR_HTTP_REDIRECT_PORT", " 80 ")], 443).unwrap();
        assert_eq!(tls.http_redirect_port, Some(80));
    }

    #[test]
    fn explicit_off_wins_with_a_warning_and_stray_sans_warn() {
        let tls = parse(&[("FYR_TLS", "off"), ("FYR_TLS_CERT", "/c"), ("FYR_TLS_KEY", "/k")], 8080).unwrap();
        assert_eq!(tls.mode, TlsMode::Off);
        assert_eq!(tls.warnings.len(), 1);

        let tls = parse(&[("FYR_TLS_SANS", "fyr.lan")], 8080).unwrap();
        assert!(tls.extra_sans.is_empty());
        assert_eq!(tls.warnings.len(), 1);
    }

    #[test]
    fn san_normalisation() {
        assert_eq!(normalize_san("Fyr.Local").as_deref(), Some("fyr.local"));
        assert_eq!(normalize_san("[::1]").as_deref(), Some("::1"));
        assert_eq!(normalize_san("FD00:0:0::1").as_deref(), Some("fd00::1"));
        assert_eq!(normalize_san("raspberrypi").as_deref(), Some("raspberrypi"));
        assert_eq!(normalize_san("*.lan").as_deref(), Some("*.lan"));
        for bad in ["", "*.", "-bad.lan", "bad-.lan", "a..b", "999.1.1.1", "with space", "under_score", "http://x"] {
            assert_eq!(normalize_san(bad), None, "{bad}");
        }
        assert_eq!(normalize_san(&"a".repeat(64)), None);
    }
}
