//! The out-of-process provider side of the embedder: HTTP routes the vector
//! `.lmod` serves so the core (and any module) can embed text over the port
//! bridge. `embed_batch` is the important one: a catalog-wide reembed sends every
//! document in ONE request, so the sidecar stays fast despite living in another
//! process (per-item IPC would be thousands of round-trips).

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::Embedder;

/// The routes the vector sidecar mounts for the embedder port. Generic over the
/// process state `S` (the runtime's `RemoteHost`); the embedder is stateless
/// compute, so it rides in an `Extension` rather than needing the host.
pub fn embedder_routes<S: Clone + Send + Sync + 'static>(emb: Arc<dyn Embedder>) -> Router<S> {
    Router::new()
        .route("/_port/embedder/embed", post(embed_h))
        .route("/_port/embedder/embed_batch", post(embed_batch_h))
        .route("/_port/embedder/meta", get(meta_h))
        .layer(Extension(emb))
}

#[derive(Deserialize)]
struct EmbedReq {
    text: String,
}

async fn embed_h(
    Extension(emb): Extension<Arc<dyn Embedder>>,
    Json(req): Json<EmbedReq>,
) -> Json<Vec<f32>> {
    // Embedding is CPU work; run it off the async runtime.
    let v = tokio::task::spawn_blocking(move || emb.embed(&req.text)).await.unwrap_or_default();
    Json(v)
}

#[derive(Deserialize)]
struct EmbedBatchReq {
    texts: Vec<String>,
}

async fn embed_batch_h(
    Extension(emb): Extension<Arc<dyn Embedder>>,
    Json(req): Json<EmbedBatchReq>,
) -> Json<Vec<Vec<f32>>> {
    let out = tokio::task::spawn_blocking(move || {
        req.texts.iter().map(|t| emb.embed(t)).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn meta_h(Extension(emb): Extension<Arc<dyn Embedder>>) -> Json<serde_json::Value> {
    Json(json!({ "dim": emb.dim(), "relevance_floor": emb.relevance_floor() }))
}
