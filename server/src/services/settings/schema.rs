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
