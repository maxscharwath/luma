//! Bridges for the vpn <-> torrents cycle: `VpnProxyPort` (vpn provides, indexer
//! + torrents consume) and `DownloadVpnPort` (torrents provides, vpn consumes;
//! the only async port).

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use kroma_module_host::HostCtx;
use kroma_module_sdk::host::async_trait;
use kroma_module_sdk::ports::{DownloadVpnPort, VpnProxyPort, VpnSeal, VpnStatusView};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct MockHost;
    impl HostCtx for MockHost {
        fn db(&self) -> &kroma_module_sdk::db::Pool {
            unimplemented!()
        }
        fn data_dir(&self) -> &std::path::Path {
            std::path::Path::new("/tmp")
        }
        fn require(
            &self,
            _user: &kroma_module_sdk::domain::User,
            _perm: kroma_module_sdk::domain::Permission,
        ) -> Result<(), axum::response::Response> {
            Ok(())
        }
        fn require_any_admin(
            &self,
            _user: &kroma_module_sdk::domain::User,
        ) -> Result<(), axum::response::Response> {
            Ok(())
        }
        fn lerr(
            &self,
            _user: &kroma_module_sdk::domain::User,
            _status: axum::http::StatusCode,
            _key: &str,
        ) -> axum::response::Response {
            unimplemented!()
        }
        fn setting_str(&self, _key: &str, default: &str) -> String {
            default.to_string()
        }
        fn setting_bool(&self, _key: &str, default: bool) -> bool {
            default
        }
        fn setting_i64(&self, _key: &str, default: i64) -> i64 {
            default
        }
        fn set_settings(&self, _patch: std::collections::BTreeMap<String, serde_json::Value>) {}
        fn publish(&self, _event: kroma_module_host::Event) {}
        fn trigger_job(&self, _key: &'static str, _reason: &'static str) {}
        fn module_enabled(&self, _id: &str) -> bool {
            true
        }
        fn library_folders(&self) -> Vec<kroma_module_host::LibraryFolders> {
            Vec::new()
        }
        fn tmdb_api_key(&self) -> Option<String> {
            None
        }
        fn metadata_language(&self) -> String {
            "en".into()
        }
        fn get_service(
            &self,
            _type_id: std::any::TypeId,
        ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
            None
        }
    }

    struct StubProxy(Option<String>);
    impl VpnProxyPort for StubProxy {
        fn proxy_url(&self, _host: &dyn HostCtx) -> Option<String> {
            self.0.clone()
        }
    }

    struct StubVpn;
    #[async_trait]
    impl DownloadVpnPort for StubVpn {
        fn vpn_status(&self) -> Option<VpnStatusView> {
            Some(VpnStatusView { connected: true, exit_ip: Some("1.2.3.4".into()), paused: false })
        }
        fn vpn_seal_check(&self, _host: &dyn HostCtx) -> Option<VpnSeal> {
            Some(VpnSeal {
                sealed: true,
                proxied_ip: Some("1.2.3.4".into()),
                direct_ip: None,
                error: None,
            })
        }
        async fn restart_engine(&self, _host: &dyn HostCtx) {}
    }

    fn offline() -> Resolver {
        Arc::new(|| None)
    }

    #[tokio::test]
    async fn proxy_url_handler_some_and_none() {
        let some: Arc<dyn VpnProxyPort> = Arc::new(StubProxy(Some("socks5://127.0.0.1:1080".into())));
        let Json(v) = proxy_url_h::<MockHost>(State(MockHost), Extension(some)).await;
        assert_eq!(v.as_deref(), Some("socks5://127.0.0.1:1080"));

        let none: Arc<dyn VpnProxyPort> = Arc::new(StubProxy(None));
        let Json(v) = proxy_url_h::<MockHost>(State(MockHost), Extension(none)).await;
        assert!(v.is_none());
    }

    #[tokio::test]
    async fn download_vpn_handlers() {
        let vpn: Arc<dyn DownloadVpnPort> = Arc::new(StubVpn);

        let Json(status) = status_h::<MockHost>(State(MockHost), Extension(vpn.clone())).await;
        assert!(status.unwrap().connected);

        let Json(seal) = seal_h::<MockHost>(State(MockHost), Extension(vpn.clone())).await;
        assert!(seal.unwrap().sealed);

        let Json(()) = restart_h::<MockHost>(State(MockHost), Extension(vpn)).await;
    }

    #[tokio::test]
    async fn clients_offline_return_none() {
        let proxy = VpnProxyClient::new(offline());
        assert!(proxy.proxy_url(&MockHost).is_none());

        let vpn = DownloadVpnClient::new(offline());
        assert!(vpn.vpn_status().is_none());
        assert!(vpn.vpn_seal_check(&MockHost).is_none());
        // Fire-and-forget restart must not panic when the provider is offline.
        vpn.restart_engine(&MockHost).await;
    }
}
