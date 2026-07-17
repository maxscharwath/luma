//! Plural categories and the default rule.
//!
//! Pluralization is opt-in *per key* via the catalog: a key gets a `_one`
//! variant only for languages you choose to pluralize. If a key has no `_one`
//! variant, [`crate::I18n::translate`] falls back to the base key, so languages
//! you don't pluralize need no rule at all this is why the default rule is a
//! trivial `one`/`other` split and there is deliberately no CLDR language table.
//! Supply [`crate::Builder::plural_rule`] only if you actually render a plural
//! whose language needs something other than "singular at 1".

/// A CLDR plural category. Catalog keys carry the category as a suffix
/// (`key_one`, `key_other`, …); the engine appends the selected one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl Category {
    /// The catalog-key suffix (`"one"`, `"other"`, …).
    pub fn suffix(self) -> &'static str {
        match self {
            Category::Zero => "zero",
            Category::One => "one",
            Category::Two => "two",
            Category::Few => "few",
            Category::Many => "many",
            Category::Other => "other",
        }
    }
}

/// Picks a [`Category`] for a locale + integer count. A plain `fn`, so it's
/// allocation-free and trivially `Send + Sync`; pass a custom one to
/// [`crate::Builder::plural_rule`].
pub type PluralRule = fn(locale: &str, count: i64) -> Category;

/// The default rule: `one` for exactly 1, else `other`. Correct for English and
/// the great majority of catalogs, which only distinguish singular from plural.
pub fn one_other(_locale: &str, count: i64) -> Category {
    if count == 1 {
        Category::One
    } else {
        Category::Other
    }
}
