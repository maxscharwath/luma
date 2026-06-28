//! The settings store: the in-memory key/value map, its typed raw accessors,
//! the persist-on-patch write path, and the built-in default values.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};

use crate::db::Pool;

/// Shared, cheap-to-clone handle to the live settings map.
#[derive(Clone)]
pub struct Settings {
    inner: Arc<RwLock<BTreeMap<String, Value>>>,
}

impl Settings {
    /// Build the store, loading any persisted rows over the built-in defaults.
    pub fn load(pool: &Pool) -> Self {
        let mut map = defaults();
        if let Ok(rows) = crate::db::settings_all(pool) {
            for (k, v) in rows {
                map.insert(k, v);
            }
        }
        Settings {
            inner: Arc::new(RwLock::new(map)),
        }
    }

    /// Raw value for `key`, falling back to the built-in default.
    pub fn get(&self, key: &str) -> Value {
        self.inner
            .read()
            .unwrap()
            .get(key)
            .cloned()
            .or_else(|| defaults().get(key).cloned())
            .unwrap_or(Value::Null)
    }

    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.get(key).as_bool().unwrap_or(fallback)
    }

    pub fn get_str(&self, key: &str, fallback: &str) -> String {
        match self.get(key) {
            Value::String(s) => s,
            _ => fallback.to_string(),
        }
    }

    pub fn get_i64(&self, key: &str, fallback: i64) -> i64 {
        let v = self.get(key);
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
            .unwrap_or(fallback)
    }

    /// Apply a patch in-memory and persist the changed keys. Unknown keys are
    /// accepted (forward-compat) but only keys present in [`defaults`] are kept,
    /// so a typo can't pollute the store. Returns the keys actually written.
    pub fn set_patch(&self, pool: &Pool, patch: BTreeMap<String, Value>) -> Vec<String> {
        let known = defaults();
        let mut written = Vec::new();
        let mut guard = self.inner.write().unwrap();
        for (k, v) in patch {
            if !known.contains_key(&k) {
                continue;
            }
            let _ = crate::db::settings_set(pool, &k, &v);
            guard.insert(k.clone(), v);
            written.push(k);
        }
        written
    }
}

/// Built-in default values for every known setting key. The set of keys here is
/// also the allow-list for [`Settings::set_patch`].
fn defaults() -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    // general
    m.insert("serverName".into(), json!("LUMA"));
    m.insert("uiLanguage".into(), json!("Français"));
    m.insert("timezone".into(), json!("Europe/Zurich (UTC+1)"));
    m.insert("autoUpdate".into(), json!(true));
    m.insert("updateChannel".into(), json!("Stable"));
    m.insert("anonStats".into(), json!(false));
    m.insert("showRecentHome".into(), json!(true));
    m.insert("theme".into(), json!("Sombre (Luma)"));
    m.insert("dateFormat".into(), json!("JJ/MM/AAAA"));
    // network
    m.insert("remoteAccess".into(), json!(false));
    m.insert("remoteUrl".into(), json!(""));
    m.insert("upLimit".into(), json!("Illimité"));
    m.insert("https".into(), json!("Préférées"));
    m.insert("ipv6".into(), json!(false));
    m.insert("localDiscovery".into(), json!(true));
    m.insert("localNetworks".into(), json!("192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12"));
    // transcoder
    m.insert("hwAccel".into(), json!(false));
    m.insert("hwDevice".into(), json!("Auto"));
    m.insert("hevcEncode".into(), json!(false));
    m.insert("transcoderSpeed".into(), json!("Automatique"));
    m.insert("bgQuality".into(), json!("Préférer la vitesse"));
    m.insert("maxConcurrent".into(), json!("4"));
    m.insert("deleteAfter".into(), json!(true));
    // storage / cache
    m.insert("cacheLimit".into(), json!("80 Go"));
    // libraries: persisted multi-folder definitions (seeded from env on first run).
    m.insert("libraries".into(), json!(null));
    m
}
