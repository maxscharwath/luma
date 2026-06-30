//! The admin settings-view schema: the grouped, localised rows the console
//! renders for the `general` / `network` / `transcoder` views, with each row's
//! current value overlaid from the store and a few computed values from config.

use serde::Serialize;
use serde_json::{json, Value};
use ts_rs::TS;

use crate::i18n;

use super::store::Settings;

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
    // intentionally NOT translated only labels, hints, group titles & descs.
    let t = |key: &str| i18n::t(locale, key, &[]);
    let group = |title: &str, desc: Option<&str>, rows: Vec<SettingRow>| SettingGroup {
        title: t(title),
        desc: desc.map(t),
        rows,
    };
    // NB: only settings the server actually enforces are surfaced here. Stored-but-
    // unused controls (theme, timezone, hwAccel, https, …) were removed rather than
    // shown with a "preference saved only" badge the server is remux-only, so the
    // hardware-encode controls have nothing to drive.
    match view {
        "general" => vec![
            group(
                "admin.serverIdentity",
                Some("admin.serverIdentityDesc"),
                vec![
                    row("serverName", t("admin.serverName"), Some(t("admin.serverNameHint")), "text", &[], g("serverName"), true),
                    row("tmdbLanguage", t("admin.tmdbLanguage"), Some(t("admin.tmdbLanguageHint")), "text", &[], g("tmdbLanguage"), true),
                    row("version", t("admin.version"), None, "value", &[], json!(env!("CARGO_PKG_VERSION")), true),
                ],
            ),
            group(
                "admin.preferences",
                None,
                vec![
                    row("watchAutoScan", t("admin.watchAutoScan"), Some(t("admin.watchAutoScanHint")), "toggle", &[], g("watchAutoScan"), true),
                    row("showRecentHome", t("admin.showRecentHome"), None, "toggle", &[], g("showRecentHome"), true),
                    row("themeSongs", t("admin.themeSongs"), Some(t("admin.themeSongsHint")), "toggle", &[], g("themeSongs"), true),
                    row("introDetection", t("admin.introDetection"), Some(t("admin.introDetectionHint")), "select", &["off", "chapters", "fingerprint"], g("introDetection"), true),
                ],
            ),
        ],
        "network" => vec![group(
            "admin.portsDiscovery",
            None,
            vec![
                row("publicAddress", t("admin.publicAddress"), None, "value", &[], json!(public_address(config)), true),
                row("port", t("admin.port"), Some(t("admin.portHint")), "value", &[], json!(config.port.to_string()), true),
                row("localDiscovery", t("admin.localDiscovery"), Some(t("admin.localDiscoveryHint")), "toggle", &[], g("localDiscovery"), true),
                row("localNetworks", t("admin.localNetworks"), Some(t("admin.localNetworksHint")), "text", &[], g("localNetworks"), true),
            ],
        )],
        "transcoder" => vec![group(
            "admin.qualityPerf",
            Some("admin.qualityPerfDesc"),
            vec![
                row("maxConcurrent", t("admin.maxConcurrent"), Some(t("admin.maxConcurrentHint")), "select", &["2", "4", "8", "12", "16", "24", "32"], g("maxConcurrent"), true),
                row("transcodeDir", t("admin.transcodeDir"), None, "value", &[], json!(transcode_dir(config)), true),
            ],
        )],
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
