//! The Indexers module as a standalone process (its `.lmod` entrypoint).
//!
//! Provides IndexerDbPort / IndexerSearchPort / TorrentFetchPort (served over the
//! port bridge for torrents + acquisition) and its admin page
//! (`/api/admin/indexers/*`, reverse-proxied by the core). It consumes TorznabPort
//! + VpnProxyPort from the sibling sidecars, resolved through the core proxy.

use std::sync::Arc;

use luma_module_runtime::RemoteHost;
use luma_module_sdk::ports::{
    IndexerDbPort, IndexerSearchPort, TorrentFetchPort, TorznabPort, VpnProxyPort,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core_url = std::env::var("LUMA_CORE_URL")?;
    let token = std::env::var("LUMA_HOST_TOKEN")?;
    let db: Arc<dyn IndexerDbPort> = Arc::new(luma_indexer::IndexerDb);
    let search: Arc<dyn IndexerSearchPort> = Arc::new(luma_indexer::IndexerSearch);
    let fetch: Arc<dyn TorrentFetchPort> = Arc::new(luma_indexer::IndexerTorrentFetch);

    luma_module_runtime::serve(
        move |host| {
            // Reach a sibling module's ports through the core reverse-proxy.
            let sibling = |id: &str| -> luma_port_bridge::Resolver {
                let base = format!("{core_url}/api/module/{id}");
                let tok = token.clone();
                Arc::new(move || Some((base.clone(), tok.clone())))
            };
            let tz: Arc<dyn TorznabPort> =
                Arc::new(luma_port_bridge::TorznabClient::new(sibling("dev.luma.torznab")));
            host.register_port(tz);
            let vp: Arc<dyn VpnProxyPort> =
                Arc::new(luma_port_bridge::VpnProxyClient::new(sibling("dev.luma.vpn")));
            host.register_port(vp);
        },
        vec![luma_indexer::server_module::<RemoteHost>()],
        luma_port_bridge::indexer_routes(db, search, fetch),
    )
    .await
}
