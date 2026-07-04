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
}

/// Built-in default values for every known setting key. The set of keys here is
/// also the allow-list for [`Settings::set_patch`].
fn defaults() -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    // general
    m.insert("serverName".into(), json!("LUMA"));
    m.insert("uiLanguage".into(), json!("Français"));
    // TMDB metadata language (e.g. "fr-FR"). Empty → fall back to the
    // env-configured `LUMA_TMDB_LANGUAGE` (default "en-US"). One language for the
    // whole catalog the household's metadata language, not a per-user UI choice.
    m.insert("tmdbLanguage".into(), json!(""));
    m.insert("timezone".into(), json!("Europe/Zurich (UTC+1)"));
    m.insert("autoUpdate".into(), json!(true));
    m.insert("updateChannel".into(), json!("Stable"));
    // Library auto-scan on folder changes (the watcher). `watchAutoScan` is the
    // master toggle; `watchIntervalSecs` is the periodic re-scan cadence (the only
    // path that catches NAS/SMB edits, which emit no FS events) `-1` = use the
    // `LUMA_WATCH_INTERVAL` env or 300s default, `0` = FS events only.
    m.insert("watchAutoScan".into(), json!(true));
    m.insert("watchIntervalSecs".into(), json!(-1));
    m.insert("anonStats".into(), json!(false));
    m.insert("showRecentHome".into(), json!(true));
    // Plex-style theme songs: loop a show's title theme under its detail page.
    // Opt-in off until the admin enables it (and a scan downloads the themes).
    m.insert("themeSongs".into(), json!(false));
    // Intro/credits marker detection: off | chapters (free, from embedded
    // chapters) | fingerprint (audio analysis job, heavy). Read by markers.detect.
    m.insert("introDetection".into(), json!("chapters"));
    m.insert("theme".into(), json!("Sombre (Luma)"));
    m.insert("dateFormat".into(), json!("JJ/MM/AAAA"));
    // network
    m.insert("remoteAccess".into(), json!(false));
    m.insert("remoteUrl".into(), json!(""));
    // Managed Cloudflare Tunnel connector (optional, off by default). When enabled
    // with a token, the server supervises a `cloudflared` child (services::remote);
    // the token is a secret (never returned to clients). The binary is provided by
    // the server, not configured here.
    m.insert("remoteAccessToken".into(), json!(""));
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
