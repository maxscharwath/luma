//! Runtime module install / uninstall for the admin Store.
//!
//! `POST /api/admin/store/install` takes a module bundle as the raw request body
//! (module.json + optional module.wasm + fe/ + icon) -- a gzip-compressed `.lmod`
//! (from `bun run modules:pack`) or a raw `.tar` -- and installs it into the
//! running server; `DELETE /api/admin/store/:id` removes it. Admin-gated
//! (`settings.manage`). The uploaded WASM guest runs sandboxed under extism (no
//! ambient FS / network), but installing arbitrary code is an admin-trust action.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::{Json, Router};
use serde_json::json;

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::state::SharedState;

/// Max bundle size (wasm + a small frontend bundle). Relative to `/api/admin`.
const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/store/install", post(install).layer(DefaultBodyLimit::max(MAX_BUNDLE_BYTES)))
        .route("/store/:id", delete(uninstall))
}

async fn install(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    body: Bytes,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    if body.is_empty() {
        return Err(bad("empty bundle"));
    }
    // fs unpack + wasm instantiate are blocking; keep them off the async runtime.
    let wasm = state.wasm.clone();
    let manifest = tokio::task::spawn_blocking(move || {
        wasm.write().map_err(|_| anyhow::anyhow!("wasm host lock poisoned"))?.install(&body)
    })
    .await
    .map_err(|_| bad("install task panicked"))?
    .map_err(|e| bad(&format!("install failed: {e:#}")))?;
    Ok(Json(json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.version,
    }))
    .into_response())
}

async fn uninstall(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let wasm = state.wasm.clone();
    tokio::task::spawn_blocking(move || {
        wasm.write().map_err(|_| anyhow::anyhow!("wasm host lock poisoned"))?.uninstall(&id)
    })
    .await
    .map_err(|_| bad("uninstall task panicked"))?
    .map_err(|e| bad(&format!("uninstall failed: {e:#}")))?;
    Ok(Json(json!({ "ok": true })).into_response())
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}
