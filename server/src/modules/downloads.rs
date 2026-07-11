//! The Downloads module (backend): the reference `ServerModule`. It owns the
//! download-client / downloads-queue / VPN admin routes and the librqbit engine
//! lifecycle, so disabling it 404s those routes and stops the running engine.
//! Its download sub-engines (rqbit / transmission / qBittorrent) plug into the
//! `DownloadClientRegistry` in `luma_torrent`.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct DownloadsModule;

impl ServerModule for DownloadsModule {
    fn id(&self) -> &'static str {
        luma_torrent::MODULE_ID
    }

    fn admin_routes(&self) -> Router<SharedState> {
        crate::api::admin::download_clients::routes()
            .merge(crate::api::admin::downloads::routes())
            .merge(crate::api::admin::vpn::routes())
    }

    fn on_enable(&self, state: &SharedState) {
        // Mirror the boot sequence: VPN bridge first (so the engine's SOCKS5 URL
        // points at a live proxy), then the engine, then flip the rows disable
        // paused back to active. start_rqbit is async, so run it detached.
        let state = state.clone();
        tokio::spawn(async move {
            state.vpn.apply(&state).await;
            state.downloads.start_rqbit(&state).await;
            state.downloads.resume_after_enable(&state);
        });
    }

    fn on_disable(&self, state: &SharedState) {
        // Tear the engine down entirely (session stopped, active downloads
        // paused) so nothing is left transferring or seeding while disabled.
        state.downloads.disable_embedded(state);
    }
}
