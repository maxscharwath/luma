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
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use luma_module_host::HostCtx;
use luma_module_supervisor::Supervisor;
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::state::SharedState;

/// Max bundle size (a native module binary + a small frontend bundle).
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;

/// Default module registry (a static `catalog.json` + `.lmod` files) the Store
/// browses; overridable via the `moduleRegistryUrl` setting.
const DEFAULT_REGISTRY: &str = "https://maxscharwath.github.io/luma-modules/catalog.json";

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/store/install", post(install).layer(DefaultBodyLimit::max(MAX_BUNDLE_BYTES)))
        .route("/store/install-url", post(install_url))
        .route("/store/catalog", get(catalog))
        .route("/store/:id", delete(uninstall))
}

#[derive(serde::Deserialize)]
struct InstallUrl {
    url: String,
}

/// Install a module straight from a registry URL (one-click from the Store).
async fn install_url(
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    Json(body): Json<InstallUrl>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let manifest =
        sup.install_from_url(&body.url).await.map_err(|e| bad(&format!("install failed: {e:#}")))?;
    Ok(Json(json!({
        "id": manifest.get("id"),
        "name": manifest.get("name"),
        "version": manifest.get("version"),
    }))
    .into_response())
}

/// The module registry catalog the Store lists (fetched server-side to avoid
/// CORS + centralize the registry URL). Falls back to the default registry.
async fn catalog(
    State(state): State<SharedState>,
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let url = {
        let u = state.setting_str("moduleRegistryUrl", DEFAULT_REGISTRY);
        if u.trim().is_empty() { DEFAULT_REGISTRY.to_string() } else { u }
    };
    let cat = sup.fetch_catalog(&url).await.map_err(|e| bad(&format!("registry unreachable: {e:#}")))?;
    Ok(Json(cat).into_response())
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
