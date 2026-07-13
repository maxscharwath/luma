//! Runtime module install / uninstall for the admin Store.
//!
//! `POST /api/admin/store/install` takes a module bundle as the raw request body
//! -- a gzip-compressed `.lmod` (from `bun run modules:pack`) carrying the
//! module's native binary + `module.json` + `fe/` + icon -- and installs it into
//! the running server: the supervisor unpacks it under `<data>/modules/<id>/` and
//! spawns it. `DELETE /api/admin/store/:id` stops + removes it. Admin-gated
//! (`settings.manage`); installing arbitrary native code is an admin-trust action.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::{Extension, Json, Router};
use luma_module_supervisor::Supervisor;
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::state::SharedState;

/// Max bundle size (a native module binary + a small frontend bundle).
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/store/install", post(install).layer(DefaultBodyLimit::max(MAX_BUNDLE_BYTES)))
        .route("/store/:id", delete(uninstall))
}

async fn install(
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    body: Bytes,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    if body.is_empty() {
        return Err(bad("empty bundle"));
    }
    // Unpack + spawn is blocking; keep it off the async runtime.
    let manifest: Value = tokio::task::spawn_blocking(move || sup.install(&body))
        .await
        .map_err(|_| bad("install task panicked"))?
        .map_err(|e| bad(&format!("install failed: {e:#}")))?;
    Ok(Json(json!({
        "id": manifest.get("id"),
        "name": manifest.get("name"),
        "version": manifest.get("version"),
    }))
    .into_response())
}

async fn uninstall(
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    tokio::task::spawn_blocking(move || sup.uninstall(&id))
        .await
        .map_err(|_| bad("uninstall task panicked"))?
        .map_err(|e| bad(&format!("uninstall failed: {e:#}")))?;
    Ok(Json(json!({ "ok": true })).into_response())
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}
