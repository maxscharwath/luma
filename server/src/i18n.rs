//! Server-side i18n a tiny mirror of the shared `@luma/core` i18n.
//!
//! The JSON catalogs in `packages/core/src/locales` are the single source of
//! truth for the whole stack (they are also bundled into the TS clients). We
//! `include_str!` them at compile time and apply the same `{var}` interpolation,
//! so a key added on the client is automatically available on the server.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::OnceLock;

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::state::SharedState;

/// Default locale (matches the TS `DEFAULT_LOCALE`).
pub const DEFAULT_LOCALE: &str = "fr";

const FR_JSON: &str = include_str!("../../packages/core/src/locales/fr.json");
const EN_JSON: &str = include_str!("../../packages/core/src/locales/en.json");

/// Lazily-parsed catalog for a locale (falls back to French for unknown codes).
fn catalog(locale: &str) -> &'static HashMap<String, String> {
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    match locale {
        "en" => EN.get_or_init(|| serde_json::from_str(EN_JSON).expect("en.json catalog")),
        _ => FR.get_or_init(|| serde_json::from_str(FR_JSON).expect("fr.json catalog")),
    }
}

/// Map a BCP-47 tag, an `Accept-Language` header, or one of the server's display
/// names (`"Français"`/`"English"`) to a supported locale code, or `None`.
pub fn normalize(tag: &str) -> Option<&'static str> {
    match tag.trim() {
        "Français" => return Some("fr"),
        "English" => return Some("en"),
        _ => {}
    }
    // Take the highest-priority listed language and strip any region/quality
    // suffix: "en-US,en;q=0.9,fr;q=0.8" → "en", "fr-CH" → "fr".
    let base = tag
        .split([',', ';', '-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "fr" => Some("fr"),
        "en" => Some("en"),
        _ => None,
    }
}

/// Replace `{name}` tokens in `template` from `vars`. Unknown tokens are kept.
fn interpolate(template: &str, vars: &[(&str, &str)]) -> String {
    if vars.is_empty() {
        return template.to_string();
    }
    let mut out = template.to_string();
    for (name, value) in vars {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Translate `key` in `locale`, falling back to French then to the raw key.
pub fn t(locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let template = catalog(locale)
        .get(key)
        .or_else(|| catalog(DEFAULT_LOCALE).get(key))
        .map(String::as_str)
        .unwrap_or(key);
    interpolate(template, vars)
}

/// The resolved request locale. Clients send their active locale (the account
/// preference once signed in) as `Accept-Language`; we honour it, else fall back
/// to the default. Drives every server-rendered string (admin settings labels,
/// error messages).
pub struct ReqLocale(pub &'static str);

#[async_trait]
impl FromRequestParts<SharedState> for ReqLocale {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let locale = parts
            .headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .and_then(normalize)
            .unwrap_or(DEFAULT_LOCALE);
        Ok(ReqLocale(locale))
    }
}
