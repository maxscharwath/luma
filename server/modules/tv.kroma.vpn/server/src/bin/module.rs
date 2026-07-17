//! The VPN module as a standalone process (its `.kmod` entrypoint).
//!
//! It provides `VpnProxyPort` (served over the port bridge for the indexer /
//! torrents consumers) and its admin page (`/api/admin/vpn/*`, reverse-proxied
//! by the core). It consumes `DownloadVpnPort` (the download engine's kill-switch
//! status/restart) from the torrents sidecar, sibling-to-sibling via the core proxy.

use std::sync::Arc;

use kroma_module_runtime::RemoteHost;
use kroma_module_sdk::host::HostCtx;
use kroma_module_sdk::ports::{DownloadVpnPort, VpnProxyPort};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let vpnproxy: Arc<dyn VpnProxyPort> = Arc::new(kroma_vpn::VpnProxy);

    kroma_module_runtime::serve(
        move |host| {
            // The module owns the WireGuard bridge service (its own code resolves
            // it via service::<Vpn>).
            host.register_service(kroma_vpn::Vpn::new(host.data_dir().to_path_buf()));
            // Consume DownloadVpnPort from the torrents sidecar (kill-switch status).
            let dvpn: Arc<dyn DownloadVpnPort> = Arc::new(kroma_port_bridge::DownloadVpnClient::new(
                host.sibling_resolver("tv.kroma.torrents"),
            ));
            host.register_port(dvpn);
        },
        vec![kroma_vpn::server_module::<RemoteHost>()],
        kroma_port_bridge::vpnproxy_routes(vpnproxy),
    )
    .await
}
