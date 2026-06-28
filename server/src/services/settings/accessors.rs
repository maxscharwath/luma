//! Typed, functional accessors over the raw store: the settings the server
//! actually reads to change behaviour (LAN nets, server name, transcode cap),
//! plus the persisted multi-folder library definitions.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::Pool;

use super::store::Settings;

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
                id: crate::services::scan::short_hash(&path),
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
