//! Remote access admin API (`/api/admin/remote`): configure the managed
//! Cloudflare Tunnel connector.
//!
//! The tunnel token is a secret stored server-side and never returned; the GET
//! exposes only `hasToken`, and a blank token on save keeps the stored one
//! (mirrors the LLM API-key handling). Writes are gated by `SettingsManage`.
//!
//! There is a single control: `enabled`. The server's reconcile loop (see
//! [`crate::services::remote`]) makes the running connector match it, so saving
//! is non-blocking (no waiting on a cloudflared launch/download) and disabling
//! always brings the connector down.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::services::settings;
use crate::state::SharedState;

/// Remote-access config. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/remote", get(get_remote).put(save_remote))
}

/// Config (token masked) + live connector status. Shared by both handlers so the
/// UI always gets the current picture back.
async fn status_value(state: &SharedState) -> serde_json::Value {
    let s = &state.settings;
    let st = state.remote.status().await;
    json!({
        "enabled": settings::remote_access_enabled(s),
        "url": settings::public_url(s),
        "hasToken": !settings::remote_access_token(s).trim().is_empty(),
        "status": serde_json::to_value(&st).unwrap_or_default(),
    })
}

/// `GET /api/admin/remote` → current config (token masked) + live status.
pub async fn get_remote(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    Ok(Json(status_value(&state).await).into_response())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteSaveBody {
    pub enabled: bool,
    pub url: String,
    /// Blank/omitted → keep the stored token.
    pub token: Option<String>,
}

/// `PUT /api/admin/remote` → persist config, then kick one reconcile so the
/// connector starts/stops immediately. Non-blocking (a launch/download runs in
/// the background). Returns the fresh status.
pub async fn save_remote(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<RemoteSaveBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Only overwrite the secret when a non-blank value was actually typed.
    let token = body.token.as_deref().map(str::trim).filter(|t| !t.is_empty());
    settings::set_remote_config(&state.settings, &state.db, body.enabled, &body.url, token);
    state.remote.reconcile(&state).await;
    Ok(Json(status_value(&state).await).into_response())
}
