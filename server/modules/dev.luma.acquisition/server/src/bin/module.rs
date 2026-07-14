//! The Acquisition module as a standalone process (its `.lmod` entrypoint).
//!
//! Serves its admin routes (`/api/admin/acquisition/*`, reverse-proxied by the
//! core) and runs the search / import / match passes on resident timers
//! (`start_cron`, since the core's JobManager can't reach into a sidecar).
//!
//! It CONSUMES (as client proxies through the core reverse-proxy): `DownloadGrabPort`
//! + `DownloadDbPort` (← the Downloads sidecar, for grab + the import ledger) and
//! `IndexerDbPort` + `IndexerSearchPort` (← the Indexers sidecar, for the search
//! sweep). It provides no ports (nothing consumes acquisition).

use std::sync::Arc;

use luma_module_runtime::RemoteHost;
use luma_module_sdk::host::HostCtx;
use luma_module_sdk::ports::{
    DownloadDbPort, DownloadGrabPort, IndexerDbPort, IndexerSearchPort,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    luma_module_runtime::serve(
        move |host| {
            let grab: Arc<dyn DownloadGrabPort> = Arc::new(luma_port_bridge::DownloadGrabClient::new(
                host.sibling_resolver("dev.luma.torrents"),
            ));
            host.register_port(grab);
            let ledger: Arc<dyn DownloadDbPort> = Arc::new(luma_port_bridge::DownloadDbClient::new(
                host.sibling_resolver("dev.luma.torrents"),
            ));
            host.register_port(ledger);
            let idb: Arc<dyn IndexerDbPort> = Arc::new(luma_port_bridge::IndexerDbClient::new(
                host.sibling_resolver("dev.luma.indexer"),
            ));
            host.register_port(idb);
            let isearch: Arc<dyn IndexerSearchPort> =
                Arc::new(luma_port_bridge::IndexerSearchClient::new(
                    host.sibling_resolver("dev.luma.indexer"),
                ));
            host.register_port(isearch);
            // Sidecar-only: drive the passes on resident timers (the core's
            // JobManager can't reach this process). In-core, `JOBS` does this.
            luma_acquisition::start_cron(Arc::new(host.clone()) as Arc<dyn HostCtx>);
        },
        vec![luma_acquisition::server_module::<RemoteHost>()],
        axum::Router::new(),
    )
    .await
}
