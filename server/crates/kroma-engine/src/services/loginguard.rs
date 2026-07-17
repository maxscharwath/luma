//! In-memory brute-force guard for the password login endpoint, keyed by client IP.
//!
//! After [`MAX_FAILS`] consecutive failed logins from one source IP we lock that
//! source out for a cooldown that doubles on each further breach (capped at
//! [`MAX_COOLDOWN_SECS`]). An online password-guessing attack is throttled to a
//! handful of tries per hour, while a legitimate user who fat-fingers a password
//! a few times is barely affected. A correct login clears the source's counter.
//!
//! Process-local (resets on restart) and best-effort fine for a single-binary
//! self-hosted deployment. This is defence-in-depth: the PBKDF2 password hash is
//! still the real barrier, and a reverse proxy / WAF can add its own limit. It
//! mirrors the PIN lockout in [`crate::api::pin`], but keyed by IP (login is
//! unauthenticated) with an escalating rather than fixed window.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Failed logins allowed from one IP before the first lockout kicks in.
const MAX_FAILS: u32 = 5;
/// Base lockout window, applied once [`MAX_FAILS`] is reached.
const BASE_COOLDOWN_SECS: i64 = 60;
/// Ceiling for the doubling backoff (1 hour).
const MAX_COOLDOWN_SECS: i64 = 60 * 60;
/// Forget an IP's record after this long with no activity (memory hygiene).
const IDLE_TTL_SECS: i64 = 60 * 60;
/// Hard cap on tracked IPs; a flood of distinct source IPs is pruned to this.
const MAX_ENTRIES: usize = 50_000;

struct Attempt {
    fails: u32,
    locked_until: i64,
    seen: i64,
}

static ATTEMPTS: LazyLock<Mutex<HashMap<String, Attempt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn now_secs() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// Seconds left on `ip`'s lockout, or `None` if it may attempt a login now.
pub fn lock_remaining(ip: &str) -> Option<i64> {
    let map = ATTEMPTS.lock().ok()?;
    let rem = map.get(ip)?.locked_until - now_secs();
    (rem > 0).then_some(rem)
}

/// Record a failed login from `ip`. Returns the lockout window in seconds when
/// this failure trips (or re-arms) a lockout, else 0.
pub fn record_fail(ip: &str) -> i64 {
    let Ok(mut map) = ATTEMPTS.lock() else {
        return 0;
    };
    let now = now_secs();
    if map.len() >= MAX_ENTRIES {
        map.retain(|_, a| a.locked_until > now || now - a.seen < IDLE_TTL_SECS);
    }
    let a = map
        .entry(ip.to_string())
        .or_insert(Attempt { fails: 0, locked_until: 0, seen: now });
    a.fails += 1;
    a.seen = now;
    if a.fails >= MAX_FAILS {
        // Doubling backoff: 60s, 120s, 240s, … capped at MAX_COOLDOWN_SECS. The
        // shift exponent is clamped so it can never overflow the `i64` shift.
        let over = (a.fails - MAX_FAILS).min(16);
        let secs = BASE_COOLDOWN_SECS.saturating_mul(1_i64 << over).min(MAX_COOLDOWN_SECS);
        a.locked_until = now + secs;
        return secs;
    }
    0
}

/// Clear an IP's failure record (called on a successful login).
pub fn reset(ip: &str) {
    if let Ok(mut map) = ATTEMPTS.lock() {
        map.remove(ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalating_cooldown_then_reset() {
        // Unique key so the process-global map can't collide with peers.
        let ip = "203.0.113.7-test-escalating";
        reset(ip);
        // Below the threshold: recorded but no lockout yet.
        for _ in 0..MAX_FAILS - 1 {
            assert_eq!(record_fail(ip), 0);
        }
        // The MAX_FAILS-th consecutive fail locks for the base window.
        assert_eq!(record_fail(ip), BASE_COOLDOWN_SECS);
        let rem = lock_remaining(ip).expect("should be locked");
        assert!(rem > 0 && rem <= BASE_COOLDOWN_SECS);
        // The next fail doubles the window.
        assert_eq!(record_fail(ip), BASE_COOLDOWN_SECS * 2);
        // A correct login clears everything.
        reset(ip);
        assert!(lock_remaining(ip).is_none());
    }

    #[test]
    fn cooldown_is_capped() {
        let ip = "203.0.113.8-test-cap";
        reset(ip);
        let mut last = 0;
        for _ in 0..40 {
            last = record_fail(ip);
        }
        assert_eq!(last, MAX_COOLDOWN_SECS);
        reset(ip);
    }
}
