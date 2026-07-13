//! The Torznab module as a standalone process (its `.lmod` entrypoint). It has no
//! admin routes — it exists to provide the `TorznabPort` (Jackett/Prowlarr search)
//! to the indexer over HTTP, so it serves only the port bridge routes.

use std::sync::Arc;

use luma_module_sdk::ports::TorznabPort;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let engine: Arc<dyn TorznabPort> = Arc::new(luma_torznab::TorznabEngine);
    luma_module_runtime::serve(|_host| {}, vec![], luma_port_bridge::torznab_routes(engine)).await
}
