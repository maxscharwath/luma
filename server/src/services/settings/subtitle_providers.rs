//! Multi-provider online-subtitle configuration, persisted in the settings store
//! and managed on the admin "Subtitles" page. Mirrors [`super::llm`]: the list
//! lives under the `subtitleProviders` key with one marked default, plus a
//! one-time migration from the legacy flat `os*` keys. Secrets (`apiKey`,
//! `password`) are never returned to the client; the admin DTO exposes only
//! `hasApiKey` / `hasPassword`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Pool;

use super::store::Settings;

/// One configured subtitle provider. `kind` selects the engine; each kind uses a
/// subset of the fields:
/// - `opensubtitles` (community DB): `api_key` + `username` + `password`.
/// - `whisper` (cloud speech-to-text, OpenAI-compatible `/audio/transcriptions`):
///   `api_key` + `base_url` + `model`.
/// - `whisperLocal` (offline whisper.cpp): `base_url` = binary path, `model` =
///   GGUF model path.
/// - `translate` (LLM-powered, reuses the app's default AI provider): no fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleProvider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

fn default_kind() -> String {
    "opensubtitles".to_string()
}

/// The configured providers. Parses the persisted `subtitleProviders` array;
/// when empty, migrates the legacy flat `os*` keys into one provider so existing
/// installs keep working.
pub fn subtitle_providers(settings: &Settings) -> Vec<SubtitleProvider> {
    if let Ok(list) = serde_json::from_value::<Vec<SubtitleProvider>>(settings.get("subtitleProviders")) {
        if !list.is_empty() {
            return list;
        }
    }
    let api_key = settings.get_str("osApiKey", "");
    let username = settings.get_str("osUsername", "");
    let password = settings.get_str("osPassword", "");
    if api_key.trim().is_empty() && username.trim().is_empty() && password.trim().is_empty() {
        return Vec::new();
    }
    vec![SubtitleProvider {
        id: "default".to_string(),
        name: "OpenSubtitles".to_string(),
        kind: "opensubtitles".to_string(),
        api_key,
        base_url: String::new(),
        model: String::new(),
        username,
        password,
    }]
}

/// The provider used for search/download: the one whose id matches
/// `subtitleDefaultProvider`, else the first configured (`None` if none).
pub fn default_subtitle_provider(settings: &Settings) -> Option<SubtitleProvider> {
    let providers = subtitle_providers(settings);
    let default_id = settings.get_str("subtitleDefaultProvider", "");
    providers.iter().find(|p| p.id == default_id).cloned().or_else(|| providers.into_iter().next())
}

/// Persist the full provider set + default id. Secret-merge: an incoming provider
/// with a blank `api_key` / `password` keeps the one already stored under that id
/// (the client never receives secrets, so a round-trip would otherwise wipe them).
pub fn set_subtitle_providers(settings: &Settings, pool: &Pool, incoming: Vec<SubtitleProvider>, default_id: &str) {
    let stored = subtitle_providers(settings);
    let merged: Vec<SubtitleProvider> = incoming
        .into_iter()
        .map(|mut p| {
            if let Some(prev) = stored.iter().find(|s| s.id == p.id) {
                if p.api_key.trim().is_empty() {
                    p.api_key = prev.api_key.clone();
                }
                if p.password.trim().is_empty() {
                    p.password = prev.password.clone();
                }
            }
            p
        })
        .collect();

    let mut patch = BTreeMap::new();
    patch.insert("subtitleProviders".to_string(), json!(merged));
    patch.insert("subtitleDefaultProvider".to_string(), json!(default_id));
    // Consume the legacy flat keys so a saved-empty list stays empty (else the
    // migration would resurrect a deleted provider on the next read).
    patch.insert("osApiKey".to_string(), json!(""));
    patch.insert("osUsername".to_string(), json!(""));
    patch.insert("osPassword".to_string(), json!(""));
    settings.set_patch(pool, patch);
}
