//! Subtitle-provider admin API (`/api/admin/subtitles*`) backing the admin
//! "Subtitles" page: read the configured providers (secrets masked), save the
//! list + default, and probe a provider. Mirrors the LLM admin
//! ([`super::llm`]): blank secrets on save keep the stored values, and `test`
//! falls back to a saved provider's key by id.

use std::collections::HashSet;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::dto::{SubtitleProviderView, SubtitleProvidersConfig};
use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::services::settings::{self, SubtitleProvider};
use crate::services::subtitles::Creds;
use crate::state::SharedState;

/// `GET /api/admin/subtitles` → configured providers + default id (secrets never
/// returned, only `hasApiKey` / `hasPassword`).
pub async fn get_subtitles(State(state): State<SharedState>, AuthUser(user): AuthUser) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let s = &state.settings;
    let default_id = settings::default_subtitle_provider(s).map(|p| p.id).unwrap_or_default();
    let providers = settings::subtitle_providers(s)
        .into_iter()
        .map(|p| SubtitleProviderView {
            id: p.id,
            name: p.name,
            kind: p.kind,
            base_url: p.base_url,
            model: p.model,
            username: p.username,
            has_api_key: !p.api_key.trim().is_empty(),
            has_password: !p.password.trim().is_empty(),
        })
        .collect();
    Ok(Json(SubtitleProvidersConfig { default_id, providers, whisper_local: cfg!(feature = "whisper-local") }).into_response())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SubtitleSaveBody {
    pub default_index: usize,
    pub providers: Vec<SubtitleProviderInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SubtitleProviderInput {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub username: String,
    /// Blank/omitted → keep the stored secret for this provider id.
    pub api_key: Option<String>,
    pub password: Option<String>,
}

/// `PUT /api/admin/subtitles` → persist the provider list + default selection.
/// Dedicated (not the generic settings PUT) so blank secrets merge with stored
/// ones. The default is identified by **index** (new providers carry no id).
pub async fn save_subtitles(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SubtitleSaveBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Assign each new (blank-id) provider the lowest free `p{n}` not already used.
    let mut taken: HashSet<String> =
        body.providers.iter().map(|p| p.id.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let mut next_free = 0usize;
    let providers: Vec<SubtitleProvider> = body
        .providers
        .into_iter()
        .map(|p| SubtitleProvider {
            id: if p.id.trim().is_empty() {
                loop {
                    let cand = format!("p{next_free}");
                    next_free += 1;
                    if taken.insert(cand.clone()) {
                        break cand;
                    }
                }
            } else {
                p.id
            },
            name: p.name,
            kind: if p.kind.trim().is_empty() { "opensubtitles".into() } else { p.kind },
            base_url: p.base_url,
            model: p.model,
            username: p.username,
            api_key: p.api_key.unwrap_or_default(),
            password: p.password.unwrap_or_default(),
        })
        .collect();
    let default_id = providers.get(body.default_index).map(|p| p.id.clone()).unwrap_or_default();
    settings::set_subtitle_providers(&state.settings, &state.db, providers, &default_id);
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SubtitleTestBody {
    pub id: Option<String>,
    pub api_key: Option<String>,
}

/// `POST /api/admin/subtitles/test` → `{ ok, message }`, probing the provider with
/// a trivial search. A blank key falls back to the saved provider's stored key.
pub async fn test_subtitles(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SubtitleTestBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let saved = body
        .id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|id| settings::subtitle_providers(&state.settings).into_iter().find(|p| p.id == id))
        .or_else(|| settings::default_subtitle_provider(&state.settings));
    let api_key = body
        .api_key
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| saved.map(|p| p.api_key).unwrap_or_default());
    if api_key.trim().is_empty() {
        return Ok(Json(json!({ "ok": false, "message": "no API key set" })).into_response());
    }
    let creds = Creds { os_api_key: api_key, ..Default::default() };
    let (ok, message) = tokio::task::spawn_blocking(move || {
        let hits = crate::services::subtitles::search(&creds, "Matrix", None, &[]);
        if hits.is_empty() {
            (false, "no results (check the API key)".to_string())
        } else {
            (true, format!("OK — {} sample results", hits.len()))
        }
    })
    .await
    .unwrap_or((false, "probe failed".to_string()));
    Ok(Json(json!({ "ok": ok, "message": message })).into_response())
}
