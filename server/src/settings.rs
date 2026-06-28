//! Persisted, runtime-editable server settings.
//!
//! `Config` (see [`crate::config`]) provides the immutable bootstrap values
//! sourced from the environment (bind address, data dir, …). This module layers
//! a *mutable* key/value store on top, persisted in the `settings` SQLite table,
//! so the admin console can change server behaviour at runtime and have it
//! survive a restart.
//!
//! Values are stored as JSON (`serde_json::Value`) keyed by a stable string. A
//! small set of keys are **functional** — the server reads them to change real
//! behaviour (LAN classification, server name, transcode limits, …). The rest
//! are persisted preferences the admin UI renders but the server does not (yet)
//! enforce; those are marked `applied: false` in the schema so the UI can be
//! honest about it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::db::Pool;
use crate::i18n;

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

/// The configured local networks (CIDR or prefix strings) used to classify a
/// client IP as LAN vs WAN. Comma/space separated.
pub fn local_networks(settings: &Settings) -> Vec<String> {
    settings
        .get_str("localNetworks", "192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12")
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The persisted display name for the server (falls back to "LUMA").
pub fn server_name(settings: &Settings) -> String {
    let n = settings.get_str("serverName", "LUMA");
    if n.trim().is_empty() {
        "LUMA".to_string()
    } else {
        n
    }
}

/// Max concurrent transcode sessions (functional cap), 1..=12.
pub fn max_transcodes(settings: &Settings) -> usize {
    settings.get_i64("maxConcurrent", 4).clamp(1, 12) as usize
}

// ----- library definitions (persisted, multi-folder) --------------------------

/// A named, runtime-editable library spanning one or more scan folders. Persisted
/// in the settings store under the `libraries` key, seeded from `LUMA_MEDIA_DIRS`
/// on first run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDef {
    pub id: String,
    pub name: String,
    /// `movies` | `shows` | `mixed` | "" (auto-detect from contents).
    #[serde(default)]
    pub kind: String,
    pub folders: Vec<String>,
    #[serde(rename = "autoScan", default = "default_true")]
    pub auto_scan: bool,
}

fn default_true() -> bool {
    true
}

/// The effective library definitions: persisted defs if present, else seeded
/// one-per-folder from the env-configured media dirs.
pub fn library_defs(settings: &Settings, config: &crate::config::Config) -> Vec<LibraryDef> {
    if let Value::Array(_) = settings.get("libraries") {
        if let Ok(defs) = serde_json::from_value::<Vec<LibraryDef>>(settings.get("libraries")) {
            return defs;
        }
    }
    config
        .media_dirs
        .iter()
        .map(|dir| {
            let path = dir.to_string_lossy().to_string();
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Bibliothèque")
                .to_string();
            LibraryDef {
                id: crate::scan::short_hash(&path),
                name,
                kind: String::new(),
                folders: vec![path],
                auto_scan: true,
            }
        })
        .collect()
}

/// Persist the full set of library definitions.
pub fn set_library_defs(settings: &Settings, pool: &Pool, defs: &[LibraryDef]) {
    let mut patch = BTreeMap::new();
    patch.insert("libraries".to_string(), json!(defs));
    settings.set_patch(pool, patch);
}

/// All scan folders across every effective library (for a flat walk if needed).
pub fn all_folders(settings: &Settings, config: &crate::config::Config) -> Vec<PathBuf> {
    library_defs(settings, config)
        .into_iter()
        .flat_map(|d| d.folders.into_iter().map(PathBuf::from))
        .collect()
}

// ----- schema -----------------------------------------------------------------

/// One editable (or read-only) setting row.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingRow {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// `toggle` | `select` | `text` | `value`.
    #[ts(type = "string")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[ts(type = "unknown")]
    pub value: Value,
    /// Whether the server actually enforces this setting (vs. stored-only).
    pub applied: bool,
}

/// A titled group of rows.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingGroup {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub rows: Vec<SettingRow>,
}

/// Build the grouped schema for one admin settings view (`general` | `network`
/// | `transcoder`), with each row's current value overlaid from `settings` and a
/// few dynamic/computed values injected from `config`.
pub fn groups(
    view: &str,
    settings: &Settings,
    config: &crate::config::Config,
    locale: &str,
) -> Vec<SettingGroup> {
    let g = |key: &str| settings.get(key);
    // Localised label/hint/title shorthands. NB: select `options` are persisted
    // *values* (the stored setting equals the chosen option string), so they are
    // intentionally NOT translated — only labels, hints, group titles & descs.
    let t = |key: &str| i18n::t(locale, key, &[]);
    let group = |title: &str, desc: Option<&str>, rows: Vec<SettingRow>| SettingGroup {
        title: t(title),
        desc: desc.map(t),
        rows,
    };
    match view {
        "general" => vec![
            group(
                "admin.serverIdentity",
                Some("admin.serverIdentityDesc"),
                vec![
                    row("serverName", t("admin.serverName"), Some(t("admin.serverNameHint")), "text", &[], g("serverName"), true),
                    row("uiLanguage", t("admin.uiLanguage"), None, "select", &["Français", "English"], g("uiLanguage"), false),
                    row("timezone", t("admin.timezone"), None, "select", &["Europe/Zurich (UTC+1)", "Europe/Paris (UTC+1)", "UTC"], g("timezone"), false),
                ],
            ),
            group(
                "admin.preferences",
                None,
                vec![
                    row("autoUpdate", t("admin.autoUpdate"), None, "toggle", &[], g("autoUpdate"), false),
                    row("updateChannel", t("admin.updateChannel"), Some(t("admin.updateChannelHint")), "select", &["Stable", "Bêta"], g("updateChannel"), false),
                    row("anonStats", t("admin.anonStats"), Some(t("admin.anonStatsHint")), "toggle", &[], g("anonStats"), false),
                    row("showRecentHome", t("admin.showRecentHome"), None, "toggle", &[], g("showRecentHome"), false),
                ],
            ),
            group(
                "admin.appearance",
                None,
                vec![
                    row("theme", t("admin.theme"), None, "select", &["Sombre (Luma)", "Système"], g("theme"), false),
                    row("dateFormat", t("admin.dateFormat"), None, "select", &["JJ/MM/AAAA", "MM/JJ/AAAA", "AAAA-MM-JJ"], g("dateFormat"), false),
                    row("version", t("admin.version"), None, "value", &[], json!(env!("CARGO_PKG_VERSION")), true),
                ],
            ),
        ],
        "network" => vec![
            group(
                "admin.remoteAccess",
                Some("admin.remoteAccessDesc"),
                vec![
                    row("remoteAccess", t("admin.enableRemoteAccess"), None, "toggle", &[], g("remoteAccess"), false),
                    row("remoteUrl", t("admin.customUrl"), Some(t("admin.customUrlHint")), "text", &[], g("remoteUrl"), false),
                    row("publicAddress", t("admin.publicAddress"), None, "value", &[], json!(public_address(config)), false),
                    row("upLimit", t("admin.upLimit"), Some(t("admin.upLimitHint")), "select", &["Illimité", "10 Mb/s", "20 Mb/s", "50 Mb/s"], g("upLimit"), false),
                ],
            ),
            group(
                "admin.secureConnections",
                None,
                vec![
                    row("https", t("admin.https"), None, "select", &["Préférées", "Requises", "Désactivées"], g("https"), false),
                    row("ipv6", t("admin.ipv6"), None, "toggle", &[], g("ipv6"), false),
                ],
            ),
            group(
                "admin.portsDiscovery",
                None,
                vec![
                    row("port", t("admin.port"), Some(t("admin.portHint")), "text", &[], json!(config.port.to_string()), false),
                    row("localDiscovery", t("admin.localDiscovery"), Some(t("admin.localDiscoveryHint")), "toggle", &[], g("localDiscovery"), false),
                    row("localNetworks", t("admin.localNetworks"), Some(t("admin.localNetworksHint")), "text", &[], g("localNetworks"), true),
                ],
            ),
        ],
        "transcoder" => vec![
            group(
                "admin.hwAccel",
                Some("admin.hwAccelDesc"),
                vec![
                    row("hwAccel", t("admin.useHwAccel"), Some(t("admin.useHwAccelHint")), "toggle", &[], g("hwAccel"), false),
                    row("hwDevice", t("admin.hwDevice"), None, "select", &["Auto", "Intel Quick Sync", "NVENC", "VA-API"], g("hwDevice"), false),
                    row("hevcEncode", t("admin.hevcEncode"), None, "toggle", &[], g("hevcEncode"), false),
                ],
            ),
            group(
                "admin.qualityPerf",
                None,
                vec![
                    row("transcoderSpeed", t("admin.transcoderSpeed"), None, "select", &["Automatique", "Préférer la vitesse", "Préférer la qualité"], g("transcoderSpeed"), false),
                    row("bgQuality", t("admin.bgQuality"), Some(t("admin.bgQualityHint")), "select", &["Préférer la vitesse", "Préférer la qualité"], g("bgQuality"), false),
                    row("maxConcurrent", t("admin.maxConcurrent"), None, "select", &["1", "2", "4", "6", "8"], g("maxConcurrent"), true),
                    row("throttleBuffer", t("admin.throttleBuffer"), None, "value", &[], json!("120 s"), true),
                ],
            ),
            group(
                "admin.tempFolders",
                None,
                vec![
                    row("transcodeDir", t("admin.transcodeDir"), None, "value", &[], json!(transcode_dir(config)), true),
                    row("deleteAfter", t("admin.deleteAfter"), None, "toggle", &[], g("deleteAfter"), true),
                ],
            ),
        ],
        _ => Vec::new(),
    }
}

fn row(
    key: &str,
    label: String,
    desc: Option<String>,
    kind: &'static str,
    options: &[&str],
    value: Value,
    applied: bool,
) -> SettingRow {
    SettingRow {
        key: key.to_string(),
        label,
        desc,
        kind,
        options: options.iter().map(|s| s.to_string()).collect(),
        value,
        applied,
    }
}

fn transcode_dir(config: &crate::config::Config) -> String {
    config.data_dir.join("transcode").to_string_lossy().to_string()
}

fn public_address(config: &crate::config::Config) -> String {
    config
        .web_url
        .clone()
        .unwrap_or_else(|| format!(":{}", config.port))
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
