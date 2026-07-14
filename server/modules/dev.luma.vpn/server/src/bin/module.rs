//! The VPN module as a standalone process (its `.lmod` entrypoint).
//!
//! It provides `VpnProxyPort` (served over the port bridge for the indexer /
//! torrents consumers) and its admin page (`/api/admin/vpn/*`, reverse-proxied
//! by the core). It consumes `DownloadVpnPort` (the download engine's kill-switch
//! status/restart) from the torrents sidecar, sibling-to-sibling via the core proxy.

use std::sync::Arc;

use luma_module_runtime::RemoteHost;
use luma_module_sdk::host::HostCtx;
use luma_module_sdk::ports::{DownloadVpnPort, VpnProxyPort};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let vpnproxy: Arc<dyn VpnProxyPort> = Arc::new(luma_vpn::VpnProxy);

    luma_module_runtime::serve(
        move |host| {
            // The module owns the WireGuard bridge service (its own code resolves
            // it via service::<Vpn>).
            host.register_service(luma_vpn::Vpn::new(host.data_dir().to_path_buf()));
            // Consume DownloadVpnPort from the torrents sidecar (kill-switch status).
            let dvpn: Arc<dyn DownloadVpnPort> = Arc::new(luma_port_bridge::DownloadVpnClient::new(
                host.sibling_resolver("dev.luma.torrents"),
            ));
            host.register_port(dvpn);
        },
        vec![luma_vpn::server_module::<RemoteHost>()],
        luma_port_bridge::vpnproxy_routes(vpnproxy),
    )
    .await
}
