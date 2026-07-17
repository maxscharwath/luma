//! Per-module admin state: the enabled flag + config values each module carries,
//! persisted in the `moduleStates` settings blob (`{ id: { enabled, config } }`).
//!
//! The module REGISTRY -- which modules exist, their manifests, capabilities and
//! backend behavior -- lives in the `kroma-module-kernel` crate (the one
//! composition point), built from the generated roster. This is only the
//! settings-state half, kept in the engine because engine internals read a
//! module's enabled flag to gate their work.

use serde_json::{json, Map, Value};

use crate::db::Pool;
use crate::services::settings::Settings;

/// The whole `{ id: { enabled, config } }` blob.
fn states(settings: &Settings) -> Map<String, Value> {
    settings.get("moduleStates").as_object().cloned().unwrap_or_default()
}

/// Read-modify-write one module's entry in the `moduleStates` blob under a
/// single settings write-lock, so a concurrent enable + config-save cannot
/// clobber each other (a plain read-then-write would drop one).
fn update_entry(
    settings: &Settings,
    pool: &Pool,
    id: &str,
    f: impl FnOnce(&mut Map<String, Value>),
) {
    settings.update_json(pool, "moduleStates", |current| {
        let mut all = current.as_object().cloned().unwrap_or_default();
        let mut entry = all.get(id).and_then(Value::as_object).cloned().unwrap_or_default();
        f(&mut entry);
        all.insert(id.to_string(), Value::Object(entry));
        Value::Object(all)
    });
}

/// Whether a module is enabled (default true when never toggled).
pub fn module_enabled(settings: &Settings, id: &str) -> bool {
    states(settings)
        .get(id)
        .and_then(|s| s.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// A module's stored config values (key -> value).
pub fn module_config(settings: &Settings, id: &str) -> Map<String, Value> {
    states(settings)
        .get(id)
        .and_then(|s| s.get("config"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Persist a module's enabled flag.
pub fn set_module_enabled(settings: &Settings, pool: &Pool, id: &str, enabled: bool) {
    update_entry(settings, pool, id, |entry| {
        entry.insert("enabled".into(), json!(enabled));
    });
}

/// Merge new config values into a module's stored config.
pub fn set_module_config(settings: &Settings, pool: &Pool, id: &str, values: Map<String, Value>) {
    update_entry(settings, pool, id, |entry| {
        let mut cfg = entry.get("config").and_then(Value::as_object).cloned().unwrap_or_default();
        for (k, v) in values {
            cfg.insert(k, v);
        }
        entry.insert("config".into(), Value::Object(cfg));
    });
}
