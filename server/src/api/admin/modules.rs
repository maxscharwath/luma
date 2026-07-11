//! Module management for the admin console: list every module with its enabled
//! state + config schema + current config values, toggle enablement, and write
//! per-module config. State persists in the settings store under the
//! `moduleStates` blob (see `luma_engine::modules`).

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::state::SharedState;

/// Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/modules", get(list_modules))
        .route("/modules/:id/enabled", post(set_enabled))
        .route("/modules/:id/config", put(set_config))
}

/// A module manifest plus its runtime admin state.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModule {
    #[serde(flatten)]
    manifest: luma_module_sdk::ModuleManifest,
    enabled: bool,
    /// Current value per config field key (falls back to the field's default).
    config_values: BTreeMap<String, Value>,
    /// Runtime-installed (WASM) modules can be uninstalled; compile-time ones can't.
    removable: bool,
}

/// `GET /api/admin/modules` -> every module with enabled state + config values.
async fn list_modules(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let removable_ids: std::collections::HashSet<String> =
        state.wasm.read().map(|h| h.manifests().into_iter().map(|m| m.id).collect()).unwrap_or_default();
    let mods: Vec<AdminModule> = crate::modules::manifests(&state)
        .into_iter()
        .map(|m| {
            let enabled = luma_engine::modules::module_enabled(&state.settings, &m.id);
            let removable = removable_ids.contains(&m.id);
            let stored = luma_engine::modules::module_config(&state.settings, &m.id);
            let config_values = m
                .config
                .iter()
                .map(|f| {
                    let value = stored.get(&f.key).cloned().unwrap_or_else(|| {
                        f.default.clone().map(Value::from).unwrap_or(Value::Null)
                    });
                    (f.key.clone(), value)
                })
                .collect();
            AdminModule { manifest: m, enabled, config_values, removable }
        })
        .collect();
    Ok(Json(mods).into_response())
}

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

/// `POST /api/admin/modules/:id/enabled` `{ enabled }`.
async fn set_enabled(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<EnabledBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    luma_engine::modules::set_module_enabled(&state.settings, &state.db, &id, body.enabled);
    // Drive the backend module's lifecycle so the toggle actually starts/stops
    // its live services (e.g. the torrent engine), not just its listing flag.
    if let Some(module) = crate::modules::find_server(&id) {
        if body.enabled {
            module.on_enable(&state);
        } else {
            module.on_disable(&state);
        }
    }
    Ok(Json(json!({ "id": id, "enabled": body.enabled })).into_response())
}

/// `PUT /api/admin/modules/:id/config` body = `{ key: value, … }`.
async fn set_config(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(values): Json<BTreeMap<String, Value>>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Allow-list against the manifest's declared config keys, so a client can only
    // write fields the module actually defines (a typo or stale key is dropped
    // rather than polluting the stored config).
    let allowed: std::collections::HashSet<String> = crate::modules::manifests(&state)
        .into_iter()
        .find(|m| m.id == id)
        .map(|m| m.config.into_iter().map(|f| f.key).collect())
        .unwrap_or_default();
    let map: serde_json::Map<String, Value> =
        values.into_iter().filter(|(k, _)| allowed.contains(k)).collect();
    luma_engine::modules::set_module_config(&state.settings, &state.db, &id, map);
    Ok(Json(json!({ "id": id, "ok": true })).into_response())
}
