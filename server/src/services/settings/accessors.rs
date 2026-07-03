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

/// Max concurrent segment ffmpegs (functional cap), 1..=32. Segments are cheap
/// stream-copies (video is never re-encoded), so the bound is generous; it seeds
/// the semaphore in [`crate::infra::hls::HlsEngine`].
pub fn max_transcodes(settings: &Settings) -> usize {
    settings.get_i64("maxConcurrent", 8).clamp(1, 32) as usize
}

/// How many CPU-heavy media passes (storyboard tiles/montage, subtitle extraction,
/// marker fingerprinting) may run at once, 1..=32. `0`/unset = auto (`cores - 1`,
/// floored at 1) so a small NAS keeps a core for playback out of the box. Seeds +
/// live-updates the process-wide [`crate::infra::ffmpeg_gate`]. Unlike
/// `maxConcurrent` (cheap stream-copy segments), these passes actually decode, so
/// this is the knob that keeps the box usable during background processing.
pub fn media_workers(settings: &Settings) -> usize {
    match settings.get_i64("mediaConcurrency", 0) {
        n if n > 0 => (n as usize).clamp(1, 32),
        _ => crate::infra::ffmpeg_gate::auto_capacity(),
    }
}

/// Byte budget for the on-disk transcode (HLS segment) cache, from the
/// `transcodeCacheLimit` setting. `0` = unlimited (any non-numeric label, e.g.
/// "Illimité"). Labels use decimal "Go" (1 Go = 1e9 bytes), matching the image
/// `cacheLimit`. Seeds the budget in [`crate::infra::hls::HlsEngine`].
pub fn transcode_cache_limit_bytes(settings: &Settings) -> u64 {
    let label = settings.get_str("transcodeCacheLimit", "20 Go");
    // Take the leading numeric run INCLUDING a decimal point (e.g. "1.5 Go" -> 1.5,
    // "20 Go" -> 20). Filtering the '.' out would concatenate "1.5" into "15" (a 10x
    // error); a non-numeric label ("Illimité") parses to nothing => unlimited (0).
    let num: String = label.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    match num.parse::<f64>() {
        Ok(gb) if gb > 0.0 => (gb * 1_000_000_000.0) as u64,
        _ => 0,
    }
}

/// Whether Plex-style theme songs are enabled: enrichment downloads a show's
/// theme and the detail page loops it. Opt-in (off by default).
pub fn theme_songs_enabled(settings: &Settings) -> bool {
    settings.get_bool("themeSongs", false)
}

/// The TMDB metadata language used when enriching the catalog (e.g. `fr-FR`)
/// the persisted `tmdbLanguage` setting, falling back to the env-configured
/// `config.tmdb_language` (default `en-US`) when unset. The catalog stores ONE
/// language for everyone, so this is the household's metadata language, not a
/// per-user UI choice.
pub fn metadata_language(settings: &Settings, config: &crate::config::Config) -> String {
    let v = settings.get_str("tmdbLanguage", "");
    if v.trim().is_empty() {
        config.tmdb_language.clone()
    } else {
        v
    }
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
/// from the env-configured media dirs. Seeding prefers the typed
/// `LUMA_MOVIES_DIRS` / `LUMA_SERIES_DIRS` roots (one "Films" / "Séries" library
/// each) and always keeps the untyped one-per-folder `LUMA_MEDIA_DIRS` seed for
/// backward compatibility.
pub fn library_defs(settings: &Settings, config: &crate::config::Config) -> Vec<LibraryDef> {
    if let Value::Array(_) = settings.get("libraries") {
        if let Ok(defs) = serde_json::from_value::<Vec<LibraryDef>>(settings.get("libraries")) {
            return defs;
        }
    }
    let mut defs = Vec::new();
    if !config.movies_dirs.is_empty() {
        defs.push(LibraryDef {
            id: crate::services::scan::short_hash("lib|movies"),
            name: "Films".to_string(),
            kind: "movies".to_string(),
            folders: config.movies_dirs.iter().map(|d| d.to_string_lossy().to_string()).collect(),
            auto_scan: true,
        });
    }
    if !config.series_dirs.is_empty() {
        defs.push(LibraryDef {
            id: crate::services::scan::short_hash("lib|shows"),
            name: "Séries".to_string(),
            kind: "shows".to_string(),
            folders: config.series_dirs.iter().map(|d| d.to_string_lossy().to_string()).collect(),
            auto_scan: true,
        });
    }
    defs.extend(config.media_dirs.iter().map(|dir| {
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
    }));
    defs
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
