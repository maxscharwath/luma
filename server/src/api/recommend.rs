//! Recommendation rows backed by content embeddings (see [`crate::db::vectors`]).
//! Read-only. "For You" is Bearer-scoped to the caller (it reads their watch
//! history); "similar" is public (similarity doesn't depend on the user).

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::db;
use crate::model::MediaItem;
use crate::state::SharedState;

/// Titles per row.
const ROW_LEN: usize = 30;

/// `GET /api/for-you` (Bearer) → `MediaItem[]` — content-based picks from the
/// caller's watch history. Empty until they've watched something embeddable.
pub async fn for_you(State(state): State<SharedState>, AuthUser(user): AuthUser) -> Response {
    match query(&state.db, move |pool| db::recommended_for(&pool, &user.id, ROW_LEN)).await {
        Ok(items) => Json(items).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/items/:id/similar` → `MediaItem[]` — "more like this" for a title.
pub async fn similar(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    match query(&state.db, move |pool| db::similar_items(&pool, &id, ROW_LEN)).await {
        Ok(items) => Json(items).into_response(),
        Err(resp) => resp,
    }
}

#[derive(Deserialize)]
pub struct ThemedParams {
    #[serde(default)]
    q: String,
}

/// `GET /api/themed?q=…` → `MediaItem[]` — zero-shot themed row: embeds the
/// free-text phrase with the process-wide embedder and returns the nearest
/// titles. Public. Empty `q` → empty row (no implicit "everything").
pub async fn themed(
    State(state): State<SharedState>,
    Query(params): Query<ThemedParams>,
) -> Response {
    let q = params.q.trim().to_string();
    if q.is_empty() {
        return Json(Vec::<MediaItem>::new()).into_response();
    }
    // Embed + search together on the blocking pool thread (embedding is CPU work).
    let embedder = state.embedder.clone();
    match query(&state.db, move |pool| {
        let vec = embedder.embed(&q);
        db::themed_items(&pool, &vec, ROW_LEN, embedder.relevance_floor())
    })
    .await
    {
        Ok(items) => Json(items).into_response(),
        Err(resp) => resp,
    }
}
