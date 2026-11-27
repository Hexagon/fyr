//! Trusted reverse proxies and client IP resolution.
//!
//! Fyr normally keys rate limits on the real TCP peer address. Behind a
//! TLS-terminating reverse proxy every request comes from the proxy, so all
//! clients would share one rate-limit bucket. `FYR_TRUSTED_PROXIES` lists the
//! proxy addresses (IPs or CIDR blocks); only for connections from those
//! peers is `X-Forwarded-For` consulted.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// One trusted network: an address plus prefix length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpNet {
    addr: IpAddr,
    prefix: u8,
}

impl IpNet {
    /// Parse `"10.0.0.1"`, `"10.0.0.0/8"`, `"::1"` or `"fd00::/8"`.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let (addr, prefix) = match raw.split_once('/') {
            Some((addr, prefix)) => (addr.trim(), Some(prefix.trim().parse::<u8>().ok()?)),
            None => (raw, None),
        };
        let addr: IpAddr = addr.parse().ok()?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max);
        (prefix <= max).then_some(Self { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, canonical(ip)) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                prefix_eq(&net.octets(), &ip.octets(), self.prefix)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                prefix_eq(&net.octets(), &ip.octets(), self.prefix)
            }
            _ => false,
        }
    }
}

/// Treat IPv4-mapped IPv6 peers (`::ffff:10.0.0.1`, common on dual-stack
/// listeners) as their IPv4 address.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    }
}

fn prefix_eq(a: &[u8], b: &[u8], prefix: u8) -> bool {
    let full = (prefix / 8) as usize;
    if a[..full] != b[..full] {
        return false;
    }
    let rest = prefix % 8;
    if rest == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rest);
    (a[full] & mask) == (b[full] & mask)
}

/// The configured trusted proxies (empty = trust nobody, the default).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedProxies(Vec<IpNet>);

impl TrustedProxies {
    /// Parse a comma/whitespace separated list. Returns the valid entries
    /// and the entries that could not be parsed (for a startup warning).
    pub fn parse(raw: &str) -> (Self, Vec<String>) {
        let mut nets = Vec::new();
        let mut invalid = Vec::new();
        for item in raw.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()) {
            match IpNet::parse(item) {
                Some(net) => nets.push(net),
                None => invalid.push(item.to_string()),
            }
        }
        (Self(nets), invalid)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|net| net.contains(ip))
    }

    /// The client address for a request from `peer`.
    ///
    /// When `peer` is a trusted proxy, walk the `X-Forwarded-For` chain
    /// (all header values in order, comma separated) from the right and
    /// return the first address that is not itself a trusted proxy. Entries
    /// further left are client-controlled and never used. Anything
    /// unparsable, or a peer that is not trusted, yields `peer`.
    pub fn client_ip<'a>(&self, peer: IpAddr, forwarded_for: impl IntoIterator<Item = &'a str>) -> IpAddr {
        let peer = canonical(peer);
        if !self.contains(peer) {
            return peer;
        }
        let chain: Vec<&str> = forwarded_for
            .into_iter()
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        for hop in chain.iter().rev() {
            let Ok(ip) = hop.parse::<IpAddr>() else {
                return peer;
            };
            let ip = canonical(ip);
            if !self.contains(ip) {
                return ip;
            }
        }
        peer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn parses_addresses_and_cidr_blocks() {
        let (trusted, invalid) = TrustedProxies::parse("127.0.0.1, 10.0.0.0/8 ::1 fd00::/8,bogus,10.0.0.0/33");
        assert_eq!(invalid, vec!["bogus".to_string(), "10.0.0.0/33".to_string()]);
        assert!(trusted.contains(ip("127.0.0.1")));
        assert!(trusted.contains(ip("10.200.3.4")));
        assert!(!trusted.contains(ip("11.0.0.1")));
        assert!(trusted.contains(ip("::1")));
        assert!(trusted.contains(ip("fd12:3456::1")));
        assert!(trusted.contains(ip("::ffff:10.1.2.3")), "IPv4-mapped peers match IPv4 nets");
    }

    #[test]
    fn non_byte_aligned_prefixes() {
        let net = IpNet::parse("172.16.0.0/12").unwrap();
        assert!(net.contains(ip("172.31.255.255")));
        assert!(!net.contains(ip("172.32.0.0")));
    }

    #[test]
    fn untrusted_peer_ignores_forwarded_for() {
        let (trusted, _) = TrustedProxies::parse("127.0.0.1");
        assert_eq!(trusted.client_ip(ip("192.168.1.9"), ["1.2.3.4"]), ip("192.168.1.9"));
        let none = TrustedProxies::default();
        assert_eq!(none.client_ip(ip("127.0.0.1"), ["1.2.3.4"]), ip("127.0.0.1"));
    }

    #[test]
    fn trusted_peer_uses_rightmost_untrusted_hop() {
        let (trusted, _) = TrustedProxies::parse("127.0.0.1,10.0.0.0/8");
        // Client-supplied "6.6.6.6" on the left is never used.
        assert_eq!(
            trusted.client_ip(ip("127.0.0.1"), ["6.6.6.6, 192.168.1.20", "10.0.0.5"]),
            ip("192.168.1.20")
        );
        assert_eq!(trusted.client_ip(ip("127.0.0.1"), Vec::<&str>::new()), ip("127.0.0.1"));
        assert_eq!(trusted.client_ip(ip("127.0.0.1"), ["garbage"]), ip("127.0.0.1"));
        assert_eq!(trusted.client_ip(ip("127.0.0.1"), ["10.0.0.7"]), ip("127.0.0.1"));
        assert_eq!(trusted.client_ip(ip("::ffff:127.0.0.1"), ["192.168.1.20"]), ip("192.168.1.20"));
    }
}
