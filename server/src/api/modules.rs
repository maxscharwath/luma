//! Module registry endpoints: the manifest list (`GET /api/modules`) the
//! frontend registry reconciles against, and each module's packaged icon
//! (`GET /api/modules/:id/icon`).
//!
//! The icon route is PUBLIC: an `<img>` can't attach a bearer, so it is merged
//! outside the content auth layer (like the theme / image endpoints).

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::SharedState;

/// Auth-gated: the module manifest list. Relative to `/api`.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/modules", get(list))
}

/// Public: packaged module icons (fetched by `<img>`, which can't send a bearer).
pub fn public_routes() -> Router<SharedState> {
    Router::new().route("/modules/:id/icon", get(icon))
}

/// A manifest plus its admin enabled flag (persisted in the `moduleStates`
/// settings blob, default true). The frontend hides modules with `enabled: false`.
#[derive(Serialize)]
struct ListedModule {
    #[serde(flatten)]
    manifest: luma_module_sdk::ModuleManifest,
    enabled: bool,
}

/// The modules running on this server, in dependency order, each tagged with its
/// admin enabled state.
async fn list(State(state): State<SharedState>) -> impl IntoResponse {
    let mods: Vec<ListedModule> = crate::modules::manifests(&state)
        .into_iter()
        .map(|m| {
            let enabled = luma_engine::modules::module_enabled(&state.settings, &m.id);
            ListedModule { manifest: m, enabled }
        })
        .collect();
    Json(mods)
}

/// `GET /api/modules/:id/icon` -> the module's packaged `icon.svg` / `icon.png`
/// (compile-time or runtime-loaded).
async fn icon(State(state): State<SharedState>, Path(id): Path<String>) -> impl IntoResponse {
    match crate::modules::icon(&state, &id) {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
