//! The Indexers module (backend): gates the `/api/admin/indexers` routes behind
//! its enabled flag, so a disabled Indexers module 404s its whole admin surface
//! (the page + routes vanish together). The manifest + native Cardigann engine
//! live in the `luma_indexer` crate; this only adds the route gate. Searches are
//! invoked on demand, so there is no long-running service to start/stop.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct IndexersModule;

impl ServerModule for IndexersModule {
    fn id(&self) -> &'static str {
        // Matches server/modules/indexer/module.json.
        "dev.luma.indexer"
    }

    fn admin_routes(&self) -> Router<SharedState> {
        crate::api::admin::indexers::routes()
    }
}
