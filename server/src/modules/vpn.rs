//! The VPN module (backend): owns the managed WireGuard bridge admin routes and
//! the bridge lifecycle. Disabling it 404s `/api/admin/vpn` and stops the bridge
//! child, so no tunnel is left running. The Downloads module `optionalDependsOn`
//! it (the engine's SOCKS5 points at this bridge), so enable VPN first.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct VpnModule;

impl ServerModule for VpnModule {
    fn id(&self) -> &'static str {
        "dev.luma.vpn"
    }

    fn admin_routes(&self) -> Router<SharedState> {
        crate::api::admin::vpn::routes()
    }

    fn on_enable(&self, state: &SharedState) {
        // Bring the bridge up from the stored config (no-op when unconfigured).
        // apply is async, so run it detached.
        let state = state.clone();
        tokio::spawn(async move {
            state.vpn.apply(&state).await;
        });
    }

    fn on_disable(&self, state: &SharedState) {
        // Tear the bridge down entirely so nothing is left tunnelling.
        let state = state.clone();
        tokio::spawn(async move {
            state.vpn.stop().await;
        });
    }
}
