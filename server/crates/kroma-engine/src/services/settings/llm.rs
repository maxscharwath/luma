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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-settings-llm-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::db::init(&path).unwrap()
    }

    fn settings(pool: &Pool) -> Settings {
        Settings::load(pool)
    }

    fn provider(id: &str, key: &str) -> LlmProvider {
        LlmProvider {
            id: id.to_string(),
            name: format!("Name-{id}"),
            provider: "openai".to_string(),
            base_url: "http://x".to_string(),
            model: "m".to_string(),
            api_key: key.to_string(),
            temperature: 0.5,
            max_tokens: 500,
            reasoning: false,
        }
    }

    #[test]
    fn providers_empty_when_nothing_configured() {
        let pool = test_pool();
        assert!(llm_providers(&settings(&pool)).is_empty());
        assert!(default_provider(&settings(&pool)).is_none());
        assert!(ordered_providers(&settings(&pool)).is_empty());
    }

    #[test]
    fn providers_migrate_from_flat_keys() {
        let pool = test_pool();
        let s = settings(&pool);
        s.set_patch(&pool, BTreeMap::from([
            ("llmModel".to_string(), json!("qwen2.5")),
            ("llmBaseUrl".to_string(), json!("http://localhost:11434/v1")),
            ("llmProvider".to_string(), json!("openai")),
        ]));
        let list = llm_providers(&s);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "default");
        assert_eq!(list[0].model, "qwen2.5");
        assert_eq!(list[0].base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn set_llm_persists_and_default_selection() {
        let pool = test_pool();
        let s = settings(&pool);
        set_llm(&s, &pool, true, vec![provider("a", "k1"), provider("b", "k2")], "b");
        let list = llm_providers(&s);
        assert_eq!(list.len(), 2);
        // default_provider resolves to id "b"
        assert_eq!(default_provider(&s).unwrap().id, "b");
        // ordered_providers puts the default first
        let ordered = ordered_providers(&s);
        assert_eq!(ordered[0].id, "b");
        assert_eq!(ordered[1].id, "a");
        // flat keys are consumed (cleared)
        assert_eq!(s.get_str("llmModel", "x"), "");
    }

    #[test]
    fn default_provider_falls_back_to_first_when_id_missing() {
        let pool = test_pool();
        let s = settings(&pool);
        set_llm(&s, &pool, true, vec![provider("a", "k1"), provider("b", "k2")], "nonexistent");
        // No provider matches "nonexistent" -> first configured.
        assert_eq!(default_provider(&s).unwrap().id, "a");
    }

    #[test]
    fn set_llm_secret_merge_keeps_stored_key_on_blank() {
        let pool = test_pool();
        let s = settings(&pool);
        set_llm(&s, &pool, true, vec![provider("a", "secret-key")], "a");
        // Re-save the same provider with a BLANK api key -> stored key is retained.
        set_llm(&s, &pool, true, vec![provider("a", "")], "a");
        let list = llm_providers(&s);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].api_key, "secret-key");
    }

    #[test]
    fn llm_provider_serde_defaults() {
        let p: LlmProvider = serde_json::from_value(json!({"id":"x"})).unwrap();
        assert_eq!(p.provider, "openai");
        assert!((p.temperature - 0.7).abs() < 1e-6);
        assert_eq!(p.max_tokens, 900);
        assert!(!p.reasoning);
    }
}
