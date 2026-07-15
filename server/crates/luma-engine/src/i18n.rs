//! LUMA's i18n wiring over the generic [`luma_i18n`] engine.
//!
//! The engine crate is app-agnostic; this module supplies LUMA's specifics the
//! default locale, the supported set, and the shared catalogs in
//! `packages/core/src/locales` (the same files the TS clients bundle, so keys
//! stay in lock-step). It builds one [`luma_i18n::I18n`] at first use and exposes
//! it under the historical `crate::i18n::…` paths, plus the axum request-locale
//! extractor.

use std::convert::Infallible;
use std::sync::OnceLock;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use luma_i18n::I18n;

use crate::state::SharedState;

/// LUMA's fallback locale (a key missing in the active locale resolves here, then
/// to the raw key).
pub const DEFAULT_LOCALE: &str = "fr";

/// LUMA's supported locale codes the single source of truth for "which languages
/// we support". Drives i18n resolution, TMDB translation fetch, and LLM
/// section-title fan-out. Guarded against the built engine by a test below.
pub const SUPPORTED_LOCALES: &[&str] = &["fr", "en"];

/// `include_str!` a shared catalog, path anchored to this crate's manifest dir
/// (`server/crates/luma-engine`), so `../../../` reaches the repo root.
macro_rules! catalog {
    ($code:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/core/src/locales/",
            $code,
            ".json"
        ))
    };
}

/// The process-wide LUMA translation engine, built once from the shared catalogs.
fn i18n() -> &'static I18n {
    static ENGINE: OnceLock<I18n> = OnceLock::new();
    ENGINE.get_or_init(|| {
        I18n::builder()
            .default_locale(DEFAULT_LOCALE)
            .catalog_json("fr", catalog!("fr"))
            .catalog_json("en", catalog!("en"))
            .build()
            .expect("LUMA i18n catalogs")
    })
}

/// Translate `key` in `locale`, falling back to [`DEFAULT_LOCALE`] then the raw
/// key. A numeric `count` var selects a CLDR plural variant.
pub fn t(locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
    i18n().t(locale, key, vars)
}

/// Map a BCP-47 tag or native display name to a supported locale, or `None`.
pub fn normalize(tag: &str) -> Option<&'static str> {
    i18n().normalize_locale(tag)
}

/// A user's account locale for server-rendered strings. Admin endpoints are
/// always authenticated, so the (account-synced) preference is the right source;
/// falls back to [`DEFAULT_LOCALE`] for an unset/unknown value. Shared by the
/// admin handlers and the module host-seam gate.
pub fn user_locale(user: &luma_domain::User) -> &'static str {
    user.language.as_deref().and_then(normalize).unwrap_or(DEFAULT_LOCALE)
}

/// Best locale from an explicit preference and/or an `Accept-Language` header.
pub fn detect_locale(preferred: Option<&str>, accept_language: Option<&str>) -> &'static str {
    i18n().detect_locale(preferred, accept_language)
}

/// The resolved request locale, from `Accept-Language` (else [`DEFAULT_LOCALE`]).
/// Drives every server-rendered string (admin labels, error messages).
pub struct ReqLocale(pub &'static str);

impl FromRequestParts<SharedState> for ReqLocale {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok());
        Ok(ReqLocale(detect_locale(None, header)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_matches_built_engine() {
        let e = i18n();
        assert_eq!(e.default_locale(), DEFAULT_LOCALE);
        assert_eq!(e.supported().collect::<Vec<_>>(), SUPPORTED_LOCALES.to_vec());
        // Real catalogs load and pluralize (default rule: singular at 1).
        assert_eq!(t("fr", "content.seasonCount", &[("count", "1")]), "1 saison");
        assert_eq!(t("fr", "content.seasonCount", &[("count", "2")]), "2 saisons");
        assert_eq!(t("en", "content.seasonCount", &[("count", "1")]), "1 season");
        assert_eq!(normalize("en-US"), Some("en"));
    }
}
