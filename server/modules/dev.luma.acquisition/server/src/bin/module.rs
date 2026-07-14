//! The Acquisition module as a standalone process (its `.lmod` entrypoint).
//!
//! Serves its admin routes (`/api/admin/acquisition/*`, reverse-proxied by the
//! core) and runs the search / import / match passes on resident timers (spawned
//! by the module's `on_enable`, since the core's cron can't reach into a sidecar).
//!
//! It CONSUMES (as client proxies through the core reverse-proxy): `DownloadGrabPort`
//! + `DownloadDbPort` (← the Downloads sidecar, for grab + the import ledger) and
//! `IndexerDbPort` + `IndexerSearchPort` (← the Indexers sidecar, for the search
//! sweep). It provides no ports (nothing consumes acquisition).

use std::sync::Arc;

use luma_module_runtime::RemoteHost;
use luma_module_sdk::ports::{
    DownloadDbPort, DownloadGrabPort, IndexerDbPort, IndexerSearchPort,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core_url = std::env::var("LUMA_CORE_URL")?;
    let token = std::env::var("LUMA_HOST_TOKEN")?;

    luma_module_runtime::serve(
        move |host| {
            // Reach a sibling module's ports through the core reverse-proxy.
            let sibling = |id: &str| -> luma_port_bridge::Resolver {
                let base = format!("{core_url}/api/module/{id}");
                let tok = token.clone();
                Arc::new(move || Some((base.clone(), tok.clone())))
            };
            let grab: Arc<dyn DownloadGrabPort> =
                Arc::new(luma_port_bridge::DownloadGrabClient::new(sibling("dev.luma.torrents")));
            host.register_port(grab);
            let ledger: Arc<dyn DownloadDbPort> =
                Arc::new(luma_port_bridge::DownloadDbClient::new(sibling("dev.luma.torrents")));
            host.register_port(ledger);
            let idb: Arc<dyn IndexerDbPort> =
                Arc::new(luma_port_bridge::IndexerDbClient::new(sibling("dev.luma.indexer")));
            host.register_port(idb);
            let isearch: Arc<dyn IndexerSearchPort> =
                Arc::new(luma_port_bridge::IndexerSearchClient::new(sibling("dev.luma.indexer")));
            host.register_port(isearch);
        },
        vec![luma_acquisition::server_module::<RemoteHost>()],
        axum::Router::new(),
    )
    .await
}
