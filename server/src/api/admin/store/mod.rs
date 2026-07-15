//! The admin module Store: browse a registry catalog, install (with automatic
//! dependency resolution + checksum verification), update and uninstall
//! runtime `.lmod` modules. Admin-gated (`settings.manage`); installing native
//! code is an admin-trust action.
//!
//! A "registry" is any static host serving a catalog index (see [`catalog`])
//! plus the `.lmod` files it points at. The default is the `modules.json` the
//! release workflow attaches to this repo's GitHub Releases, so the Store is
//! GitHub-backed out of the box; the `moduleRegistryUrl` setting points it at
//! any other registry (a third-party repo's releases, GitHub Pages, a NAS).

mod catalog;
mod install;

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use luma_module_supervisor::Supervisor;
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::state::SharedState;

/// Max bundle size (a native module binary + a small frontend bundle).
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;

/// Default module registry: the machine-readable index of `.lmod` bundles the
/// release workflow attaches to every GitHub Release of this repo.
/// `releases/latest/download/...` is a stable URL that always resolves to the
/// newest release's asset. Overridable via the `moduleRegistryUrl` setting.
const DEFAULT_REGISTRY: &str =
    "https://github.com/maxscharwath/luma/releases/latest/download/modules.json";

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/store/install", post(install_upload).layer(DefaultBodyLimit::max(MAX_BUNDLE_BYTES)))
        .route("/store/install-url", post(install_url))
        .route("/store/install-id", post(install_id))
        .route("/store/catalog", get(catalog_view))
        .route("/store/{id}", delete(uninstall))
}

#[derive(serde::Deserialize)]
struct InstallUrl {
    url: String,
    /// Expected SHA-256 of the bundle, when known (the registry catalog pins
    /// one per artifact). Verified before install; omitted = unverified.
    #[serde(default)]
    sha256: Option<String>,
}

/// Install a module straight from a URL. Verified against `sha256` when given.
async fn install_url(
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    Json(body): Json<InstallUrl>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let manifest = sup
        .install_from_url(&body.url, body.sha256.as_deref())
        .await
        .map_err(|e| bad(&format!("install failed: {e:#}")))?;
    Ok(Json(json!({
        "id": manifest.get("id"),
        "name": manifest.get("name"),
        "version": manifest.get("version"),
    }))
    .into_response())
}

#[derive(serde::Deserialize)]
struct InstallId {
    id: String,
}

/// One-click Store install/update by module id: resolves the module (and any
/// missing hard dependencies) against the registry catalog, checks server
/// compatibility + platform, downloads with checksum verification, and
/// installs everything in dependency order.
async fn install_id(
    State(state): State<SharedState>,
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    Json(body): Json<InstallId>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let report = install::install_with_deps(&state, &sup, &body.id)
        .await
        .map_err(|e| bad(&format!("install failed: {e:#}")))?;
    Ok(Json(report).into_response())
}

/// The registry catalog the Store lists, fetched server-side (no CORS, one
/// registry URL) and enriched per module with this server's verdict: the
/// artifact matching its build target, installed version, update flag, and
/// compatibility + reason. An unreachable registry is NOT an HTTP error: the
/// response carries `registryUrl` + `error` and an empty module list, so the
/// Store UI can show what failed and offer to fix the URL.
async fn catalog_view(
    State(state): State<SharedState>,
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let url = catalog::registry_url(&state);
    let body = match catalog::fetch(&sup, &url).await {
        Ok(modules) => catalog::enriched(&state, &modules, &url),
        Err(e) => catalog::unreachable(&url, &e),
    };
    Ok(Json(body).into_response())
}

/// Install an uploaded `.lmod` (raw request body). The manual escape hatch:
/// no registry, no checksum to verify against (the upload IS the source).
async fn install_upload(
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

#[derive(serde::Deserialize, Default)]
struct UninstallQuery {
    /// Skip the dependents guard (the UI asks for explicit confirmation).
    #[serde(default)]
    force: bool,
}

async fn uninstall(
    State(state): State<SharedState>,
    Extension(sup): Extension<Arc<Supervisor>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Query(q): Query<UninstallQuery>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Dependents guard: removing a module other enabled modules hard-depend on
    // would break them at their next port call. Surface who needs it instead.
    if !q.force {
        let dependents: Vec<String> = luma_module_kernel::manifests(&state)
            .into_iter()
            .filter(|m| m.id != id && m.depends_on.iter().any(|d| d.id == id))
            .filter(|m| luma_engine::modules::module_enabled(&state.settings, &m.id))
            .map(|m| m.id)
            .collect();
        if !dependents.is_empty() {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "'{id}' is required by: {}. Disable or uninstall those first, or retry with force=true.",
                    dependents.join(", ")
                ),
            )
                .into_response());
        }
    }
    tokio::task::spawn_blocking(move || sup.uninstall(&id))
        .await
        .map_err(|_| bad("uninstall task panicked"))?
        .map_err(|e| bad(&format!("uninstall failed: {e:#}")))?;
    Ok(Json(json!({ "ok": true })).into_response())
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}
