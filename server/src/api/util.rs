//! Shared HTTP-handler helpers. The `spawn_blocking` DB combinators now live on
//! the module host seam (so relocated module crates share them); re-exported here
//! so `crate::api::util::{blocking, query}` call sites are unchanged.

use std::net::SocketAddr;

use axum::http::HeaderMap;

pub(crate) use kroma_module_host::{blocking, query};

/// Best client IP for an incoming request. Cloudflare sets `CF-Connecting-IP` to
/// the true client and overwrites it at the edge, so it can't be spoofed by a
/// client prefilling the header the way the first `X-Forwarded-For` hop can.
/// Preferred when present; falls back to the first `X-Forwarded-For` hop (other
/// reverse proxies, e.g. the Synology one), then the direct socket peer. Shared
/// by playback session accounting and the login brute-force guard.
pub(crate) fn client_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    if let Some(cf) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
        let cf = cf.trim();
        if !cf.is_empty() {
            return cf.to_string();
        }
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let first = first.trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    addr.ip().to_string()
}
