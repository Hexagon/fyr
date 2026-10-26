//! In-memory abuse limits for messaging: per-IP token buckets for writes and
//! a concurrency cap for SSE streams. Nothing here is ever persisted, so
//! client IP addresses never reach the disk.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Hard cap on tracked IPs per limiter; stale entries are evicted first.
const MAX_TRACKED_IPS: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Key used for per-client limits: IPv4 addresses as-is, IPv6 addresses
/// truncated to their /64 prefix (one host can trivially rotate addresses
/// inside its /64), and IPv4-mapped IPv6 addresses as IPv4.
pub fn client_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return IpAddr::V4(v4);
            }
            let mut segments = v6.segments();
            for segment in &mut segments[4..] {
                *segment = 0;
            }
            IpAddr::V6(std::net::Ipv6Addr::from(segments))
        }
    }
}

/// Classic token bucket keyed by client IP (see [`client_key`]).
pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

impl RateLimiter {
    /// `capacity` tokens (burst), refilled at one token per `refill_every`.
    pub fn new(capacity: u32, refill_every: Duration) -> Self {
        Self {
            capacity: f64::from(capacity),
            refill_per_sec: 1.0 / refill_every.as_secs_f64(),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Take one token for `ip`. Returns `Err(retry_after)` when empty.
    pub fn check(&self, ip: IpAddr) -> Result<(), Duration> {
        self.check_at(ip, Instant::now())
    }

    fn check_at(&self, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        let ip = client_key(ip);
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() >= MAX_TRACKED_IPS && !buckets.contains_key(&ip) {
            self.evict_full(&mut buckets, now);
            if buckets.len() >= MAX_TRACKED_IPS {
                // Table still full of active clients: fail closed for new
                // IPs rather than growing without bound.
                return Err(Duration::from_secs(60));
            }
        }
        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - bucket.tokens;
            Err(Duration::from_secs_f64(missing / self.refill_per_sec).max(Duration::from_secs(1)))
        }
    }

    /// Drop buckets that have refilled completely (equivalent to absent).
    pub fn prune(&self) {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        self.evict_full(&mut buckets, Instant::now());
    }

    fn evict_full(&self, buckets: &mut HashMap<IpAddr, Bucket>, now: Instant) {
        let full_after = self.capacity / self.refill_per_sec;
        buckets.retain(|_, b| now.saturating_duration_since(b.last).as_secs_f64() < full_after);
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.buckets.lock().unwrap().len()
    }
}

/// Caps concurrent SSE streams globally and per IP.
pub struct StreamLimiter {
    max_total: usize,
    max_per_ip: usize,
    inner: Arc<Mutex<StreamCounts>>,
}

#[derive(Default)]
struct StreamCounts {
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
}

/// Held for the lifetime of one stream; releases its slot on drop.
pub struct StreamPermit {
    ip: IpAddr,
    inner: Arc<Mutex<StreamCounts>>,
}

impl Drop for StreamPermit {
    fn drop(&mut self) {
        let mut counts = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        counts.total = counts.total.saturating_sub(1);
        if let Some(n) = counts.per_ip.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                counts.per_ip.remove(&self.ip);
            }
        }
    }
}

impl StreamLimiter {
    pub fn new(max_total: usize, max_per_ip: usize) -> Self {
        Self {
            max_total,
            max_per_ip,
            inner: Arc::new(Mutex::new(StreamCounts::default())),
        }
    }

    pub fn acquire(&self, ip: IpAddr) -> Option<StreamPermit> {
        let ip = client_key(ip);
        let mut counts = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mine = counts.per_ip.get(&ip).copied().unwrap_or(0);
        if counts.total >= self.max_total || mine >= self.max_per_ip {
            return None;
        }
        counts.total += 1;
        counts.per_ip.insert(ip, mine + 1);
        Some(StreamPermit {
            ip,
            inner: Arc::clone(&self.inner),
        })
    }

    pub fn active(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, n))
    }

    #[test]
    fn bucket_allows_burst_then_blocks_then_refills() {
        let limiter = RateLimiter::new(3, Duration::from_secs(10));
        let t0 = Instant::now();
        for _ in 0..3 {
            assert!(limiter.check_at(ip(1), t0).is_ok());
        }
        let retry = limiter.check_at(ip(1), t0).unwrap_err();
        assert!(retry >= Duration::from_secs(9), "{retry:?}");
        // Other IPs are independent.
        assert!(limiter.check_at(ip(2), t0).is_ok());
        // One token back after one refill period.
        assert!(limiter.check_at(ip(1), t0 + Duration::from_secs(10)).is_ok());
        assert!(limiter.check_at(ip(1), t0 + Duration::from_secs(10)).is_err());
    }

    #[test]
    fn ipv6_clients_are_limited_per_64_prefix() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(b).is_err(), "same /64 shares a bucket");
        assert!(limiter.check(other).is_ok());
        let mapped: IpAddr = "::ffff:10.0.0.9".parse().unwrap();
        assert_eq!(client_key(mapped), "10.0.0.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn prune_forgets_refilled_buckets() {
        let limiter = RateLimiter::new(2, Duration::from_millis(1));
        limiter.check(ip(1)).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        limiter.prune();
        assert_eq!(limiter.tracked(), 0);
    }

    #[test]
    fn stream_limiter_caps_per_ip_and_total_and_releases_on_drop() {
        let limiter = StreamLimiter::new(3, 2);
        let a1 = limiter.acquire(ip(1)).unwrap();
        let _a2 = limiter.acquire(ip(1)).unwrap();
        assert!(limiter.acquire(ip(1)).is_none(), "per-IP cap");
        let _b1 = limiter.acquire(ip(2)).unwrap();
        assert!(limiter.acquire(ip(3)).is_none(), "global cap");
        drop(a1);
        assert_eq!(limiter.active(), 2);
        assert!(limiter.acquire(ip(3)).is_some());
    }
}
