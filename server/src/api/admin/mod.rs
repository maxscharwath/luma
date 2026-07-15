//! Admin console API (`/api/admin/*`). Backs the "Admin Serveur" dashboard:
//! live sessions, system metrics, storage, users, libraries, settings and
//! analytics. Every route is gated by a capability (see [`require`] /
//! [`require_any_admin`]); reads need *any* admin capability, writes need the
//! specific one.
//!
//! Handlers are grouped per managed noun in the submodules below; the
//! server-status / live-sessions / metrics dashboard handlers and the shared
//! capability guards live here.

mod backup;
mod jobs;
mod libraries;
mod llm;
mod modules;
mod pipeline;
mod settings;
mod stats;
mod storage;
mod store;
mod users;

use std::sync::Arc;

use axum::extract::{OriginalUri, Path as AxPath, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use luma_module_supervisor::Supervisor;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::api::error::lerr;
use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::i18n;
use crate::infra::events::ServerEvent;
use crate::model::{Permission, User};
use crate::state::SharedState;

/// Compose the admin subtree. Nested under `/api/admin` by [`crate::api::router`],
/// so every path here is relative to that prefix. Each managed noun owns its
/// routes in its submodule; the dashboard handlers (status / sessions / metrics)
/// live in this file.
pub fn routes(state: SharedState) -> Router<SharedState> {
    // Core admin routers merged directly; each backend module's routers are
    // mounted behind its enabled-gate (404 when the module is disabled), so a
    // disabled module's whole admin surface disappears. The Downloads / VPN /
    // Indexers / Remote routers are modules now, so they are no longer merged
    // here -- they come in via `luma_module_kernel::mount_admin` below.
    let mut router = Router::new()
        .route("/server", get(server_info))
        .route("/sessions", get(sessions))
        .route("/sessions/{id}/stop", post(terminate_session))
        .route("/metrics", get(metrics))
        .merge(users::routes())
        .merge(libraries::routes())
        .merge(settings::routes())
        .merge(storage::routes())
        .merge(stats::routes())
        .merge(jobs::routes())
        .merge(llm::routes())
        .merge(modules::routes())
        .merge(store::routes())
        .merge(pipeline::routes())
        .merge(backup::routes());
    router = router.merge(luma_module_kernel::mount_admin(state.clone()));
    // Out-of-process modules with admin routes: anything not matched above is
    // reverse-proxied to the sidecar that owns the path's first segment (from its
    // manifest `adminPrefixes`), so a converted module's `/api/admin/<prefix>/*`
    // still works. Non-module unmatched paths get the proxy's 404. Explicit
    // wildcard routes rather than a `.fallback`: axum 0.8's `nest` drops the
    // nested router's fallback, and its matchit allows a param route to overlap
    // the static ones above (statics win), which 0.7 forbade.
    router
        .route("/{seg}", axum::routing::any(admin_module_proxy))
        .route("/{seg}/{*rest}", axum::routing::any(admin_module_proxy))
}

/// Reverse-proxy an unmatched `/api/admin/<seg>/*` to the module sidecar owning
/// `<seg>`; 404 otherwise.
async fn admin_module_proxy(
    Extension(sup): Extension<Arc<Supervisor>>,
    OriginalUri(uri): OriginalUri,
    req: Request,
) -> Response {
    let rest = uri.path().strip_prefix("/api/admin/").unwrap_or_default();
    let seg = rest.split('/').next().unwrap_or_default();
    match sup.admin_route_port(seg) {
        Some(port) => {
            let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
            luma_module_supervisor::proxy_to(port, &format!("/{rest}{query}"), req).await
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ----- guards -----------------------------------------------------------------

/// The admin's account locale. Admin endpoints are always authenticated, so the
/// (account-synced) preference is the right source for server-rendered strings
/// no `Accept-Language` needed. Falls back to the default for an unset/unknown
/// preference.
fn user_locale(user: &User) -> &'static str {
    i18n::user_locale(user)
}

fn require(user: &User, perm: Permission) -> Result<(), Response> {
    if user.can(perm) {
        Ok(())
    } else {
        Err(lerr(user_locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

/// Any management capability unlocks the read-only dashboard panels.
fn require_any_admin(user: &User) -> Result<(), Response> {
    if user.is_any_admin() {
        Ok(())
    } else {
        Err(lerr(user_locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

// ----- server status ----------------------------------------------------------

/// `GET /api/admin/server` → identity + uptime for the sidebar status card.
pub async fn server_info(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let hostname = sysinfo::System::host_name().unwrap_or_else(|| "luma".into());
    Ok(Json(crate::api::dto::ServerInfo {
        name: crate::services::settings::server_name(&state.settings),
        hostname,
        version: env!("CARGO_PKG_VERSION"),
        uptime_sec: luma_engine::process_started().elapsed().as_secs(),
        online: true,
        sessions: state.playback.list().len(),
    })
    .into_response())
}

// ----- live sessions ----------------------------------------------------------

/// `GET /api/admin/sessions` → live "En cours de lecture" sessions.
pub async fn sessions(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    Ok(Json(json!({ "sessions": state.playback.list() })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct TerminateBody {
    #[serde(default)]
    pub message: Option<String>,
}

/// `POST /api/admin/sessions/:id/stop` → terminate a live playback session. The
/// owning client (web/TV) receives a `playback.terminate` event over the WS bus,
/// stops the video, and shows `message` (empty → a localized default). Idempotent.
pub async fn terminate_session(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<TerminateBody>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    // Drop it from the registry (grace window blocks re-registration) + log it.
    if let Some(session) = state.playback.terminate(&id) {
        let _ = query(&state.db, move |pool| {
            crate::services::playback::record(&pool, &session);
            Ok(())
        })
        .await;
    }
    let message = body
        .message
        .map(|m| m.trim().chars().take(200).collect::<String>())
        .unwrap_or_default();
    state
        .events
        .publish(ServerEvent::PlaybackTerminate { session_id: id, message });
    state
        .events
        .publish(ServerEvent::PlaybackStopped { count: state.playback.list().len() });
    Ok(Json(json!({ "ok": true })).into_response())
}

// ----- metrics ----------------------------------------------------------------

/// `GET /api/admin/metrics` → CPU / RAM / bandwidth snapshot + history.
pub async fn metrics(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    Ok(Json(state.metrics.snapshot()).into_response())
}

