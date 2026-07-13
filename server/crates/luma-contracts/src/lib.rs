//! Peer port traits: the contracts modules use to call each other WITHOUT
//! depending on each other's crate. A provider module implements a port and the
//! composition root registers it in the host service registry; a consumer module
//! resolves it via `luma_module_host::resolve_port`. Only generic contracts live
//! here, never a module's own types, so no crate here depends on a module.

use luma_module_host::HostCtx;

// The download-client contract (engine trait + shared types + the host port), so
// download engine modules depend on the SDK, not on the torrents crate.
pub mod download_client;
pub use download_client::*;

/// The VPN module's local SOCKS5 bridge, for modules that route traffic through it
/// (downloads always; indexers when opted in). `None` when no bridge is configured
/// or the VPN module is absent.
pub trait VpnProxyPort: Send + Sync {
    /// The `socks5://127.0.0.1:<port>` URL when a bridge is configured, else `None`.
    fn proxy_url(&self, host: &dyn HostCtx) -> Option<String>;
}

/// The indexer module's authenticated `.torrent` fetch. Built-in Cardigann
/// indexers cookie-gate their downloads, so a bare fetch returns the login page;
/// this lets the downloads module grab the real file without depending on the
/// indexer crate.
pub trait TorrentFetchPort: Send + Sync {
    /// Fetch the `.torrent` bytes for `url` through the indexer's authenticated
    /// session. `None` when this indexer is not one the port handles (the caller
    /// then does a plain HTTP fetch); `Some(Err)` when the authenticated fetch
    /// itself failed.
    fn fetch_torrent(
        &self,
        host: &dyn HostCtx,
        indexer_id: &str,
        url: &str,
    ) -> Option<anyhow::Result<Vec<u8>>>;
}

// --- Download manager VPN surface -------------------------------------------
// The VPN module's admin page shows the download engine's kill-switch status,
// runs a seal check, and restarts the engine after the VPN config changes. These
// live here so the VPN module resolves them as a port instead of naming the
// torrents crate.

/// The download engine's VPN / kill-switch status, for the VPN admin page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatusView {
    pub connected: bool,
    pub exit_ip: Option<String>,
    /// Downloads are currently held by the kill switch.
    pub paused: bool,
}

/// Outcome of a VPN seal check (is peer traffic actually leaving via the proxy?).
pub struct VpnSeal {
    pub sealed: bool,
    pub proxied_ip: Option<String>,
    pub direct_ip: Option<String>,
    pub error: Option<String>,
}

/// The download manager's VPN surface, resolved by the VPN module.
#[luma_module_host::async_trait]
pub trait DownloadVpnPort: Send + Sync {
    /// Latest VPN/kill-switch status of the download engine.
    fn vpn_status(&self) -> Option<VpnStatusView>;
    /// Run the VPN seal check now.
    fn vpn_seal_check(&self, host: &dyn HostCtx) -> Option<VpnSeal>;
    /// Restart the embedded engine (e.g. after the VPN config changed).
    async fn restart_engine(&self, host: &dyn HostCtx);
}
