//! Acquisition orchestration: the quality profile from settings and the search
//! DISPATCH (interactive here via [`search`]; the automatic wanted-list pass in
//! [`auto`]). The per-indexer capability caching + native-engine session building
//! moved to the Indexers module (`luma_indexer::admin`); this calls into it.

pub mod auto;
pub mod import;
pub mod jobs;
pub mod search;

use luma_scene::{Profile, Res};

use crate::db::IndexerRow;
use luma_engine::services::jobs::now_ms;
use luma_engine::state::SharedState;

const GB: u64 = 1_073_741_824;

/// Resolve the module's download manager from the host service registry. It was
/// a direct `AppState` field until acquisition moved out of the core crate; now
/// every acquisition path that needs it looks it up by type through `HostCtx`.
fn downloads(state: &SharedState) -> std::sync::Arc<crate::DownloadManager> {
    luma_module_host::service::<crate::DownloadManager>(&**state)
        .expect("download manager registered")
}

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
    if row.kind == luma_indexer::admin::KIND_BUILTIN {
        let session = luma_indexer::admin::builtin_session(state, row)?;
        let outcome = session.search(&to_indexer_query(query), &row.categories);
        // Healthy if we got releases (a partial per-path error alongside real
        // results must not flag the indexer as broken) or the sweep was clean.
        let note_ok = !outcome.releases.is_empty() || outcome.errors.is_empty();
        let _ = crate::db::note_indexer_result(
            &state.db,
            &row.id,
            note_ok,
            if note_ok { None } else { outcome.errors.first().map(String::as_str) },
            now_ms(),
        );
        // Surface an all-error, no-result sweep as an error (so it reads as a
        // broken indexer, not "nothing found").
        if outcome.releases.is_empty() && !outcome.errors.is_empty() {
            anyhow::bail!("{}", outcome.errors.join("; "));
        }
        Ok(outcome.releases.into_iter().map(release_from_indexer).collect())
    } else {
        let caps = luma_indexer::admin::indexer_caps(state, row)?;
        luma_torznab::search(&luma_indexer::admin::endpoint_of(row), query, &caps)
    }
}

/// Resolve the grabbable target (magnet / .torrent URL) for a built-in release,
/// following the definition's `download` block if the search row carried no
/// direct link.
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
    let session = luma_indexer::admin::builtin_session(state, row)?;
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
