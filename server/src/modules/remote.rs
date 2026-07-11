//! The Remote access module (backend): gates the `/api/admin/remote` routes
//! behind its enabled flag, so a disabled module 404s its whole admin surface
//! (page + routes vanish together). The connector lifecycle is driven by the
//! remote-access enable toggle inside those routes (and its boot supervisor), so
//! this only adds the route gate.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct RemoteModule;

impl ServerModule for RemoteModule {
    fn id(&self) -> &'static str {
        // Matches server/modules/remote/module.json.
        "dev.luma.remote"
    }

    fn admin_routes(&self) -> Router<SharedState> {
        crate::api::admin::remote::routes()
    }
}
