//! TMDB metadata endpoints: details + IDs for one item or show. Results are
//! cached; returns 503 when `KROMA_TMDB_API_KEY` is unset, 404 on no match.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::error::json_error;
use crate::api::util::{blocking, query};
use crate::db;
use crate::infra::metadata::{self, Target};
use crate::model::Kind;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// TMDB-enriched metadata for shows and items.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/shows/{id}/metadata", get(show_metadata))
        .route("/items/{id}/metadata", get(item_metadata))
}

/// `GET /api/items/:id/metadata` → TMDB details + IDs for one item.
///
/// Movies resolve against TMDB movies; episodes resolve against the parent show
/// (TV). Results are cached. Returns 503 if `KROMA_TMDB_API_KEY` is unset, 404 if
/// the item is unknown or TMDB has no match.
pub async fn item_metadata(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let api_key = require_tmdb_key(&state)?;

    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;

    // Episodes inherit their show's identity for the lookup. `item` is owned and
    // unused afterwards, so move its strings out rather than cloning.
    let year = item.year;
    let (target, title) = if item.kind == Kind::Episode {
        (Target::Tv, item.show_title.unwrap_or(item.title))
    } else {
        (Target::Movie, item.title)
    };

    resolve_metadata(state, api_key, target, title, year).await
}

/// `GET /api/shows/:id/metadata` → TMDB details + IDs for one show.
pub async fn show_metadata(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let api_key = require_tmdb_key(&state)?;

    let show = query(&state.db, move |pool| db::get_show(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "show not found"))?
        .show;

    resolve_metadata(state, api_key, Target::Tv, show.title, show.year).await
}

/// The configured TMDB key, or a ready 503 telling the operator to set it. Shared
/// by the two metadata handlers.
fn require_tmdb_key(state: &SharedState) -> Result<String, Response> {
    state.config.tmdb_api_key.clone().ok_or_else(|| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "metadata disabled: set KROMA_TMDB_API_KEY",
        )
    })
}

/// Shared tail for the two metadata handlers: run the (blocking) TMDB lookup off
/// the async runtime and shape the JSON / 404 response.
async fn resolve_metadata(
    state: SharedState,
    api_key: String,
    target: Target,
    title: String,
    year: Option<u32>,
) -> Result<Response, Response> {
    let language = crate::services::settings::metadata_language(&state.settings, &state.config);
    let result = blocking(move || {
        Ok(metadata::lookup(
            &state.metadata_cache,
            &api_key,
            &language,
            target,
            &title,
            year,
        ))
    })
    .await?;

    let resp = match result {
        Some(meta) => Json(meta).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "no metadata match"),
    };
    Ok(resp)
}
