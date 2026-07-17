//! The Downloads module as a standalone process (its `.kmod` entrypoint).
//!
//! This one process hosts the whole downloads vertical:
//!   * the Downloads module itself (queue / download-clients / organize routes +
//!     the librqbit engine lifecycle),
//!   * the co-located torrent-engine modules (Transmission / qBittorrent), which
//!     register their client `kind` into THIS process's shared `DownloadManager`
//!     on enable. Engines are plugins into the one manager + registry, so they
//!     must share its process; `serve()` hosts them as an in-process cluster.
//!
//! It PROVIDES (over the port bridge, for sibling sidecars): `DownloadGrabPort` +
//! `DownloadDbPort` (→ Acquisition) and `DownloadVpnPort` (→ VPN). It CONSUMES
//! (as client proxies through the core reverse-proxy): `VpnProxyPort` (← VPN) and
//! `IndexerDbPort` / `IndexerSearchPort` / `TorrentFetchPort` (← Indexers).

use std::sync::Arc;

use kroma_module_runtime::RemoteHost;
use kroma_module_sdk::ports::{
    DownloadClientHost, DownloadDbPort, DownloadGrabPort, DownloadVpnPort, IndexerDbPort,
    IndexerSearchPort, TorrentFetchPort, VpnProxyPort,
};
use kroma_torrent::{DownloadDb, DownloadManager};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = std::path::PathBuf::from(std::env::var("KROMA_DATA_DIR")?);

    // One manager instance, shared by this process's own routes (as a service),
    // the co-hosted engines (as the DownloadClientHost port), and the provider
    // routes served for sibling sidecars.
    let downloads = DownloadManager::new(&data_dir);

    // Provider ports served over the bridge for the Acquisition + VPN sidecars.
    let grab: Arc<dyn DownloadGrabPort> = downloads.clone();
    let ledger: Arc<dyn DownloadDbPort> = Arc::new(DownloadDb);
    let dc_vpn: Arc<dyn DownloadVpnPort> = downloads.clone();
    let extra = kroma_port_bridge::download_routes::<RemoteHost>(grab, ledger)
        .merge(kroma_port_bridge::downloadvpn_routes::<RemoteHost>(dc_vpn));

    let downloads_setup = downloads.clone();
    kroma_module_runtime::serve(
        move |host| {
            // The manager is this process's concrete service (its own routes
            // resolve it) AND the DownloadClientHost port (the co-hosted engine
            // modules register their kind into it on enable).
            host.register_service(downloads_setup.clone());
            let dc_host: Arc<dyn DownloadClientHost> = downloads_setup.clone();
            host.register_port(dc_host);

            // Ports consumed from sibling sidecars, reached through the core
            // reverse-proxy (`{core}/api/module/{id}/_port/...`).
            let vp: Arc<dyn VpnProxyPort> = Arc::new(kroma_port_bridge::VpnProxyClient::new(
                host.sibling_resolver("tv.kroma.vpn"),
            ));
            host.register_port(vp);
            let tf: Arc<dyn TorrentFetchPort> = Arc::new(kroma_port_bridge::TorrentFetchClient::new(
                host.sibling_resolver("tv.kroma.indexer"),
            ));
            host.register_port(tf);
            let idb: Arc<dyn IndexerDbPort> = Arc::new(kroma_port_bridge::IndexerDbClient::new(
                host.sibling_resolver("tv.kroma.indexer"),
            ));
            host.register_port(idb);
            let isearch: Arc<dyn IndexerSearchPort> =
                Arc::new(kroma_port_bridge::IndexerSearchClient::new(
                    host.sibling_resolver("tv.kroma.indexer"),
                ));
            host.register_port(isearch);
        },
        vec![
            kroma_torrent::server_module::<RemoteHost>(),
            kroma_transmission::server_module::<RemoteHost>(),
            kroma_qbittorrent::server_module::<RemoteHost>(),
        ],
        extra,
    )
    .await
}
