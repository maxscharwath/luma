//! Settings management: the grouped settings schema (+ current values) and a
//! patch endpoint that persists changes to the settings store.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::infra::events::ServerEvent;
use crate::model::Permission;
use crate::services::settings;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// Admin settings. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/settings", get(get_settings).put(put_settings))
}

#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    #[serde(default)]
    pub view: Option<String>,
}

/// `GET /api/admin/settings?view=general|network|transcoder` → grouped schema +
/// current values.
pub async fn get_settings(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(q): Query<SettingsQuery>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let view = q.view.unwrap_or_else(|| "general".into());
    let groups = settings::groups(&view, &state.settings, &state.config, super::user_locale(&user));
    Ok(Json(crate::api::dto::SettingsView { view, groups }).into_response())
}

/// `PUT /api/admin/settings` body = `{ key: value, … }` → persist a patch.
pub async fn put_settings(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(patch): Json<BTreeMap<String, Value>>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let written = state.settings.set_patch(&state.db, patch);
    // The HLS engine caches its disk budget; refresh it so a new
    // `transcodeCacheLimit` takes effect live (next reaper sweep) without a restart.
    if written.iter().any(|k| k == "transcodeCacheLimit") {
        state.hls.set_cache_budget(settings::transcode_cache_limit_bytes(&state.settings));
    }
    // The ffmpeg concurrency gate caches its budget; refresh it so a new
    // `mediaConcurrency` throttles (or opens up) background media work live.
    if written.iter().any(|k| k == "mediaConcurrency") {
        crate::infra::ffmpeg_gate::set_capacity(settings::media_workers(&state.settings));
    }
    state.events.publish(ServerEvent::SettingsUpdated);
    Ok(Json(json!({ "updated": written })).into_response())
}
