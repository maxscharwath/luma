//! The admin settings-view schema: the grouped, localised rows the console
//! renders for the `general` / `network` / `transcoder` views, with each row's
//! current value overlaid from the store and a few computed values from config.

use serde::Serialize;
use serde_json::{json, Value};

use crate::i18n;

use super::store::Settings;

/// One editable (or read-only) setting row.
#[derive(Debug, Clone, Serialize)]
pub struct SettingRow {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// `toggle` | `select` | `text` | `value`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    pub value: Value,
    /// Whether the server actually enforces this setting (vs. stored-only).
    pub applied: bool,
}

/// A titled group of rows.
#[derive(Debug, Clone, Serialize)]
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
                    row("publicUserList", t("admin.publicUserList"), Some(t("admin.publicUserListHint")), "toggle", &[], g("publicUserList"), true),
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
                row("mediaConcurrency", t("admin.mediaConcurrency"), Some(t("admin.mediaConcurrencyHint")), "select", &["0", "1", "2", "3", "4", "6", "8", "12", "16"], g("mediaConcurrency"), true),
                row("transcodeDir", t("admin.transcodeDir"), None, "value", &[], json!(transcode_dir(config)), true),
            ],
        )],
        "acquisition" => {
            // Import-target selects offer the configured libraries by name
            // ("Auto" = first library of the matching kind).
            let libs = super::library_defs(settings, config);
            let lib_options = |kind: &str| -> Vec<String> {
                let mut opts = vec!["Auto".to_string()];
                opts.extend(libs.iter().filter(|d| d.kind == kind || d.kind.is_empty()).map(|d| d.name.clone()));
                opts
            };
            let movie_opts = lib_options("movies");
            let show_opts = lib_options("shows");
            vec![
            group(
                "admin.acqGeneral",
                Some("admin.acqGeneralDesc"),
                vec![
                    row("acqEnabled", t("admin.acqEnabled"), Some(t("admin.acqEnabledHint")), "toggle", &[], g("acqEnabled"), true),
                    row("acqAutoApprove", t("admin.acqAutoApprove"), Some(t("admin.acqAutoApproveHint")), "toggle", &[], g("acqAutoApprove"), true),
                    row("acqDeleteAfterImport", t("admin.acqDeleteAfterImport"), Some(t("admin.acqDeleteAfterImportHint")), "toggle", &[], g("acqDeleteAfterImport"), true),
                    row("acqMovieLibrary", t("admin.acqMovieLibrary"), None, "select", &movie_opts.iter().map(String::as_str).collect::<Vec<_>>(), g("acqMovieLibrary"), true),
                    row("acqSeriesLibrary", t("admin.acqSeriesLibrary"), None, "select", &show_opts.iter().map(String::as_str).collect::<Vec<_>>(), g("acqSeriesLibrary"), true),
                ],
            ),
            group(
                "admin.acqQuality",
                Some("admin.acqQualityDesc"),
                vec![
                    row("acqResolution", t("admin.acqResolution"), None, "select", &["720p", "1080p", "2160p"], g("acqResolution"), true),
                    row("acqPreferHevc", t("admin.acqPreferHevc"), Some(t("admin.acqPreferHevcHint")), "toggle", &[], g("acqPreferHevc"), true),
                    row("acqMinSeeders", t("admin.acqMinSeeders"), None, "select", &["0", "1", "2", "5", "10"], g("acqMinSeeders"), true),
                    row("acqMaxSizeGbMovie", t("admin.acqMaxSizeGbMovie"), None, "select", &["5", "10", "15", "25", "40", "80"], g("acqMaxSizeGbMovie"), true),
                    row("acqMaxSizeGbEpisode", t("admin.acqMaxSizeGbEpisode"), None, "select", &["1", "2", "3", "5", "8"], g("acqMaxSizeGbEpisode"), true),
                    row("acqRequiredKeywords", t("admin.acqRequiredKeywords"), Some(t("admin.acqRequiredKeywordsHint")), "text", &[], g("acqRequiredKeywords"), true),
                    row("acqForbiddenKeywords", t("admin.acqForbiddenKeywords"), Some(t("admin.acqForbiddenKeywordsHint")), "text", &[], g("acqForbiddenKeywords"), true),
                ],
            ),
            group(
                "admin.acqEngine",
                Some("admin.acqEngineDesc"),
                vec![
                    row("rqbitPort", t("admin.rqbitPort"), Some(t("admin.rqbitPortHint")), "text", &[], g("rqbitPort"), true),
                    row("rqbitDownKbps", t("admin.rqbitDownKbps"), Some(t("admin.rqbitRateHint")), "text", &[], g("rqbitDownKbps"), true),
                    row("rqbitUpKbps", t("admin.rqbitUpKbps"), Some(t("admin.rqbitRateHint")), "text", &[], g("rqbitUpKbps"), true),
                ],
            ),
        ]
        }
        // The VPN is global to several flows (torrent downloads + optional
        // indexer routing), so its toggles live in their own section (the
        // WireGuard config itself is the dedicated `/admin/vpn` API).
        "vpn" => vec![
            group(
                "admin.acqVpn",
                Some("admin.acqVpnDesc"),
                vec![
                    row("vpnKillSwitch", t("admin.vpnKillSwitch"), Some(t("admin.vpnKillSwitchHint")), "toggle", &[], g("vpnKillSwitch"), true),
                    row("vpnCheckUrl", t("admin.vpnCheckUrl"), None, "text", &[], g("vpnCheckUrl"), true),
                    row("acqIndexersUseVpn", t("admin.vpnRouteIndexers"), Some(t("admin.vpnRouteIndexersHint")), "toggle", &[], g("acqIndexersUseVpn"), true),
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
    config.data_dir.join("hls").to_string_lossy().to_string()
}

fn public_address(config: &crate::config::Config) -> String {
    config
        .web_url
        .clone()
        .unwrap_or_else(|| format!(":{}", config.port))
}
