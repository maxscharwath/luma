//! LLM admin API (`/api/admin/llm*`) backing the dedicated IA / Intelligence
//! page: read the current config (key masked), list the models an endpoint
//! advertises, and probe a connection. The list/test handlers accept the values
//! the admin is *currently editing* (falling back to saved settings for any
//! omitted/blank field, notably the API key), so "Load models" / "Test" work
//! before the form is saved.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::dto::{LlmAdminConfig, LlmProviderView};
use crate::api::extract::AuthUser;
use crate::model::Permission;
use crate::services::settings::{self, LlmProvider, Settings};
use crate::state::SharedState;

/// `GET /api/admin/llm` → the configured providers + default id (keys never
/// returned, only `hasApiKey` per provider).
pub async fn get_llm(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let s = &state.settings;
    let default_id = settings::default_provider(s).map(|p| p.id).unwrap_or_default();
    let providers = settings::llm_providers(s)
        .into_iter()
        .map(|p| LlmProviderView {
            id: p.id,
            name: p.name,
            provider: p.provider,
            base_url: p.base_url,
            model: p.model,
            has_api_key: !p.api_key.trim().is_empty(),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
            reasoning: p.reasoning,
        })
        .collect();
    Ok(Json(LlmAdminConfig {
        enabled: s.get_bool("llmEnabled", false),
        default_id,
        providers,
    })
    .into_response())
}

/// Save body the full provider set + global enable + default provider index. A
/// provider's `apiKey` is optional: omit/blank to keep the stored secret (see
/// `set_llm`). The default is identified by **index** (not id), because new
/// providers carry no id yet the server assigns one on save.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LlmSaveBody {
    pub enabled: bool,
    pub default_index: usize,
    pub providers: Vec<LlmProviderInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LlmProviderInput {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Blank/omitted → keep the previously stored key for this provider id.
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: i64,
    pub reasoning: bool,
}

/// `PUT /api/admin/llm` → persist the provider list, default selection and the
/// global enable flag. Dedicated (not the generic settings PUT) so blank API
/// keys can be merged with the stored secrets instead of wiping them.
pub async fn save_llm(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<LlmSaveBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // New providers send a blank id; the server owns id assignment (never the
    // client). Keep existing ids so their stored key/default survive, and give
    // each new provider the lowest free `p{n}` not already taken by another
    // provider here. (A plain index-based `p{i}` collides after a delete/reorder:
    // a new provider can land on an index whose old id another provider still
    // holds, silently merging the two onto one stored key.)
    let mut taken: std::collections::HashSet<String> = body
        .providers
        .iter()
        .map(|p| p.id.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut next_free = 0usize;
    let providers: Vec<LlmProvider> = body
        .providers
        .into_iter()
        .map(|p| LlmProvider {
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
            provider: if p.provider.trim().is_empty() { "openai".into() } else { p.provider },
            base_url: p.base_url,
            model: p.model,
            api_key: p.api_key.unwrap_or_default(),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
            reasoning: p.reasoning,
        })
        .collect();
    // Resolve the default from its index now that ids are assigned.
    let default_id = providers.get(body.default_index).map(|p| p.id.clone()).unwrap_or_default();
    settings::set_llm(&state.settings, &state.db, body.enabled, providers, &default_id);
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Probe body the in-progress form values, plus the provider `id` being edited
/// so a blank field (notably the masked API key) can fall back to *that* saved
/// provider's stored value rather than the legacy flat keys.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProbeBody {
    pub id: Option<String>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

fn resolved(settings: &Settings, body: &ProbeBody) -> (String, String, String, String) {
    // The saved provider this probe edits (by id), else the default its stored
    // values back-fill any blank form field. Multi-provider keys live in
    // `llmProviders[].api_key`, so reaching them by id is the only way the probe
    // can reuse a masked key (the legacy flat keys are empty on these installs).
    let saved = body
        .id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|id| settings::llm_providers(settings).into_iter().find(|p| p.id == id))
        .or_else(|| settings::default_provider(settings));
    let (s_provider, s_base, s_model, s_key) = saved
        .map(|p| (p.provider, p.base_url, p.model, p.api_key))
        .unwrap_or_default();
    let pick = |v: &Option<String>, fallback: String, default: &str| {
        v.as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| if fallback.trim().is_empty() { default.to_string() } else { fallback })
    };
    (
        pick(&body.provider, s_provider, "openai"),
        // base_url: take the typed value verbatim if present (may legitimately be
        // empty for Anthropic), else the saved provider's.
        body.base_url.clone().unwrap_or(s_base),
        pick(&body.model, s_model, ""),
        pick(&body.api_key, s_key, ""),
    )
}

/// `POST /api/admin/llm/models` → `{ models: string[] }` advertised by the
/// endpoint (or `{ models: [], error }` so the UI can show why it failed).
pub async fn llm_models(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ProbeBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let (provider, base_url, _model, api_key) = resolved(&state.settings, &body);
    let result =
        tokio::task::spawn_blocking(move || crate::infra::llm::list_models(&provider, &base_url, &api_key))
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("probe task failed")));
    match result {
        Ok(models) => Ok(Json(json!({ "models": models })).into_response()),
        Err(e) => Ok(Json(json!({ "models": [], "error": format!("{e:#}") })).into_response()),
    }
}

/// `POST /api/admin/llm/test` → `{ ok, message }`, probing a trivial completion.
pub async fn test_llm(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ProbeBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let (provider, base_url, model, api_key) = resolved(&state.settings, &body);
    let (ok, message) = tokio::task::spawn_blocking(move || {
        match crate::infra::llm::build_http(&provider, &base_url, &model, &api_key, 0.7, false) {
            None => (false, "not configured set a base URL and model".to_string()),
            Some(llm) => match llm.complete("You are a connectivity check. Reply with exactly: OK", "ping", 16) {
                Ok(reply) => (true, format!("{} → {}", llm.describe(), reply.trim())),
                Err(e) => (false, format!("{e:#}")),
            },
        }
    })
    .await
    .unwrap_or((false, "probe task failed".to_string()));
    Ok(Json(json!({ "ok": ok, "message": message })).into_response())
}
