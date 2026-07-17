//! The mDNS module as a standalone process (its `.kmod` entrypoint). A
//! lifecycle-only module: no routes; `on_enable` starts advertising.

use kroma_module_runtime::RemoteHost;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    kroma_module_runtime::serve_one(|_host| {}, kroma_mdns::server_module::<RemoteHost>()).await
}
