//! Quick Connect device pairing (Plex/Jellyfin-style).
//!
//! A device that's painful to type on (the TV) calls [`initiate`] and shows the
//! returned short numeric **code**. An already-signed-in user approves that code
//! from the web app ([`authorize`]). The device meanwhile polls with its private
//! **secret** ([`poll`]) and, once approved, receives a real session token so
//! it logs in without anyone typing a password on the remote.
//!
//! State is in-memory (entries are short-lived) behind a `Mutex`, mirroring the
//! lightweight `activity` / `transcode` session stores.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::services::auth::{random_token, random_u32};
use crate::model::User;

/// How long a pending code stays valid.
pub const CODE_TTL_SECS: i64 = 300; // 5 minutes
/// Number of digits in a Quick Connect code.
const CODE_DIGITS: u32 = 4;
/// Hard cap on concurrent pending codes. `initiate` is unauthenticated by design
/// (the pairing device isn't signed in yet), so without a bound a flood of calls
/// could grow the map unchecked within the TTL window. Capping it also keeps the
/// map sparse against the 10^CODE_DIGITS keyspace so the code-generation loop
/// below stays collision-free. Generous vs real concurrency (a handful of devices).
const MAX_PENDING: usize = 256;

struct Pending {
    secret: String,
    created_at: i64,
    /// Set once a signed-in user approves the code.
    user: Option<User>,
    token: Option<String>,
}

pub struct QuickConnectInner {
    /// Keyed by the human-facing code.
    map: Mutex<HashMap<String, Pending>>,
}

pub type QuickConnect = Arc<QuickConnectInner>;

/// Result of [`QuickConnectInner::initiate`].
pub struct Initiated {
    pub code: String,
    pub secret: String,
    pub expires_in: i64,
}

/// Result of [`QuickConnectInner::poll`].
pub enum PollState {
    Pending,
    Authorized { token: String, user: User },
    Unknown,
}

pub fn new() -> QuickConnect {
    Arc::new(QuickConnectInner { map: Mutex::new(HashMap::new()) })
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

impl QuickConnectInner {
    /// Drop expired entries.
    fn reap(map: &mut HashMap<String, Pending>) {
        let cutoff = now() - CODE_TTL_SECS;
        map.retain(|_, p| p.created_at > cutoff);
    }

    /// Create a pending request → a unique code + a private secret.
    pub fn initiate(&self) -> Initiated {
        let mut map = self.map.lock().unwrap();
        Self::reap(&mut map);
        // Bound memory: once at capacity (after reaping expired entries), evict the
        // oldest pending code so a flood of unauthenticated initiate() calls can't
        // grow the map past MAX_PENDING. Pairing stays functional (always issues a
        // code); the evicted device simply re-initiates.
        while map.len() >= MAX_PENDING {
            let Some(oldest) = map.iter().min_by_key(|(_, p)| p.created_at).map(|(c, _)| c.clone())
            else {
                break;
            };
            map.remove(&oldest);
        }
        let modulo = 10u32.pow(CODE_DIGITS);
        let code = loop {
            let candidate = format!("{:0>width$}", random_u32() % modulo, width = CODE_DIGITS as usize);
            if !map.contains_key(&candidate) {
                break candidate;
            }
        };
        let secret = random_token();
        map.insert(
            code.clone(),
            Pending { secret: secret.clone(), created_at: now(), user: None, token: None },
        );
        Initiated { code, secret, expires_in: CODE_TTL_SECS }
    }

    /// Approve a code for `user`, attaching a freshly-minted session `token`.
    /// Returns false if the code is unknown/expired.
    pub fn authorize(&self, code: &str, user: User, token: String) -> bool {
        let mut map = self.map.lock().unwrap();
        Self::reap(&mut map);
        match map.get_mut(code) {
            Some(p) => {
                p.user = Some(user);
                p.token = Some(token);
                true
            }
            None => false,
        }
    }

    /// Poll by secret. Once authorized, the entry is consumed and its token +
    /// user returned.
    pub fn poll(&self, secret: &str) -> PollState {
        let mut map = self.map.lock().unwrap();
        Self::reap(&mut map);
        let Some(code) = map.iter().find(|(_, p)| p.secret == secret).map(|(c, _)| c.clone()) else {
            return PollState::Unknown;
        };
        let entry = map.get(&code).expect("entry present");
        match (entry.token.clone(), entry.user.clone()) {
            (Some(token), Some(user)) => {
                map.remove(&code);
                PollState::Authorized { token, user }
            }
            _ => PollState::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initiate_is_capped_under_flood() {
        let qc = new();
        for _ in 0..(MAX_PENDING + 100) {
            qc.initiate();
        }
        // A flood of pending codes never grows the map past the cap.
        assert!(qc.map.lock().unwrap().len() <= MAX_PENDING);
    }
}
