//! The `TorznabPort` bridge (stateless — no `HostCtx`, the cleanest port).

use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Json, Router};
use kroma_module_sdk::ports::{Caps, IndexerEndpoint, Query, Release, TorznabPort};

use crate::{call, Resolver};

// --- Provider side (the torznab module process serves these) -----------------

/// Routes a provider mounts to expose its `TorznabPort` over HTTP. Merge into the
/// module process's router (see `kroma_module_runtime::serve`'s `extra`).
pub fn torznab_routes<S: Clone + Send + Sync + 'static>(engine: Arc<dyn TorznabPort>) -> Router<S> {
    Router::new()
        .route("/_port/torznab/caps", post(caps_h))
        .route("/_port/torznab/search", post(search_h))
        .layer(Extension(engine))
}

/// Map an `anyhow::Result` from a blocking port call into the wire envelope.
async fn run<T>(
    job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Json<Result<T, String>>
where
    T: Send + 'static,
{
    let out = tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| format!("{e:#}")));
    Json(out)
}

async fn caps_h(
    Extension(engine): Extension<Arc<dyn TorznabPort>>,
    Json(endpoint): Json<IndexerEndpoint>,
) -> Json<Result<Caps, String>> {
    run(move || engine.caps(&endpoint)).await
}

#[derive(serde::Deserialize)]
struct SearchReq {
    endpoint: IndexerEndpoint,
    query: Query,
    caps: Caps,
}

async fn search_h(
    Extension(engine): Extension<Arc<dyn TorznabPort>>,
    Json(req): Json<SearchReq>,
) -> Json<Result<Vec<Release>, String>> {
    run(move || engine.search(&req.endpoint, &req.query, &req.caps)).await
}

// --- Consumer side (whoever holds the indexer resolves this) -----------------

/// A `TorznabPort` that forwards to the torznab module process over localhost.
pub struct TorznabClient {
    resolve: Resolver,
}

impl TorznabClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

impl TorznabPort for TorznabClient {
    fn caps(&self, endpoint: &IndexerEndpoint) -> anyhow::Result<Caps> {
        call(&self.resolve, "torznab/caps", endpoint)
    }

    fn search(&self, endpoint: &IndexerEndpoint, query: &Query, caps: &Caps) -> anyhow::Result<Vec<Release>> {
        call(
            &self.resolve,
            "torznab/search",
            &serde_json::json!({ "endpoint": endpoint, "query": query, "caps": caps }),
        )
    }
}
