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

/// The persisted display name for the server (falls back to "KROMA").
pub fn server_name(settings: &Settings) -> String {
    let n = settings.get_str("serverName", "KROMA");
    if n.trim().is_empty() {
        "KROMA".to_string()
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

// ----- remote access (managed Cloudflare Tunnel connector) --------------------

/// Whether the managed `cloudflared` connector is enabled (off by default). When
/// on and a token is stored, the server supervises the tunnel (see
/// the `kroma-remote` crate). Installs with their own tunnel leave this off.
pub fn remote_access_enabled(settings: &Settings) -> bool {
    settings.get_bool("remoteAccess", false)
}

/// The public base URL clients reach the server at (e.g. `https://kroma.example.com`),
/// used for share / Quick Connect links. Trailing slash trimmed; empty if unset.
pub fn public_url(settings: &Settings) -> String {
    settings.get_str("remoteUrl", "").trim().trim_end_matches('/').to_string()
}

/// The stored Cloudflare Tunnel token for the managed connector. Secret never
/// returned to clients (the admin API exposes only a `hasToken` bool).
pub fn remote_access_token(settings: &Settings) -> String {
    settings.get_str("remoteAccessToken", "")
}

/// Persist the remote-access config. `token` is `Some` only when the admin
/// provided a new value a blank field keeps the stored secret rather than wiping
/// it (mirrors the LLM API-key handling). The `cloudflared` binary is provided by
/// the server, so there is no configurable path.
pub fn set_remote_config(
    settings: &Settings,
    pool: &Pool,
    enabled: bool,
    url: &str,
    token: Option<&str>,
) {
    let mut patch = BTreeMap::new();
    patch.insert("remoteAccess".to_string(), json!(enabled));
    patch.insert("remoteUrl".to_string(), json!(url.trim()));
    if let Some(tok) = token {
        patch.insert("remoteAccessToken".to_string(), json!(tok));
    }
    settings.set_patch(pool, patch);
}

// ----- library definitions (persisted, multi-folder) --------------------------

/// A named, runtime-editable library spanning one or more scan folders. Persisted
/// in the settings store under the `libraries` key, seeded from `KROMA_MEDIA_DIRS`
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
/// `KROMA_MOVIES_DIRS` / `KROMA_SERIES_DIRS` roots (one "Films" / "Séries" library
/// each) and always keeps the untyped one-per-folder `KROMA_MEDIA_DIRS` seed for
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-settings-acc-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::db::init(&path).unwrap()
    }

    fn settings(pool: &Pool) -> Settings {
        Settings::load(pool)
    }

    /// A Config with explicit fields (avoids reading the process env).
    fn test_config() -> crate::config::Config {
        crate::config::Config {
            host: "0.0.0.0".to_string(),
            port: 4040,
            media_dirs: Vec::new(),
            movies_dirs: Vec::new(),
            series_dirs: Vec::new(),
            data_dir: PathBuf::from("/tmp/kroma-data"),
            tmdb_api_key: None,
            tmdb_language: "en-US".to_string(),
            tmdb_enrich: false,
            web_url: None,
            web_dir: None,
        }
    }

    #[test]
    fn local_networks_splits_default_and_custom() {
        let pool = test_pool();
        let s = settings(&pool);
        // default
        let nets = local_networks(&s);
        assert!(nets.contains(&"192.168.0.0/16".to_string()));
        assert_eq!(nets.len(), 3);
        // custom: comma AND space separated, empties dropped
        s.set_patch(&pool, BTreeMap::from([("localNetworks".to_string(), json!("10.0.0.0/8,  172.16.0.0/12 ,"))]));
        let nets = local_networks(&s);
        assert_eq!(nets, vec!["10.0.0.0/8".to_string(), "172.16.0.0/12".to_string()]);
    }

    #[test]
    fn server_name_defaults_and_guards_blank() {
        let pool = test_pool();
        let s = settings(&pool);
        assert_eq!(server_name(&s), "KROMA");
        s.set_patch(&pool, BTreeMap::from([("serverName".to_string(), json!("Home"))]));
        assert_eq!(server_name(&s), "Home");
        s.set_patch(&pool, BTreeMap::from([("serverName".to_string(), json!("   "))]));
        assert_eq!(server_name(&s), "KROMA"); // blank -> fallback
    }

    #[test]
    fn max_transcodes_parses_and_clamps() {
        let pool = test_pool();
        let s = settings(&pool);
        assert_eq!(max_transcodes(&s), 8); // default "8"
        s.set_patch(&pool, BTreeMap::from([("maxConcurrent".to_string(), json!("12"))]));
        assert_eq!(max_transcodes(&s), 12);
        s.set_patch(&pool, BTreeMap::from([("maxConcurrent".to_string(), json!(0))]));
        assert_eq!(max_transcodes(&s), 1); // clamped up
        s.set_patch(&pool, BTreeMap::from([("maxConcurrent".to_string(), json!(999))]));
        assert_eq!(max_transcodes(&s), 32); // clamped down
    }

    #[test]
    fn media_workers_explicit_and_auto() {
        let pool = test_pool();
        let s = settings(&pool);
        // default 0 -> auto capacity (cores-1, floored at 1)
        assert!(media_workers(&s) >= 1);
        s.set_patch(&pool, BTreeMap::from([("mediaConcurrency".to_string(), json!("4"))]));
        assert_eq!(media_workers(&s), 4);
        s.set_patch(&pool, BTreeMap::from([("mediaConcurrency".to_string(), json!(99))]));
        assert_eq!(media_workers(&s), 32); // clamped
    }

    #[test]
    fn transcode_cache_limit_parses_decimal_and_unlimited() {
        let pool = test_pool();
        let s = settings(&pool);
        assert_eq!(transcode_cache_limit_bytes(&s), 20_000_000_000); // default "20 Go"
        s.set_patch(&pool, BTreeMap::from([("transcodeCacheLimit".to_string(), json!("1.5 Go"))]));
        assert_eq!(transcode_cache_limit_bytes(&s), 1_500_000_000);
        s.set_patch(&pool, BTreeMap::from([("transcodeCacheLimit".to_string(), json!("Illimité"))]));
        assert_eq!(transcode_cache_limit_bytes(&s), 0); // non-numeric -> unlimited
        s.set_patch(&pool, BTreeMap::from([("transcodeCacheLimit".to_string(), json!("0 Go"))]));
        assert_eq!(transcode_cache_limit_bytes(&s), 0);
    }

    #[test]
    fn simple_bool_and_url_accessors() {
        let pool = test_pool();
        let s = settings(&pool);
        assert!(!theme_songs_enabled(&s));
        assert!(!remote_access_enabled(&s));
        assert_eq!(public_url(&s), "");
        assert_eq!(remote_access_token(&s), "");
        s.set_patch(&pool, BTreeMap::from([
            ("themeSongs".to_string(), json!(true)),
            ("remoteAccess".to_string(), json!(true)),
            ("remoteUrl".to_string(), json!("https://kroma.example.com/")),
            ("remoteAccessToken".to_string(), json!("secret")),
        ]));
        assert!(theme_songs_enabled(&s));
        assert!(remote_access_enabled(&s));
        assert_eq!(public_url(&s), "https://kroma.example.com"); // trailing slash trimmed
        assert_eq!(remote_access_token(&s), "secret");
    }

    #[test]
    fn metadata_language_falls_back_to_config() {
        let pool = test_pool();
        let s = settings(&pool);
        let cfg = test_config();
        assert_eq!(metadata_language(&s, &cfg), "en-US"); // unset -> config
        s.set_patch(&pool, BTreeMap::from([("tmdbLanguage".to_string(), json!("fr-FR"))]));
        assert_eq!(metadata_language(&s, &cfg), "fr-FR");
    }

    #[test]
    fn set_remote_config_merges_token() {
        let pool = test_pool();
        let s = settings(&pool);
        set_remote_config(&s, &pool, true, "  https://a.b/  ", Some("tok1"));
        assert!(remote_access_enabled(&s));
        assert_eq!(public_url(&s), "https://a.b");
        assert_eq!(remote_access_token(&s), "tok1");
        // None keeps the stored token.
        set_remote_config(&s, &pool, false, "https://a.b", None);
        assert!(!remote_access_enabled(&s));
        assert_eq!(remote_access_token(&s), "tok1");
    }

    #[test]
    fn library_defs_seed_from_config_when_unset() {
        let pool = test_pool();
        let s = settings(&pool); // libraries default null
        let mut cfg = test_config();
        cfg.movies_dirs = vec![PathBuf::from("/media/films")];
        cfg.series_dirs = vec![PathBuf::from("/media/series")];
        cfg.media_dirs = vec![PathBuf::from("/media/Misc")];
        let defs = library_defs(&s, &cfg);
        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].name, "Films");
        assert_eq!(defs[0].kind, "movies");
        assert_eq!(defs[1].name, "Séries");
        assert_eq!(defs[1].kind, "shows");
        // untyped media dir seeds a library named after the folder, empty kind
        assert_eq!(defs[2].name, "Misc");
        assert!(defs[2].kind.is_empty());
    }

    #[test]
    fn library_defs_round_trip_persisted() {
        let pool = test_pool();
        let s = settings(&pool);
        let cfg = test_config();
        let defs = vec![LibraryDef {
            id: "lib1".to_string(),
            name: "Cinema".to_string(),
            kind: "movies".to_string(),
            folders: vec!["/a".to_string(), "/b".to_string()],
            auto_scan: false,
        }];
        set_library_defs(&s, &pool, &defs);
        let got = library_defs(&s, &cfg);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Cinema");
        assert!(!got[0].auto_scan);
        // all_folders flattens
        let folders = all_folders(&s, &cfg);
        assert_eq!(folders, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn library_def_auto_scan_defaults_true_on_deserialize() {
        // A def JSON without autoScan defaults to true.
        let d: LibraryDef =
            serde_json::from_value(json!({"id":"x","name":"N","folders":["/f"]})).unwrap();
        assert!(d.auto_scan);
        assert!(d.kind.is_empty());
    }
}
