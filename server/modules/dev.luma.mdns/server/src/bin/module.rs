//! The mDNS module as a standalone process (its `.lmod` entrypoint). A
//! lifecycle-only module: no routes; `on_enable` starts advertising.

use luma_module_runtime::RemoteHost;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    luma_module_runtime::serve(|_host| {}, luma_mdns::server_module::<RemoteHost>()).await
}
