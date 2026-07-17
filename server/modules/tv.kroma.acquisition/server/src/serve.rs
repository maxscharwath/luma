//! The out-of-process provider side of the acquisition search / grab surface:
//! the routes the acquisition `.kmod` serves for the core's
//! `/api/requests/:id/search` + `/grab` endpoints (via `AcquisitionSearchPort`).
//!
//! These live here (not in the generic bridge) because `grab` backgrounds the
//! slow engine add with the owned host -- the handler's `State<S>` gives it one,
//! which the generic bridge can't.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use kroma_module_sdk::host::HostCtx;
use serde::Deserialize;

/// The routes the acquisition sidecar mounts for its search port.
pub fn acqsearch_routes<S: HostCtx + Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/_port/acqsearch/search", post(search_h::<S>))
        .route("/_port/acqsearch/grab", post(grab_h::<S>))
}

/// Run a blocking provider call into the `Result<T, String>` wire envelope.
async fn blocking_env<T: Send + 'static>(
    job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Json<Result<T, String>> {
    Json(
        tokio::task::spawn_blocking(job)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| format!("{e:#}"))),
    )
}

#[derive(Deserialize)]
struct SearchReq {
    request_id: String,
}

async fn search_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Json(req): Json<SearchReq>,
) -> Json<Result<serde_json::Value, String>> {
    blocking_env(move || Ok(serde_json::to_value(crate::search::interactive_search(&host, &req.request_id)?)?))
        .await
}

#[derive(Deserialize)]
struct GrabReq {
    request_id: String,
    guid: String,
    indexer_id: String,
}

async fn grab_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Json(req): Json<GrabReq>,
) -> Json<Result<String, String>> {
    blocking_env(move || {
        let row = crate::search::grab_cached(&host, &req.request_id, &req.guid, &req.indexer_id)?;
        let id = row.id.clone();
        // Background the slow engine add (magnet resolve / .torrent fetch) so the
        // grab returns immediately, matching the core's former behavior. The grab
        // client's `activate` is a blocking HTTP call to the torrents sidecar, so a
        // plain thread (no runtime needed) is enough.
        std::thread::spawn(move || crate::downloads(&host).activate(&host, &row));
        Ok(id)
    })
    .await
}
