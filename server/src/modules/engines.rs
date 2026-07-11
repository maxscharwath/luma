//! Download-engine sub-modules: backend-only capability modules (no page, no
//! routes) that each provide one `download-client` factory to the Downloads
//! module's registry. Toggling one on/off registers / unregisters its kind, so
//! it appears in or drops out of the download-client picker. They `dependsOn`
//! the Downloads module (`dev.luma.torrents`), which owns the registry. `rqbit`
//! stays part of Downloads itself (the embedded engine), so it has no sub-module.

use crate::state::SharedState;

use super::ServerModule;

/// The Transmission RPC sub-engine, impl'd in the `luma-transmission` crate.
pub struct TransmissionEngine;

impl ServerModule for TransmissionEngine {
    fn id(&self) -> &'static str {
        "dev.luma.engine.transmission"
    }
    fn on_enable(&self, state: &SharedState) {
        state.downloads.register_engine(luma_transmission::register);
    }
    fn on_disable(&self, state: &SharedState) {
        state.downloads.unregister_engine(luma_transmission::KIND);
    }
}

/// The qBittorrent WebUI sub-engine, impl'd in the `luma-qbittorrent` crate.
pub struct QbittorrentEngine;

impl ServerModule for QbittorrentEngine {
    fn id(&self) -> &'static str {
        "dev.luma.engine.qbittorrent"
    }
    fn on_enable(&self, state: &SharedState) {
        state.downloads.register_engine(luma_qbittorrent::register);
    }
    fn on_disable(&self, state: &SharedState) {
        state.downloads.unregister_engine(luma_qbittorrent::KIND);
    }
}
