//! A small, generic i18n engine a framework-agnostic Rust counterpart to
//! `@kroma/core`'s `i18n.ts`.
//!
//! Nothing here is application-specific: you build an [`I18n`] instance from your
//! own catalogs, default locale, and plural rules, then translate against it. The
//! engine provides `{name}` interpolation, CLDR pluralization, locale
//! normalization/detection, and a default→raw-key fallback chain.
//!
//! ```
//! use kroma_i18n::I18n;
//! let i18n = I18n::builder()
//!     .default_locale("fr")
//!     .catalog_json("fr", r#"{ "hi": "Salut {name}", "n_item_one": "{count} objet", "n_item": "{count} objets" }"#)
//!     .catalog_json("en", r#"{ "hi": "Hi {name}" }"#)
//!     .build()
//!     .unwrap();
//!
//! assert_eq!(i18n.t("en", "hi", &[("name", "Max")]), "Hi Max");
//! assert_eq!(i18n.t("fr", "n_item", &[("count", "1")]), "1 objet");   // plural: _one
//! assert_eq!(i18n.t("fr", "n_item", &[("count", "3")]), "3 objets");  // plural: base
//! // A key missing in `en` falls back to the default locale (`fr`), then the raw key.
//! assert_eq!(i18n.t("en", "n_item", &[("count", "1")]), "1 objet");
//! ```

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

mod plural;
pub use plural::{one_other, Category, PluralRule};

/// A configured translation engine. Cheap to share behind an `Arc`/`OnceLock`;
/// build once at startup.
pub struct I18n {
    default: String,
    /// Insertion order preserved (default locale first, by convention).
    locales: Vec<Locale>,
    plural: PluralRule,
}

struct Locale {
    code: String,
    label_key: String,
    entries: HashMap<String, String>,
}

/// A locale's code and the message key holding its native display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleInfo<'a> {
    pub code: &'a str,
    pub label_key: &'a str,
}

/// Why [`Builder::build`] failed.
#[derive(Debug)]
pub enum BuildError {
    /// No `default_locale` was set.
    MissingDefault,
    /// No catalog was added for the configured default locale.
    DefaultNotLoaded(String),
    /// A catalog's JSON was not a flat `{ "key": "value" }` object.
    Catalog(String, serde_json::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::MissingDefault => write!(f, "no default locale set"),
            BuildError::DefaultNotLoaded(c) => write!(f, "no catalog for default locale `{c}`"),
            BuildError::Catalog(c, e) => write!(f, "catalog `{c}` is not a flat string map: {e}"),
        }
    }
}

impl Error for BuildError {}

/// Builds an [`I18n`]. See [`I18n::builder`].
pub struct Builder {
    default: Option<String>,
    raw: Vec<(String, String)>,
    parsed: Vec<(String, HashMap<String, String>)>,
    plural: PluralRule,
    label_key: fn(&str) -> String,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            default: None,
            raw: Vec::new(),
            parsed: Vec::new(),
            plural: one_other,
            // Native display name lives at `lang.<code>` by convention.
            label_key: |code| format!("lang.{code}"),
        }
    }
}

impl Builder {
    /// The fallback locale: a key missing in the active locale resolves here,
    /// then to the raw key. Must have a catalog. Required.
    pub fn default_locale(mut self, code: impl Into<String>) -> Self {
        self.default = Some(code.into());
        self
    }

    /// Use a custom plural rule instead of the default [`one_other`].
    pub fn plural_rule(mut self, rule: PluralRule) -> Self {
        self.plural = rule;
        self
    }

    /// Override how a locale's native-label key is derived from its code
    /// (default: `|c| format!("lang.{c}")`). Used by [`I18n::normalize_locale`].
    pub fn label_key(mut self, f: fn(&str) -> String) -> Self {
        self.label_key = f;
        self
    }

    /// Add a locale from a flat `{ "key": "value" }` JSON catalog. Parsed at
    /// [`build`](Self::build).
    pub fn catalog_json(mut self, code: impl Into<String>, json: impl Into<String>) -> Self {
        self.raw.push((code.into(), json.into()));
        self
    }

    /// Add a locale from an already-parsed map.
    pub fn catalog(mut self, code: impl Into<String>, entries: HashMap<String, String>) -> Self {
        self.parsed.push((code.into(), entries));
        self
    }

    /// Parse/validate everything and construct the engine.
    pub fn build(self) -> Result<I18n, BuildError> {
        let default = self.default.ok_or(BuildError::MissingDefault)?;
        let mut locales = Vec::with_capacity(self.raw.len() + self.parsed.len());
        for (code, json) in self.raw {
            let entries = serde_json::from_str(&json).map_err(|e| BuildError::Catalog(code.clone(), e))?;
            locales.push(Locale { label_key: (self.label_key)(&code), code, entries });
        }
        for (code, entries) in self.parsed {
            locales.push(Locale { label_key: (self.label_key)(&code), code, entries });
        }
        if !locales.iter().any(|l| l.code == default) {
            return Err(BuildError::DefaultNotLoaded(default));
        }
        // Default locale leads the ordering.
        locales.sort_by_key(|l| l.code != default);
        Ok(I18n { default, locales, plural: self.plural })
    }
}

impl I18n {
    /// Start building an engine.
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// The configured fallback locale.
    pub fn default_locale(&self) -> &str {
        &self.default
    }

    /// Supported locale codes, default first.
    pub fn supported(&self) -> impl Iterator<Item = &str> {
        self.locales.iter().map(|l| l.code.as_str())
    }

    /// Every locale's code + native-label key, default first.
    pub fn locales(&self) -> impl Iterator<Item = LocaleInfo<'_>> {
        self.locales.iter().map(|l| LocaleInfo { code: &l.code, label_key: &l.label_key })
    }

    /// Whether `code` is a supported locale.
    pub fn is_locale(&self, code: &str) -> bool {
        self.locales.iter().any(|l| l.code == code)
    }

    /// Whether `key` exists in the default (authoritative) catalog.
    pub fn is_message_key(&self, key: &str) -> bool {
        self.lookup(&self.default, key).is_some()
    }

    fn lookup(&self, code: &str, key: &str) -> Option<&str> {
        self.locales.iter().find(|l| l.code == code)?.entries.get(key).map(String::as_str)
    }

    fn has_key(&self, code: &str, key: &str) -> bool {
        self.lookup(code, key).is_some() || self.lookup(&self.default, key).is_some()
    }

    /// Resolve a requested tag to the supported catalog code that best serves it:
    /// an **exact** match first (so a regional catalog like `fr-CH` wins if you
    /// ship one), else the **base language** with the region stripped and case
    /// normalized (`fr`, `fr_FR`, `fr-CH`, `FR` all → `fr`), else `None`.
    fn resolve_code(&self, tag: &str) -> Option<&str> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(l) = self.locales.iter().find(|l| l.code == trimmed) {
            return Some(&l.code);
        }
        let base = base_language(trimmed);
        self.locales.iter().find(|l| base_language(&l.code) == base).map(|l| l.code.as_str())
    }

    /// Map a BCP-47 tag (`"en-US"`, `"FR"`, `"fr_CH"`) or a native display name
    /// (from the `label_key` catalog entry) to a supported locale, or `None`.
    pub fn normalize_locale(&self, tag: &str) -> Option<&str> {
        if let Some(code) = self.resolve_code(tag) {
            return Some(code);
        }
        // Native display name (data-driven from each locale's own label entry).
        let trimmed = tag.trim();
        self.locales
            .iter()
            .find(|l| l.entries.get(&l.label_key).map(String::as_str) == Some(trimmed))
            .map(|l| l.code.as_str())
    }

    /// Best locale: an explicit `preferred` wins, then the first resolvable
    /// `Accept-Language` entry, else the default.
    pub fn detect_locale(&self, preferred: Option<&str>, accept_language: Option<&str>) -> &str {
        if let Some(loc) = preferred.and_then(|p| self.normalize_locale(p)) {
            return loc;
        }
        if let Some(header) = accept_language {
            for part in header.split(',') {
                let tag = part.split(';').next().unwrap_or("").trim();
                if let Some(loc) = self.normalize_locale(tag) {
                    return loc;
                }
            }
        }
        &self.default
    }

    /// Resolve `key` to its plural variant for `count`: `key_<category>` if it
    /// exists, else `key_other`, else the base `key`. The plural category uses the
    /// caller's original `tag` (so a custom rule sees `pt_BR` vs `pt_PT`); the
    /// variant is looked up under the resolved catalog `code`.
    fn resolve_plural_key(&self, tag: &str, code: &str, key: &str, count: i64) -> String {
        let variant = format!("{key}_{}", (self.plural)(tag, count).suffix());
        if self.has_key(code, &variant) {
            return variant;
        }
        let other = format!("{key}_other");
        if self.has_key(code, &other) {
            return other;
        }
        key.to_string()
    }

    /// Translate `key` in `locale`, falling back to the default locale then the
    /// raw key. Regional tags resolve to their base catalog (`fr_CH` → `fr`). A
    /// numeric `count` var selects a plural variant and, like every var, is
    /// interpolated into `{count}`.
    pub fn translate(&self, locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
        let code = self.resolve_code(locale).unwrap_or(&self.default);
        let count = vars.iter().find(|(k, _)| *k == "count").and_then(|(_, v)| v.parse::<i64>().ok());
        let lookup_key = match count {
            Some(c) => self.resolve_plural_key(locale, code, key, c),
            None => key.to_string(),
        };
        let template = self
            .lookup(code, &lookup_key)
            .or_else(|| self.lookup(&self.default, &lookup_key))
            .unwrap_or(key);
        interpolate(template, vars)
    }

    /// Short alias for [`translate`](Self::translate).
    pub fn t(&self, locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
        self.translate(locale, key, vars)
    }

    /// A translation function bound to one locale (unknown codes fall back to the
    /// default, so this never fails).
    pub fn translator<'a>(&'a self, locale: &str) -> Translator<'a> {
        let code = self.resolve_code(locale).unwrap_or(&self.default);
        Translator { i18n: self, locale: code }
    }
}

/// A translation function bound to one locale of an [`I18n`].
#[derive(Clone, Copy)]
pub struct Translator<'a> {
    i18n: &'a I18n,
    locale: &'a str,
}

impl<'a> Translator<'a> {
    /// The bound locale.
    pub fn locale(&self) -> &'a str {
        self.locale
    }

    /// Translate `key` (see [`I18n::translate`]).
    pub fn t(&self, key: &str, vars: &[(&str, &str)]) -> String {
        self.i18n.translate(self.locale, key, vars)
    }
}

/// The base language subtag, region stripped and lowercased: `"fr_CH"` → `"fr"`,
/// `"en-US"` → `"en"`, `"FR"` → `"fr"`.
fn base_language(tag: &str) -> String {
    tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

/// Replace `{name}` tokens in `template` from `vars` (`name` is `[A-Za-z0-9_]+`,
/// matching the TS `\{(\w+)\}`). Unknown tokens are kept verbatim; single pass,
/// so a substituted value is never re-scanned.
pub fn interpolate(template: &str, vars: &[(&str, &str)]) -> String {
    if vars.is_empty() || !template.contains('{') {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                let is_token =
                    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, value)) if is_token => out.push_str(value),
                    _ => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                out.push_str(after);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> I18n {
        // A tiny app-agnostic catalog set, exercising the engine generically.
        I18n::builder()
            .default_locale("fr")
            .catalog_json(
                "fr",
                r#"{ "lang.fr": "Français", "lang.en": "Anglais",
                     "hi": "Salut {name}", "seasons": "{count} saisons", "seasons_one": "{count} saison" }"#,
            )
            .catalog_json(
                "en",
                r#"{ "lang.en": "English", "hi": "Hi {name}",
                     "seasons": "{count} seasons", "seasons_one": "{count} season" }"#,
            )
            .build()
            .unwrap()
    }

    #[test]
    fn builder_validates() {
        assert!(matches!(I18n::builder().build(), Err(BuildError::MissingDefault)));
        assert!(matches!(
            I18n::builder().default_locale("de").catalog_json("en", "{}").build(),
            Err(BuildError::DefaultNotLoaded(_))
        ));
        assert!(matches!(
            I18n::builder().default_locale("en").catalog_json("en", "not json").build(),
            Err(BuildError::Catalog(..))
        ));
    }

    #[test]
    fn default_leads_and_supported() {
        let i = fixture();
        assert_eq!(i.default_locale(), "fr");
        assert_eq!(i.supported().collect::<Vec<_>>(), vec!["fr", "en"]);
        assert!(i.is_locale("en") && !i.is_locale("de"));
        assert!(i.is_message_key("hi") && !i.is_message_key("nope"));
    }

    #[test]
    fn interpolation_keeps_unknown_tokens() {
        assert_eq!(interpolate("hi {name}", &[("name", "Max")]), "hi Max");
        assert_eq!(interpolate("keep {unknown}", &[("name", "x")]), "keep {unknown}");
        assert_eq!(interpolate("{a}", &[("a", "{b}"), ("b", "!")]), "{b}");
    }

    #[test]
    fn normalize_and_detect() {
        let i = fixture();
        assert_eq!(i.normalize_locale("en-US"), Some("en"));
        assert_eq!(i.normalize_locale("FR"), Some("fr"));
        assert_eq!(i.normalize_locale("Français"), Some("fr"));
        assert_eq!(i.normalize_locale("English"), Some("en"));
        assert_eq!(i.normalize_locale("de"), None);
        assert_eq!(i.detect_locale(Some("de"), Some("en-US,en;q=0.9")), "en");
        assert_eq!(i.detect_locale(None, None), "fr");
    }

    #[test]
    fn regional_variants_resolve_to_base() {
        let i = fixture();
        // fr, fr_FR, fr-CH, FR all resolve to the `fr` catalog.
        for tag in ["fr", "fr_FR", "fr-CH", "FR", "fr_CA"] {
            assert_eq!(i.normalize_locale(tag), Some("fr"), "tag {tag}");
            assert_eq!(i.t(tag, "seasons", &[("count", "2")]), "2 saisons", "tag {tag}");
        }
        assert_eq!(i.t("en-GB", "hi", &[("name", "Jo")]), "Hi Jo");
        // An exact regional catalog wins over the base.
        let r = I18n::builder()
            .default_locale("en")
            .catalog_json("en", r#"{ "color": "color" }"#)
            .catalog_json("en-GB", r#"{ "color": "colour" }"#)
            .build()
            .unwrap();
        assert_eq!(r.t("en-GB", "color", &[]), "colour"); // exact
        assert_eq!(r.t("en-AU", "color", &[]), "color"); // base fallback
    }

    #[test]
    fn pluralization_default_one_other() {
        let i = fixture();
        // Default rule: singular at 1, plural otherwise (all locales).
        assert_eq!(i.t("en", "seasons", &[("count", "1")]), "1 season");
        assert_eq!(i.t("en", "seasons", &[("count", "0")]), "0 seasons");
        assert_eq!(i.t("fr", "seasons", &[("count", "1")]), "1 saison");
        assert_eq!(i.t("fr", "seasons", &[("count", "2")]), "2 saisons");
        // A key with no `_one` variant just uses the base key (no rule needed).
        assert_eq!(i.t("en", "hi", &[("name", "A"), ("count", "1")]), "Hi A");
    }

    #[test]
    fn plural_rule_is_pluggable() {
        // No baked-in language table: to make French treat 0 as singular, pass a
        // rule. Proves the engine is generic without hardcoding CLDR.
        fn fr_zero_is_one(locale: &str, count: i64) -> Category {
            if locale.starts_with("fr") && count == 0 {
                Category::One
            } else {
                one_other(locale, count)
            }
        }
        let i = I18n::builder()
            .default_locale("fr")
            .plural_rule(fr_zero_is_one)
            .catalog_json("fr", r#"{ "seasons": "{count} saisons", "seasons_one": "{count} saison" }"#)
            .build()
            .unwrap();
        assert_eq!(i.t("fr", "seasons", &[("count", "0")]), "0 saison");
        assert_eq!(i.t("fr", "seasons", &[("count", "2")]), "2 saisons");
    }

    #[test]
    fn fallback_and_translator() {
        let i = fixture();
        // en missing "lang.fr" → falls back to default (fr) catalog.
        assert_eq!(i.t("en", "lang.fr", &[]), "Français");
        // Unknown key → raw key.
        assert_eq!(i.t("en", "missing.key", &[]), "missing.key");
        let tr = i.translator("en-US");
        assert_eq!(tr.locale(), "en");
        assert_eq!(tr.t("hi", &[("name", "Sam")]), "Hi Sam");
        assert_eq!(i.translator("xx").locale(), "fr");
    }
}
