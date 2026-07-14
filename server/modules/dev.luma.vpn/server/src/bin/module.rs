//! The VPN module as a standalone process (its `.lmod` entrypoint).
//!
//! It provides `VpnProxyPort` (served over the port bridge for the indexer /
//! torrents consumers) and its admin page (`/api/admin/vpn/*`, reverse-proxied
//! by the core). It consumes `DownloadVpnPort` (the download engine's kill-switch
//! status/restart) via the core bridge while torrents is still in-core.

use std::sync::Arc;

use luma_module_runtime::RemoteHost;
use luma_module_sdk::host::HostCtx;
use luma_module_sdk::ports::{DownloadVpnPort, VpnProxyPort};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core_url = std::env::var("LUMA_CORE_URL")?;
    let token = std::env::var("LUMA_HOST_TOKEN")?;
    let vpnproxy: Arc<dyn VpnProxyPort> = Arc::new(luma_vpn::VpnProxy);

    luma_module_runtime::serve(
        move |host| {
            // The module owns the WireGuard bridge service (its own code resolves
            // it via service::<Vpn>).
            host.register_service(luma_vpn::Vpn::new(host.data_dir().to_path_buf()));
            // Consume DownloadVpnPort through the core bridge (torrents in-core).
            let resolve: luma_port_bridge::Resolver =
                Arc::new(move || Some((core_url.clone(), token.clone())));
            let dvpn: Arc<dyn DownloadVpnPort> =
                Arc::new(luma_port_bridge::DownloadVpnClient::new(resolve));
            host.register_port(dvpn);
        },
        vec![luma_vpn::server_module::<RemoteHost>()],
        luma_port_bridge::vpnproxy_routes(vpnproxy),
    )
    .await
}
