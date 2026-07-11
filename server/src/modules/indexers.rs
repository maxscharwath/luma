//! The Indexers module (backend glue): the manifest, native Cardigann engine,
//! admin orchestration AND routes all live in the `luma_indexer` crate now (over
//! the HostCtx seam). This binding gates the crate's routes behind the module's
//! enabled flag, so a disabled Indexers module 404s its whole admin surface.

use axum::Router;

use crate::state::SharedState;

use super::ServerModule;

pub struct IndexersModule;

impl ServerModule for IndexersModule {
    fn id(&self) -> &'static str {
        // Matches server/modules/indexer/module.json.
        "dev.luma.indexer"
    }

    fn admin_routes(&self, _state: &SharedState) -> Option<Router<SharedState>> {
        Some(luma_indexer::routes::routes::<SharedState>())
    }
}
