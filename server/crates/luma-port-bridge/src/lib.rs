//! HTTP bridges for the cross-module SDK ports.
//!
//! When a provider and a consumer of a port live in different processes, the
//! provider serves the port at `/_port/<port>/<method>` (JSON in/out) and the
//! consumer resolves a **client proxy** that implements the same trait by
//! forwarding each call over localhost. The boundary types already derive serde;
//! any `&dyn HostCtx` argument is dropped from the wire and re-supplied locally on
//! the provider side.
//!
//! Discovery: the consumer is handed a `Resolver` closure that returns the
//! provider's `(base_url, auth_token)` (e.g. from the supervisor's live port map),
//! so a provider restart on a new port is picked up transparently.

use std::sync::Arc;

use anyhow::anyhow;

/// Resolves a provider module's base URL + callback token at call time. `None`
/// when the provider isn't currently running.
pub type Resolver = Arc<dyn Fn() -> Option<(String, String)> + Send + Sync>;

/// Serialize `body`, POST it to `base/path` with the bearer token, and
/// deserialize the `Result<T, String>` envelope the provider returns.
fn call<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    resolve: &Resolver,
    path: &str,
    body: &B,
) -> anyhow::Result<T> {
    let (base, token) = resolve().ok_or_else(|| anyhow!("provider module not running"))?;
    let resp = luma_http::Fetch::new()
        .header("authorization", format!("Bearer {token}"))
        .post_json(&format!("{base}/_port/{path}"), &serde_json::to_value(body)?)?
        .ensure_ok()?;
    let out: Result<T, String> = resp.json()?;
    out.map_err(|e| anyhow!(e))
}

/// Like [`call`] but the provider returns `T` directly (no `Result` envelope) —
/// used for port methods that return `Option<_>` / infallible values. Any
/// transport error maps to the caller's own fallback (usually `None`).
fn call_raw<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    resolve: &Resolver,
    path: &str,
    body: &B,
) -> anyhow::Result<T> {
    let (base, token) = resolve().ok_or_else(|| anyhow!("provider module not running"))?;
    let resp = luma_http::Fetch::new()
        .header("authorization", format!("Bearer {token}"))
        .post_json(&format!("{base}/_port/{path}"), &serde_json::to_value(body)?)?
        .ensure_ok()?;
    Ok(resp.json()?)
}

pub mod indexer;
pub mod torznab;
pub mod vpn;
pub use indexer::{
    indexer_routes, IndexerDbClient, IndexerSearchClient, TorrentFetchClient,
};
pub use torznab::{torznab_routes, TorznabClient};
pub use vpn::{downloadvpn_routes, vpnproxy_routes, DownloadVpnClient, VpnProxyClient};
