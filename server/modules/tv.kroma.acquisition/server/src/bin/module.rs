//! The Acquisition module as a standalone process (its `.kmod` entrypoint).
//!
//! Serves its admin routes (`/api/admin/acquisition/*`, reverse-proxied by the
//! core). The search / import / match passes are contributed as
//! [`ServerModule::jobs`](kroma_module_sdk::host::ServerModule::jobs): the runtime
//! registers them with the CORE JobManager (so they appear in admin Tâches with
//! cron scheduling + history) and serves the `/_job/run/{key}` endpoint the core
//! scheduler calls to run each pass in this process.
//!
//! It CONSUMES (as client proxies through the core reverse-proxy): `DownloadGrabPort`
//! + `DownloadDbPort` (← the Downloads sidecar, for grab + the import ledger) and
//! `IndexerDbPort` + `IndexerSearchPort` (← the Indexers sidecar, for the search
//! sweep). It provides no ports (nothing consumes acquisition).

use std::sync::Arc;

use kroma_module_runtime::RemoteHost;
use kroma_module_sdk::ports::{
    DownloadDbPort, DownloadGrabPort, IndexerDbPort, IndexerSearchPort,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    kroma_module_runtime::serve(
        move |host| {
            let grab: Arc<dyn DownloadGrabPort> = Arc::new(kroma_port_bridge::DownloadGrabClient::new(
                host.sibling_resolver("tv.kroma.torrents"),
            ));
            host.register_port(grab);
            let ledger: Arc<dyn DownloadDbPort> = Arc::new(kroma_port_bridge::DownloadDbClient::new(
                host.sibling_resolver("tv.kroma.torrents"),
            ));
            host.register_port(ledger);
            let idb: Arc<dyn IndexerDbPort> = Arc::new(kroma_port_bridge::IndexerDbClient::new(
                host.sibling_resolver("tv.kroma.indexer"),
            ));
            host.register_port(idb);
            let isearch: Arc<dyn IndexerSearchPort> =
                Arc::new(kroma_port_bridge::IndexerSearchClient::new(
                    host.sibling_resolver("tv.kroma.indexer"),
                ));
            host.register_port(isearch);
            // The search / import / match passes are contributed via `jobs()`; the
            // runtime registers them with the core JobManager and serves the
            // `/_job/run/{key}` endpoint the core scheduler drives them through.
        },
        vec![kroma_acquisition::server_module::<RemoteHost>()],
        // Provider routes for the core's /api/requests/:id/search + /grab endpoints.
        kroma_acquisition::acqsearch_routes::<RemoteHost>(),
    )
    .await
}
