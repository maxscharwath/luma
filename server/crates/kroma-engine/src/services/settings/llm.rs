//! Multi-provider LLM configuration persisted in the settings store. The admin
//! IA page registers several providers (a local Ollama, Claude, OpenRouter, …)
//! and marks one as the default used for generation. Mirrors `accessors`'
//! `library_defs`: the list lives under the `llmProviders` settings key, with a
//! one-time migration from the legacy single-provider flat `llm*` keys.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Pool;

use super::store::Settings;

/// One configured LLM endpoint. `api_key` is only ever populated server-side
/// it is never returned to the client (the admin DTO exposes `has_api_key`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProvider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// `openai` (OpenAI-compatible / Ollama) | `anthropic` | `openrouter`.
    #[serde(default = "default_kind")]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: i64,
    #[serde(default)]
    pub reasoning: bool,
}

fn default_kind() -> String {
    "openai".to_string()
}
fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> i64 {
    900
}

/// The configured providers. Parses the persisted `llmProviders` array; when it
/// is empty, migrates the legacy single-provider flat keys into one provider
/// (id `"default"`) if any were set, so existing installs keep working.
pub fn llm_providers(settings: &Settings) -> Vec<LlmProvider> {
    if let Ok(list) = serde_json::from_value::<Vec<LlmProvider>>(settings.get("llmProviders")) {
        if !list.is_empty() {
            return list;
        }
    }
    // Migration from the flat keys (pre multi-provider installs).
    let base_url = settings.get_str("llmBaseUrl", "");
    let model = settings.get_str("llmModel", "");
    let api_key = settings.get_str("llmApiKey", "");
    if base_url.trim().is_empty() && model.trim().is_empty() && api_key.trim().is_empty() {
        return Vec::new();
    }
    vec![LlmProvider {
        id: "default".to_string(),
        name: "Default".to_string(),
        provider: settings.get_str("llmProvider", "openai"),
        base_url,
        model,
        api_key,
        temperature: settings.get("llmTemperature").as_f64().unwrap_or(0.7) as f32,
        max_tokens: settings.get_i64("llmMaxTokens", 900),
        reasoning: settings.get_bool("llmReasoning", false),
    }]
}

/// The provider used for generation: the one whose id matches
/// `llmDefaultProvider`, else the first configured (or `None` if there are none).
pub fn default_provider(settings: &Settings) -> Option<LlmProvider> {
    let providers = llm_providers(settings);
    let default_id = settings.get_str("llmDefaultProvider", "");
    providers
        .iter()
        .find(|p| p.id == default_id)
        .cloned()
        .or_else(|| providers.into_iter().next())
}

/// The configured providers in failover order: the default first, then the rest
/// in their stored order. Shared by the LLM client's failover chain
/// ([`crate::infra::llm::from_settings`]) and by subtitle translation, so a primary
/// that is out of credits / rate-limited / down degrades to the next provider
/// (e.g. cloud OpenRouter to a local Ollama) everywhere, not just in some features.
pub fn ordered_providers(settings: &Settings) -> Vec<LlmProvider> {
    let all = llm_providers(settings);
    let default_id = default_provider(settings).map(|p| p.id);
    let mut out = Vec::with_capacity(all.len());
    if let Some(did) = &default_id {
        if let Some(def) = all.iter().find(|p| &p.id == did) {
            out.push(def.clone());
        }
    }
    out.extend(all.into_iter().filter(|p| Some(&p.id) != default_id.as_ref()));
    out
}

/// Persist the full provider set + the global enable flag + default id.
///
/// **Secret-merge**: any incoming provider with a blank `api_key` keeps the key
/// already stored under the same id the client never receives saved keys, so a
/// plain round-trip would otherwise wipe them.
pub fn set_llm(
    settings: &Settings,
    pool: &Pool,
    enabled: bool,
    incoming: Vec<LlmProvider>,
    default_id: &str,
) {
    let stored = llm_providers(settings);
    let merged: Vec<LlmProvider> = incoming
        .into_iter()
        .map(|mut p| {
            if p.api_key.trim().is_empty() {
                if let Some(prev) = stored.iter().find(|s| s.id == p.id) {
                    p.api_key = prev.api_key.clone();
                }
            }
            p
        })
        .collect();

    let mut patch = BTreeMap::new();
    patch.insert("llmEnabled".to_string(), json!(enabled));
    patch.insert("llmProviders".to_string(), json!(merged));
    patch.insert("llmDefaultProvider".to_string(), json!(default_id));
    // Consume the legacy flat keys: once the multi-provider list is authoritative,
    // a saved-empty list must stay empty. Leaving them set would re-trigger the
    // one-time migration on the next read and resurrect a deleted provider + key.
    patch.insert("llmBaseUrl".to_string(), json!(""));
    patch.insert("llmModel".to_string(), json!(""));
    patch.insert("llmApiKey".to_string(), json!(""));
    settings.set_patch(pool, patch);
}
