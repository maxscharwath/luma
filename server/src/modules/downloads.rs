//! The Downloads module (backend): still in-tree because its admin routes have
//! not been relocated to the `luma-downloads` crate yet. It owns the
//! download-client / downloads-queue admin routes and the librqbit engine
//! lifecycle, so disabling it 404s those routes and stops the running engine. Its
//! download sub-engines (rqbit / transmission / qBittorrent) plug into the
//! `DownloadClientRegistry`; VPN is a separate module that this one
//! `optionalDependsOn`. Reaches its `DownloadManager` through the host service
//! registry, like a relocated module would.

use std::sync::Arc;

use axum::Router;

use luma_downloads::DownloadManager;
use luma_module_host::{async_trait, service, HostCtx, ServerModule};

use crate::state::SharedState;

pub struct DownloadsModule;

#[async_trait]
impl ServerModule<SharedState> for DownloadsModule {
    fn id(&self) -> &'static str {
        luma_torrent::MODULE_ID
    }

    fn admin_routes(&self, _host: &SharedState) -> Option<Router<SharedState>> {
        Some(
            crate::api::admin::download_clients::routes()
                .merge(crate::api::admin::downloads::routes()),
        )
    }

    async fn on_enable(&self, host: Arc<dyn HostCtx>) {
        // Everything the Downloads module needs at (re)enable lives here, so the
        // binary shell never seeds rows or spawns the monitor: seed the embedded
        // client row, start the engine, flip disable-paused rows back to active,
        // and ensure the resident monitor is running (spawned once). The VPN bridge
        // is its own module (ordered first by the dependency graph), so its SOCKS5
        // is already up. Awaited (not detached) so a following disable cannot race.
        if let Some(downloads) = service::<DownloadManager>(host.as_ref()) {
            downloads.seed_embedded_client(host.as_ref());
            downloads.start_rqbit(host.as_ref()).await;
            downloads.resume_after_enable(host.as_ref());
            downloads.ensure_monitor(host.clone());
        }
    }

    async fn on_disable(&self, host: Arc<dyn HostCtx>) {
        // Tear the engine down entirely (session stopped, active downloads paused)
        // so nothing is left transferring or seeding while disabled.
        if let Some(downloads) = service::<DownloadManager>(host.as_ref()) {
            downloads.disable_embedded(host.as_ref());
        }
    }
}
