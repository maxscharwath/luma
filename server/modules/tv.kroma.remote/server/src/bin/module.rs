//! The Remote-access module as a standalone process (its `.kmod` entrypoint).
//!
//! The whole binary is one `serve()` call: the runtime opens the shared DB,
//! builds the out-of-process host, and serves this module's admin routes on the
//! local port the core supervisor assigned. The service wiring that used to live
//! in the core binary's `main.rs` (constructing `RemoteAccess`) now lives here,
//! where it belongs.

use kroma_module_runtime::RemoteHost;
use kroma_module_sdk::host::HostCtx;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    kroma_module_runtime::serve_one(
        |host| {
            // The module owns its connector; register it so the module's own code
            // (on_enable, the admin routes) resolves it via `service::<RemoteAccess>`.
            host.register_service(kroma_remote::RemoteAccess::new(host.data_dir().to_path_buf()));
        },
        kroma_remote::server_module::<RemoteHost>(),
    )
    .await
}
