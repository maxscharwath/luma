//! Acquisition orchestration: the quality profile from settings, per-indexer
//! capability caching, and the search pipelines (interactive here via
//! [`search`]; the automatic wanted-list pass rides the downloads milestone).

pub mod auto;
pub mod import;
pub mod search;

use std::collections::HashMap;
use std::sync::Mutex;

use luma_scene::{Profile, Res};
use luma_torznab::{Caps, IndexerEndpoint};

use crate::db::IndexerRow;
use crate::services::jobs::now_ms;
use crate::state::SharedState;

const GB: u64 = 1_073_741_824;

/// Build the decision engine's profile from the admin settings.
pub fn profile_from_settings(state: &SharedState) -> Profile {
    let s = &state.settings;
    let resolution = match s.get_str("acqResolution", "1080p").as_str() {
        "720p" => Res::R720,
        "2160p" => Res::R2160,
        _ => Res::R1080,
    };
    let list = |key: &str| -> Vec<String> {
        s.get_str(key, "")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect()
    };
    Profile {
        resolution,
        prefer_hevc: s.get_bool("acqPreferHevc", true),
        min_seeders: s.get_i64("acqMinSeeders", 2).max(0) as u32,
        max_size_bytes_movie: (s.get_i64("acqMaxSizeGbMovie", 15).max(0) as u64) * GB,
        max_size_bytes_episode: (s.get_i64("acqMaxSizeGbEpisode", 3).max(0) as u64) * GB,
        required_keywords: list("acqRequiredKeywords"),
        forbidden_keywords: list("acqForbiddenKeywords"),
    }
}

pub fn endpoint_of(row: &IndexerRow) -> IndexerEndpoint {
    IndexerEndpoint {
        url: row.url.clone(),
        api_key: row.api_key.clone(),
        categories: row.categories.clone(),
    }
}

/// Process-wide `t=caps` cache: capabilities are static per indexer, and every
/// search would otherwise pay an extra round-trip per indexer. Keyed by
/// indexer id + url (so re-pointing an indexer refreshes).
static CAPS_CACHE: Mutex<Option<HashMap<String, Caps>>> = Mutex::new(None);

pub fn indexer_caps(state: &SharedState, row: &IndexerRow) -> anyhow::Result<Caps> {
    let key = format!("{}|{}", row.id, row.url);
    if let Some(caps) = CAPS_CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(caps);
    }
    let result = luma_torznab::caps(&endpoint_of(row));
    match &result {
        Ok(_) => {
            let _ = crate::db::note_indexer_result(&state.db, &row.id, true, None, now_ms());
        }
        Err(e) => {
            let _ =
                crate::db::note_indexer_result(&state.db, &row.id, false, Some(&format!("{e:#}")), now_ms());
        }
    }
    let caps = result?;
    CAPS_CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(key, caps.clone());
    Ok(caps)
}

/// Drop a cached capability entry (config changed / manual test forces a
/// fresh probe).
pub fn invalidate_caps(indexer_id: &str) {
    if let Some(map) = CAPS_CACHE.lock().unwrap().as_mut() {
        map.retain(|k, _| !k.starts_with(&format!("{indexer_id}|")));
    }
}

// ----- built-in (native Cardigann) engine dispatch --------------------------------

/// `kind` value for a native-engine indexer row.
pub const KIND_BUILTIN: &str = "builtin";

/// The runtime definition cache (lives under the data dir).
pub fn definition_store(state: &SharedState) -> luma_indexer::store::DefinitionStore {
    luma_indexer::store::DefinitionStore::new(&state.config.data_dir)
}

/// Build a live [`luma_indexer::Session`] for a `builtin` indexer row: load its
/// definition, seed the config from the stored settings JSON + chosen base
/// link, and wire optional VPN / FlareSolverr transport.
pub fn build_builtin_session(state: &SharedState, row: &IndexerRow) -> anyhow::Result<luma_indexer::Session> {
    let def_id = row
        .definition_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("built-in indexer '{}' has no definition", row.name))?;
    let def = definition_store(state).load(def_id)?;
    let settings: std::collections::HashMap<String, String> =
        serde_json::from_str(&row.settings).unwrap_or_default();
    let cfg = luma_indexer::IndexerConfig { base_url: row.url.clone(), settings };

    // Route search traffic through the VPN bridge only when the admin opted in
    // (a downed tunnel must not silently break search otherwise).
    let socks5 = (state.settings.get_bool("acqIndexersUseVpn", false)
        && crate::services::vpn::Vpn::wg_configured(state))
    .then(|| crate::services::vpn::Vpn::local_proxy_url(state));
    let flaresolverr = {
        let url = state.settings.get_str("acqFlaresolverrUrl", "");
        (!url.trim().is_empty()).then(|| url.trim().to_string())
    };

    Ok(luma_indexer::Session::new(
        &state.config.data_dir,
        &row.id,
        def,
        cfg,
        socks5,
        flaresolverr,
    ))
}

/// Map a Torznab query onto the indexer-engine query (same shapes).
fn to_indexer_query(q: &luma_torznab::Query) -> luma_indexer::Query {
    match q {
        luma_torznab::Query::Movie { tmdb_id, imdb_id, title, year } => luma_indexer::Query::Movie {
            tmdb_id: *tmdb_id,
            imdb_id: imdb_id.clone(),
            title: title.clone(),
            year: *year,
        },
        luma_torznab::Query::Episode { tmdb_id, title, season, episode } => {
            luma_indexer::Query::Episode {
                tmdb_id: *tmdb_id,
                title: title.clone(),
                season: *season,
                episode: *episode,
            }
        }
        luma_torznab::Query::Season { tmdb_id, title, season } => luma_indexer::Query::Season {
            tmdb_id: *tmdb_id,
            title: title.clone(),
            season: *season,
        },
    }
}

/// Normalize a native-engine release into the Torznab release shape the scoring
/// pipeline already consumes.
fn release_from_indexer(r: luma_indexer::Release) -> luma_torznab::Release {
    luma_torznab::Release {
        title: r.title,
        guid: r.guid,
        link: r.link,
        magnet: r.magnet,
        info_hash: r.info_hash,
        size_bytes: r.size_bytes,
        seeders: r.seeders,
        leechers: r.leechers,
        tmdb_id: r.tmdb_id,
        imdb_id: r.imdb_id,
        published_at: r.published_at,
        details_url: r.details_url,
    }
}

/// Run one query against one indexer, whatever its kind, returning normalized
/// releases. This is the single dispatch point the search pipelines call.
pub fn search_indexer(
    state: &SharedState,
    row: &IndexerRow,
    query: &luma_torznab::Query,
) -> anyhow::Result<Vec<luma_torznab::Release>> {
    if row.kind == KIND_BUILTIN {
        let session = build_builtin_session(state, row)?;
        let outcome = session.search(&to_indexer_query(query), &row.categories);
        let note_ok = outcome.errors.is_empty();
        let _ = crate::db::note_indexer_result(
            &state.db,
            &row.id,
            note_ok,
            outcome.errors.first().map(String::as_str),
            now_ms(),
        );
        // Surface an all-error, no-result sweep as an error (so it reads as a
        // broken indexer, not "nothing found").
        if outcome.releases.is_empty() && !outcome.errors.is_empty() {
            anyhow::bail!("{}", outcome.errors.join("; "));
        }
        Ok(outcome.releases.into_iter().map(release_from_indexer).collect())
    } else {
        let caps = indexer_caps(state, row)?;
        luma_torznab::search(&endpoint_of(row), query, &caps)
    }
}

/// Resolve the grabbable target (magnet / .torrent URL) for a built-in
/// release, following the definition's `download` block if the search row
/// carried no direct link.
pub fn resolve_builtin_download(
    state: &SharedState,
    row: &IndexerRow,
    title: &str,
    details_url: Option<&str>,
    magnet_or_url: &str,
) -> anyhow::Result<String> {
    // A magnet is already grabbable as-is.
    if magnet_or_url.starts_with("magnet:") {
        return Ok(magnet_or_url.to_string());
    }
    let session = build_builtin_session(state, row)?;
    let release = luma_indexer::Release {
        title: title.to_string(),
        magnet: magnet_or_url.starts_with("magnet:").then(|| magnet_or_url.to_string()),
        link: (magnet_or_url.starts_with("http")).then(|| magnet_or_url.to_string()),
        details_url: details_url.map(str::to_string),
        ..Default::default()
    };
    match session.resolve_download(&release)? {
        luma_indexer::DownloadTarget::Magnet(m) => Ok(m),
        luma_indexer::DownloadTarget::TorrentUrl(u) => Ok(u),
    }
}

/// Capabilities for any indexer kind (native ones derive from the definition;
/// no network round-trip).
pub fn any_indexer_caps(state: &SharedState, row: &IndexerRow) -> anyhow::Result<Caps> {
    if row.kind == KIND_BUILTIN {
        let def = definition_store(state).load(
            row.definition_id.as_deref().ok_or_else(|| anyhow::anyhow!("no definition"))?,
        )?;
        let c = luma_indexer::Caps::from_definition(&def);
        Ok(Caps {
            search_tmdb: c.search_tmdb,
            search_imdb: c.search_imdb,
            tv_search_tmdb: c.tv_search_tmdb,
            server_title: c.server_title,
        })
    } else {
        indexer_caps(state, row)
    }
}
