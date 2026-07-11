//! Download-engine sub-modules: backend-only capability modules (no page, no
//! routes) that each provide one `download-client` factory to the Downloads
//! module's registry. Toggling one on/off registers / unregisters its kind, so
//! it appears in or drops out of the download-client picker. They `dependsOn`
//! the Downloads module (`dev.luma.torrents`), which owns the registry. `rqbit`
//! stays part of Downloads itself (the embedded engine), so it has no sub-module.

use crate::state::SharedState;

use super::ServerModule;

/// The Transmission RPC sub-engine (registry kind `transmission`).
pub struct TransmissionEngine;

impl ServerModule for TransmissionEngine {
    fn id(&self) -> &'static str {
        "dev.luma.engine.transmission"
    }
    fn on_enable(&self, state: &SharedState) {
        state.downloads.set_client_kind_enabled("transmission", true);
    }
    fn on_disable(&self, state: &SharedState) {
        state.downloads.set_client_kind_enabled("transmission", false);
    }
}

/// The qBittorrent WebUI sub-engine (registry kind `qbittorrent`).
pub struct QbittorrentEngine;

impl ServerModule for QbittorrentEngine {
    fn id(&self) -> &'static str {
        "dev.luma.engine.qbittorrent"
    }
    fn on_enable(&self, state: &SharedState) {
        state.downloads.set_client_kind_enabled("qbittorrent", true);
    }
    fn on_disable(&self, state: &SharedState) {
        state.downloads.set_client_kind_enabled("qbittorrent", false);
    }
}
