//! The evaluation context for definition templates and filters: the dynamic
//! [`Value`] type, and the variable namespaces (`.Keywords`, `.Config.*`,
//! `.Query.*`, `.Result.*`, `.Categories`, `.True`/`.False`) a template can
//! reference.

use std::collections::HashMap;

use crate::definition::Definition;
use crate::IndexerConfig;

/// A dynamic template value. Kept deliberately small: Cardigann only ever
/// juggles strings, booleans (checkbox settings + the `.True`/`.False`
/// constants), and string lists (`.Categories`).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Bool(bool),
    List(Vec<String>),
    Nil,
}

impl Value {
    /// Go-template truthiness: non-empty string / true / non-empty list.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            Value::List(l) => !l.is_empty(),
            Value::Nil => false,
        }
    }

    /// How the value renders when interpolated into output text.
    pub fn render(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::List(l) => l.join(" "),
            Value::Nil => String::new(),
        }
    }
}

/// The variable bindings a template evaluates against. Built once per search
/// (the query + config parts) and extended per row (`Result`) during field
/// extraction.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The filtered free-text keywords (`.Keywords`).
    pub keywords: String,
    /// Mapped tracker category ids for this request (`.Categories`).
    pub categories: Vec<String>,
    /// Setting name -> typed value (`.Config.*`). Checkbox settings are
    /// [`Value::Bool`] so `{{ if .Config.freeleech }}` and
    /// `eq .Config.x .False` behave; everything else is a string.
    pub config: HashMap<String, Value>,
    /// Query attributes (`.Query.*`): `Keywords`, `Type`, `IMDBID`, `TMDBID`,
    /// `Season`, `Ep`/`Episode`, `Year`, ...
    pub query: HashMap<String, String>,
    /// Fields extracted so far for the current row (`.Result.*`).
    pub result: HashMap<String, String>,
    /// The current item inside a `{{ range }}` (bare `.`).
    pub dot: Option<Value>,
}

impl Context {
    /// Seed the config namespace from a definition's settings (types + defaults)
    /// overlaid with the admin-entered [`IndexerConfig`].
    pub fn with_config(def: &Definition, cfg: &IndexerConfig) -> Self {
        let mut config = HashMap::new();
        for s in &def.settings {
            let raw = cfg
                .settings
                .get(&s.name)
                .cloned()
                .or_else(|| s.default.clone());
            let value = match s.kind.as_str() {
                "checkbox" => Value::Bool(matches!(
                    raw.as_deref(),
                    Some("true") | Some("1") | Some("yes") | Some("on")
                )),
                // info_* settings are display-only; skip them entirely.
                k if k.starts_with("info") => continue,
                _ => Value::Str(raw.unwrap_or_default()),
            };
            config.insert(s.name.clone(), value);
        }
        // The site link is always available to templates as `.Config.sitelink`.
        config.insert("sitelink".to_string(), Value::Str(cfg.base_url.clone()));
        Context { config, ..Default::default() }
    }

    /// Resolve a dotted path (`["Config", "username"]`, `["Keywords"]`, `[]` for
    /// the bare dot) to a value. Unknown paths resolve to [`Value::Nil`].
    pub fn resolve(&self, path: &[&str]) -> Value {
        match path {
            [] => self.dot.clone().unwrap_or(Value::Nil),
            ["True"] => Value::Bool(true),
            ["False"] => Value::Bool(false),
            ["Keywords"] => Value::Str(self.keywords.clone()),
            ["Categories"] => Value::List(self.categories.clone()),
            ["Config", key] => self.config.get(*key).cloned().unwrap_or(Value::Nil),
            ["Query", key] => {
                self.query.get(*key).map(|s| Value::Str(s.clone())).unwrap_or(Value::Nil)
            }
            ["Result", key] => {
                self.result.get(*key).map(|s| Value::Str(s.clone())).unwrap_or(Value::Nil)
            }
            _ => Value::Nil,
        }
    }
}
