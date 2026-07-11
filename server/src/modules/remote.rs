//! The Remote access module (backend glue): the connector service + its routes
//! now live in the `luma-remote` crate (over the `HostCtx` seam). This binding
//! injects the live `RemoteAccess` (held on `AppState`) into the crate's generic
//! routes and gates them behind the module's enabled flag.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct RemoteModule;

impl ServerModule for RemoteModule {
    fn id(&self) -> &'static str {
        // Matches server/modules/remote/module.json.
        "dev.luma.remote"
    }

    fn admin_routes(&self, state: &SharedState) -> Router<SharedState> {
        luma_remote::routes::<SharedState>(state.remote.clone())
    }
}
