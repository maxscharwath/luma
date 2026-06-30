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
mod subtitles;
mod settings;
mod stats;
mod storage;
mod users;

pub use backup::{export_backup, import_backup, MAX_BACKUP_BYTES};
pub use jobs::{cancel_job, job_detail, list_jobs, run_job, run_logs, update_job};
pub use llm::{get_llm, llm_models, save_llm, test_llm};
pub use subtitles::{get_subtitles, save_subtitles, test_subtitles};
pub use libraries::{create_library, delete_library, list_libraries, scan_library, update_library};
pub use settings::{get_settings, put_settings};
pub use stats::{history, overview, top_users};
pub use storage::{clear_cache, reset_metadata, storage};
pub use users::{delete_user, list_users, update_user};

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::error::lerr;
use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::i18n;
use crate::infra::events::ServerEvent;
use crate::model::{Permission, User};
use crate::state::SharedState;

// ----- guards -----------------------------------------------------------------

/// The admin's account locale. Admin endpoints are always authenticated, so the
/// (account-synced) preference is the right source for server-rendered strings
/// no `Accept-Language` needed. Falls back to the default for an unset/unknown
/// preference.
fn user_locale(user: &User) -> &'static str {
    user.language
        .as_deref()
        .and_then(i18n::normalize)
        .unwrap_or(i18n::DEFAULT_LOCALE)
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
    if user.can(Permission::UsersManage)
        || user.can(Permission::LibraryManage)
        || user.can(Permission::SettingsManage)
    {
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
        uptime_sec: crate::process_started().elapsed().as_secs(),
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

