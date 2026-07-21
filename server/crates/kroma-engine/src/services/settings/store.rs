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

    /// Reload the live map from the database, over the built-in defaults. Used
    /// after a backup import writes the `settings` rows directly (bypassing
    /// [`Self::set_patch`]), so the in-memory store reflects the restored config.
    pub fn reload(&self, pool: &Pool) {
        let mut map = defaults();
        if let Ok(rows) = crate::db::settings_all(pool) {
            for (k, v) in rows {
                map.insert(k, v);
            }
        }
        *self.inner.write().unwrap() = map;
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

    /// Atomically read-modify-write one setting under a single write-lock. `f`
    /// receives the current value (or its built-in default) and returns the new
    /// value. Unlike a `get` + `set_patch` pair, no other writer can interleave
    /// between the read and the write, so concurrent updates to the same key
    /// (e.g. two module-state edits) can't clobber each other. Only keys in
    /// [`defaults`] are persisted, matching [`Self::set_patch`].
    pub fn update_json(&self, pool: &Pool, key: &str, f: impl FnOnce(Value) -> Value) {
        if !defaults().contains_key(key) {
            return;
        }
        let mut guard = self.inner.write().unwrap();
        let current = guard
            .get(key)
            .cloned()
            .or_else(|| defaults().get(key).cloned())
            .unwrap_or(Value::Null);
        let next = f(current);
        let _ = crate::db::settings_set(pool, key, &next);
        guard.insert(key.to_string(), next);
    }
}

/// Built-in default values for every known setting key. The set of keys here is
/// also the allow-list for [`Settings::set_patch`].
fn defaults() -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    // general
    m.insert("serverName".into(), json!("KROMA"));
    // Per-module admin state: { "<id>": { "enabled": bool, "config": {..} } }.
    // One allow-listed key holding the whole module-state blob (module ids are
    // not known at compile time, so they can't each be an allow-listed key).
    m.insert("moduleStates".into(), json!({}));
    // Module Store registry override. Empty = the built-in default (the
    // modules.json attached to this repo's GitHub Releases); any URL serving a
    // catalog (schema 1 or 2) works, so third-party stores are one setting away.
    m.insert("moduleRegistryUrl".into(), json!(""));
    m.insert("uiLanguage".into(), json!("Français"));
    // TMDB metadata language (e.g. "fr-FR"). Empty → fall back to the
    // env-configured `KROMA_TMDB_LANGUAGE` (default "en-US"). One language for the
    // whole catalog the household's metadata language, not a per-user UI choice.
    m.insert("tmdbLanguage".into(), json!(""));
    m.insert("timezone".into(), json!("Europe/Zurich (UTC+1)"));
    m.insert("autoUpdate".into(), json!(true));
    m.insert("updateChannel".into(), json!("Stable"));
    // Library auto-scan on folder changes (the watcher). `watchAutoScan` is the
    // master toggle; `watchIntervalSecs` is the periodic re-scan cadence (the only
    // path that catches NAS/SMB edits, which emit no FS events) `-1` = use the
    // `KROMA_WATCH_INTERVAL` env or 300s default, `0` = FS events only.
    m.insert("watchAutoScan".into(), json!(true));
    m.insert("watchIntervalSecs".into(), json!(-1));
    m.insert("anonStats".into(), json!(false));
    m.insert("showRecentHome".into(), json!(true));
    // Security: expose the account roster on the login screen (the "Qui regarde ?"
    // profile picker). Off by default so knowing the server URL does not reveal
    // who has an account; when off, `GET /api/users` returns an empty list and
    // clients fall back to a plain email/password sign-in. Read by the accounts
    // API (`list_users` / `auth_config`).
    m.insert("publicUserList".into(), json!(false));
    // Plex-style theme songs: loop a show's title theme under its detail page.
    // Opt-in off until the admin enables it (and a scan downloads the themes).
    m.insert("themeSongs".into(), json!(false));
    // Intro/credits marker detection: off | chapters (free, from embedded
    // chapters) | fingerprint (audio analysis job, heavy). Read by markers.detect.
    m.insert("introDetection".into(), json!("chapters"));
    m.insert("theme".into(), json!("Sombre (Kroma)"));
    m.insert("dateFormat".into(), json!("JJ/MM/AAAA"));
    // network
    // Auto-update installed .kmod modules to the newest compatible catalog
    // version at boot, so a server update keeps the modules current on its own.
    m.insert("moduleAutoUpdate".into(), json!(true));
    m.insert("remoteAccess".into(), json!(false));
    m.insert("remoteUrl".into(), json!(""));
    // Managed Cloudflare Tunnel connector (optional, off by default). When enabled
    // with a token, the server supervises a `cloudflared` child (services::remote);
    // the token is a secret (never returned to clients). The binary is provided by
    // the server, not configured here.
    m.insert("remoteAccessToken".into(), json!(""));
    m.insert("upLimit".into(), json!("Illimité"));
    m.insert("https".into(), json!("Préférées"));
    // Optional HTTPS listener with an auto-generated self-signed certificate,
    // for LAN use where a secure origin unlocks the Web Crypto API (passkeys /
    // subtle crypto refuse to run over plain HTTP on a non-localhost host). Off
    // by default; the plain-HTTP port keeps serving either way. Applied at boot,
    // so a change needs a server restart (`KROMA_HTTPS` / `KROMA_HTTPS_PORT` env
    // override the stored values). See src/tls.rs.
    m.insert("httpsEnabled".into(), json!(false));
    m.insert("httpsPort".into(), json!("4443"));
    m.insert("httpsRedirect".into(), json!(false));
    m.insert("ipv6".into(), json!(false));
    m.insert("localDiscovery".into(), json!(true));
    m.insert("localNetworks".into(), json!("192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12"));
    // transcoder
    m.insert("hwAccel".into(), json!(false));
    m.insert("hwDevice".into(), json!("Auto"));
    m.insert("hevcEncode".into(), json!(false));
    m.insert("transcoderSpeed".into(), json!("Automatique"));
    m.insert("bgQuality".into(), json!("Préférer la vitesse"));
    m.insert("maxConcurrent".into(), json!("8"));
    // How many CPU-heavy background media passes (storyboard/subtitle/marker
    // ffmpeg) run at once. "0" = auto (cores - 1); raise/lower to trade nightly
    // throughput for how usable the box stays during processing (see
    // infra::ffmpeg_gate).
    m.insert("mediaConcurrency".into(), json!("0"));
    // Global "hold all background media processing" switch (admin Pipeline
    // console). Persisted so a pause survives a restart; the dispatcher parks
    // every stage while set. Not exposed as a normal settings row it is toggled
    // via the pipeline pause/resume action.
    m.insert("pipelinePaused".into(), json!(false));
    m.insert("deleteAfter".into(), json!(true));
    // storage / cache
    m.insert("cacheLimit".into(), json!("80 Go"));
    // Byte budget for the on-disk transcode (HLS segment) cache. Regenerable, so
    // kept small: idle / superseded sessions are evicted oldest-first once over
    // this; an actively-playing session is never interrupted (see infra::hls).
    m.insert("transcodeCacheLimit".into(), json!("20 Go"));
    // jobs: scheduler timezone offset in minutes from UTC (cron `0 4 * * *`
    // means 4am at this offset). 0 = UTC; e.g. 60 = UTC+1, -300 = UTC-5.
    m.insert("jobsUtcOffset".into(), json!(0));
    // acquisition: the release decision engine's quality profile (see
    // services::acquisition + the kroma-scene crate). KROMA is HEVC-first, so
    // x265 releases outrank x264 by default. Keyword lists are comma-separated,
    // matched as whole tokens against release names.
    m.insert("acqEnabled".into(), json!(false));
    m.insert("acqAutoApprove".into(), json!(false));
    // Remove the torrent + its downloaded data once imported into the library
    // (frees the download folder + stops seeding; the library copy survives).
    m.insert("acqDeleteAfterImport".into(), json!(false));
    m.insert("acqResolution".into(), json!("1080p"));
    m.insert("acqPreferHevc".into(), json!(true));
    m.insert("acqMinSeeders".into(), json!(2));
    m.insert("acqMaxSizeGbMovie".into(), json!(15));
    m.insert("acqMaxSizeGbEpisode".into(), json!(3));
    m.insert("acqRequiredKeywords".into(), json!(""));
    m.insert(
        "acqForbiddenKeywords".into(),
        json!("cam, hdcam, ts, telesync, telecine, screener, dvdscr, workprint"),
    );
    // Embedded torrent engine knobs (0 = ephemeral port / unlimited rate).
    // Applied on engine (re)start; the settings PUT hook restarts it live.
    m.insert("rqbitPort".into(), json!(0));
    m.insert("rqbitDownKbps".into(), json!(0));
    m.insert("rqbitUpKbps".into(), json!(0));
    // VPN routing for torrent traffic (see services::downloads/vpn): a pasted
    // WireGuard config that KROMA bridges to a local SOCKS5 via a managed
    // wireproxy child, which the embedded engine routes peers through. This is
    // the single VPN path (no raw external proxy option). `vpnWgConfig` is a
    // secret: stored here, written via /api/admin/vpn, never in a settings view.
    // `vpnLocalPort` is the internal bridge port (implementation detail).
    m.insert("vpnWgConfig".into(), json!(""));
    m.insert("vpnLocalPort".into(), json!(25345));
    m.insert("vpnKillSwitch".into(), json!(false));
    m.insert("vpnCheckUrl".into(), json!("https://api.ipify.org"));
    // Import targets: the library (by name) new downloads land in; "Auto" =
    // first library of the matching kind.
    m.insert("acqMovieLibrary".into(), json!("Auto"));
    m.insert("acqSeriesLibrary".into(), json!("Auto"));
    // File naming templates (Sonarr/Radarr-style tokens), used by import and
    // the library rename tool. See kroma_torrent::organize::naming for the tokens.
    m.insert("namingMovieFolder".into(), json!("{Title} ({Year})"));
    m.insert("namingMovieFile".into(), json!("{Title} ({Year}) {Quality Full}"));
    m.insert("namingSeriesFolder".into(), json!("{Title} ({Year})"));
    m.insert("namingSeasonFolder".into(), json!("Season {season:00}"));
    m.insert(
        "namingEpisodeFile".into(),
        json!("{Title} - S{season:00}E{episode:00} - {Episode Title} {Quality Full}"),
    );
    // AI / LLM: powers personalized auto-named home sections + per-user taste
    // profiles (the `sections.personalize` job). Open-ended provider choice:
    // openai = any OpenAI-compatible server (Ollama, llama.cpp, LM Studio, …);
    // anthropic = Claude. Off until configured.
    m.insert("llmEnabled".into(), json!(false));
    m.insert("llmProvider".into(), json!("openai"));
    m.insert("llmBaseUrl".into(), json!("")); // e.g. http://localhost:11434/v1 (Ollama)
    m.insert("llmModel".into(), json!("")); // e.g. qwen2.5:1.5b-instruct, or claude-haiku-4-5
    m.insert("llmApiKey".into(), json!(""));
    // generation controls
    m.insert("llmTemperature".into(), json!(0.7)); // OpenAI-compatible sampling temp
    m.insert("llmMaxTokens".into(), json!(900)); // output cap per completion
    m.insert("llmReasoning".into(), json!(false)); // Anthropic adaptive thinking (Claude 4.6+)
    // multi-provider: the editable list of LLM providers + the id of the default
    // one used for generation. Seeded (migrated) from the flat `llm*` keys above
    // on first read when empty. See `settings::llm`.
    m.insert("llmProviders".into(), json!([]));
    m.insert("llmDefaultProvider".into(), json!(""));
    // libraries: persisted multi-folder definitions (seeded from env on first run).
    m.insert("libraries".into(), json!(null));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-settings-store-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::db::init(&path).unwrap()
    }

    #[test]
    fn defaults_carry_known_keys() {
        let d = defaults();
        assert_eq!(d.get("serverName"), Some(&json!("KROMA")));
        assert_eq!(d.get("moduleStates"), Some(&json!({})));
        assert_eq!(d.get("watchIntervalSecs"), Some(&json!(-1)));
        assert_eq!(d.get("llmTemperature"), Some(&json!(0.7)));
        assert!(d.get("nonexistentKey").is_none());
    }

    #[test]
    fn get_falls_back_to_default_then_null() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        // A known key with no persisted row reads its built-in default.
        assert_eq!(s.get("serverName"), json!("KROMA"));
        // An unknown key is neither stored nor defaulted -> Null.
        assert_eq!(s.get("totallyUnknown"), Value::Null);
    }

    #[test]
    fn typed_getters_coerce_and_fall_back() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        // bool
        assert!(s.get_bool("watchAutoScan", false)); // default true
        assert!(!s.get_bool("anonStats", true)); // default false
        assert!(s.get_bool("missingBool", true)); // missing -> fallback
        // serverName is a string, not a bool -> fallback
        assert!(s.get_bool("serverName", true));
        // str
        assert_eq!(s.get_str("serverName", "x"), "KROMA");
        assert_eq!(s.get_str("anonStats", "fb"), "fb"); // non-string -> fallback
        assert_eq!(s.get_str("missingStr", "fb"), "fb");
        // i64: numeric default, string-numeric parse, non-numeric fallback
        assert_eq!(s.get_i64("watchIntervalSecs", 99), -1); // default -1
        assert_eq!(s.get_i64("maxConcurrent", 0), 8); // default "8" string parsed
        assert_eq!(s.get_i64("serverName", 42), 42); // "KROMA" not numeric -> fallback
        assert_eq!(s.get_i64("missingI64", 7), 7);
    }

    #[test]
    fn set_patch_keeps_known_skips_unknown_and_persists() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        let mut patch = BTreeMap::new();
        patch.insert("serverName".to_string(), json!("MyBox"));
        patch.insert("bogusKey".to_string(), json!(123));
        let mut written = s.set_patch(&pool, patch);
        written.sort();
        assert_eq!(written, vec!["serverName".to_string()]); // bogusKey dropped
        assert_eq!(s.get("serverName"), json!("MyBox"));
        // bogus key never entered the store.
        assert_eq!(s.get("bogusKey"), Value::Null);
        // Persisted: a fresh load from the same DB reflects the change.
        let s2 = Settings::load(&pool);
        assert_eq!(s2.get("serverName"), json!("MyBox"));
    }

    #[test]
    fn update_json_read_modify_writes_known_key_only() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        // watchIntervalSecs default -1 -> +5 = 4.
        s.update_json(&pool, "watchIntervalSecs", |v| json!(v.as_i64().unwrap_or(0) + 5));
        assert_eq!(s.get_i64("watchIntervalSecs", 0), 4);
        // persisted
        assert_eq!(Settings::load(&pool).get_i64("watchIntervalSecs", 0), 4);
        // Unknown key: no-op, stays Null.
        s.update_json(&pool, "notAKey", |_| json!("x"));
        assert_eq!(s.get("notAKey"), Value::Null);
    }

    #[test]
    fn reload_picks_up_direct_db_writes() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        assert_eq!(s.get("serverName"), json!("KROMA"));
        // Write straight to the DB (bypassing set_patch), then reload.
        crate::db::settings_set(&pool, "serverName", &json!("Restored")).unwrap();
        s.reload(&pool);
        assert_eq!(s.get("serverName"), json!("Restored"));
    }

    #[test]
    fn cloned_handle_shares_the_same_map() {
        let pool = test_pool();
        let s = Settings::load(&pool);
        let clone = s.clone();
        s.set_patch(&pool, BTreeMap::from([("serverName".to_string(), json!("Shared"))]));
        assert_eq!(clone.get("serverName"), json!("Shared"));
    }
}
