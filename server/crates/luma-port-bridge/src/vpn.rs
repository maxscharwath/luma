//! Bridges for the vpn <-> torrents cycle: `VpnProxyPort` (vpn provides, indexer
//! + torrents consume) and `DownloadVpnPort` (torrents provides, vpn consumes;
//! the only async port).

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use luma_module_host::HostCtx;
use luma_module_sdk::host::async_trait;
use luma_module_sdk::ports::{DownloadVpnPort, VpnProxyPort, VpnSeal, VpnStatusView};
use serde_json::json;

use crate::{call_raw, Resolver};

// --- VpnProxyPort ------------------------------------------------------------

pub fn vpnproxy_routes<S: HostCtx + Clone + Send + Sync + 'static>(
    port: Arc<dyn VpnProxyPort>,
) -> Router<S> {
    Router::new().route("/_port/vpnproxy/proxy_url", post(proxy_url_h::<S>)).layer(Extension(port))
}

async fn proxy_url_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(port): Extension<Arc<dyn VpnProxyPort>>,
) -> Json<Option<String>> {
    Json(tokio::task::spawn_blocking(move || port.proxy_url(&host)).await.ok().flatten())
}

/// `VpnProxyPort` forwarding to the vpn sidecar.
pub struct VpnProxyClient {
    resolve: Resolver,
}

impl VpnProxyClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

impl VpnProxyPort for VpnProxyClient {
    fn proxy_url(&self, _host: &dyn HostCtx) -> Option<String> {
        call_raw(&self.resolve, "vpnproxy/proxy_url", &json!({})).ok().flatten()
    }
}

// --- DownloadVpnPort (async) -------------------------------------------------

pub fn downloadvpn_routes<S: HostCtx + Clone + Send + Sync + 'static>(
    port: Arc<dyn DownloadVpnPort>,
) -> Router<S> {
    Router::new()
        .route("/_port/downloadvpn/status", post(status_h::<S>))
        .route("/_port/downloadvpn/seal_check", post(seal_h::<S>))
        .route("/_port/downloadvpn/restart", post(restart_h::<S>))
        .layer(Extension(port))
}

async fn status_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(_host): State<S>,
    Extension(port): Extension<Arc<dyn DownloadVpnPort>>,
) -> Json<Option<VpnStatusView>> {
    Json(tokio::task::spawn_blocking(move || port.vpn_status()).await.ok().flatten())
}

async fn seal_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(port): Extension<Arc<dyn DownloadVpnPort>>,
) -> Json<Option<VpnSeal>> {
    Json(tokio::task::spawn_blocking(move || port.vpn_seal_check(&host)).await.ok().flatten())
}

async fn restart_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(port): Extension<Arc<dyn DownloadVpnPort>>,
) -> Json<()> {
    port.restart_engine(&host).await;
    Json(())
}

/// `DownloadVpnPort` forwarding to whoever provides it (the core while torrents is
/// in-core, later the torrents sidecar).
pub struct DownloadVpnClient {
    resolve: Resolver,
}

impl DownloadVpnClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

#[async_trait]
impl DownloadVpnPort for DownloadVpnClient {
    fn vpn_status(&self) -> Option<VpnStatusView> {
        call_raw(&self.resolve, "downloadvpn/status", &json!({})).ok().flatten()
    }

    fn vpn_seal_check(&self, _host: &dyn HostCtx) -> Option<VpnSeal> {
        call_raw(&self.resolve, "downloadvpn/seal_check", &json!({})).ok().flatten()
    }

    async fn restart_engine(&self, _host: &dyn HostCtx) {
        let _: anyhow::Result<()> = call_raw(&self.resolve, "downloadvpn/restart", &json!({}));
    }
}
