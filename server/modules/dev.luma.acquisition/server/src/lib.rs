//! Acquisition orchestration: the quality profile from settings and the search
//! DISPATCH (interactive via [`search`]; the automatic wanted-list pass in
//! [`auto`]), plus grab + import. Extracted out of the `luma-torrent` crate so
//! disabling the Acquisition module gates the whole search / grab / auto feature.
//!
//! The coupling is one-way and clean: acquisition resolves the
//! [`luma_torrent::DownloadManager`] through the host service registry, and the
//! Downloads module (engine + queue) NEVER calls acquisition. So this crate
//! depends on `luma-torrent`, never the reverse (no cycle). The per-indexer
//! capability caching + native-engine session building live in the Indexers
//! module (`luma_indexer::admin`); this calls into it.

// The axum `Response` is intentionally the Err type of request guards so handlers
// short-circuit with `?`; boxing every guard for `result_large_err` would churn
// dozens of signatures for no real gain on these error paths.
#![allow(clippy::result_large_err)]

pub mod auto;
pub mod dtos;
pub mod import;
pub mod jobs;
pub mod routes;
pub mod search;

pub use dtos::*;

use luma_module_sdk::scene::{Profile, Res};

use luma_module_sdk::engine::services::jobs::now_ms;
use luma_module_sdk::engine::state::SharedState;
use luma_torrent::db::IndexerRow;

const GB: u64 = 1_073_741_824;

/// The acquisition background jobs this module contributes to the app's job
/// registry (search / import / match). The binary passes this to
/// `AppState::new` so the core registers them without naming the module.
pub const JOBS: &[luma_module_sdk::engine::services::jobs::Builtin] =
    &[jobs::import::SPEC, jobs::search::SPEC, jobs::match_::SPEC];

/// This module's id, shared with `module.json` and the frontend package. The one
/// place callers (route gate, job guards) name the module.
pub const MODULE_ID: &str = "dev.luma.acquisition";

/// This module's registry entry (manifest + packaged icon embedded at compile time).
pub const MODULE: luma_module_sdk::EmbeddedModule = luma_module_sdk::EmbeddedModule::new(
    include_str!("../../module.json"),
    include_bytes!("../../icon.svg"),
);

/// Resolve the Downloads module's download manager from the host service
/// registry. Acquisition reaches the engine only by type through `HostCtx`, so
/// it never holds a concrete `AppState` field for it.
pub(crate) fn downloads(state: &SharedState) -> std::sync::Arc<luma_torrent::DownloadManager> {
    luma_module_sdk::host::service::<luma_torrent::DownloadManager>(&**state)
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
fn to_indexer_query(q: &luma_module_sdk::ports::Query) -> luma_indexer::Query {
    match q {
        luma_module_sdk::ports::Query::Movie { tmdb_id, imdb_id, title, year } => luma_indexer::Query::Movie {
            tmdb_id: *tmdb_id,
            imdb_id: imdb_id.clone(),
            title: title.clone(),
            year: *year,
        },
        luma_module_sdk::ports::Query::Episode { tmdb_id, title, season, episode } => {
            luma_indexer::Query::Episode {
                tmdb_id: *tmdb_id,
                title: title.clone(),
                season: *season,
                episode: *episode,
            }
        }
        luma_module_sdk::ports::Query::Season { tmdb_id, title, season } => luma_indexer::Query::Season {
            tmdb_id: *tmdb_id,
            title: title.clone(),
            season: *season,
        },
    }
}

/// Normalize a native-engine release into the Torznab release shape the scoring
/// pipeline already consumes.
fn release_from_indexer(r: luma_indexer::Release) -> luma_module_sdk::ports::Release {
    luma_module_sdk::ports::Release {
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
    query: &luma_module_sdk::ports::Query,
) -> anyhow::Result<Vec<luma_module_sdk::ports::Release>> {
    if row.kind == luma_indexer::admin::KIND_BUILTIN {
        let session = luma_indexer::admin::builtin_session(state, row)?;
        let outcome = session.search(&to_indexer_query(query), &row.categories);
        // Healthy if we got releases (a partial per-path error alongside real
        // results must not flag the indexer as broken) or the sweep was clean.
        let note_ok = !outcome.releases.is_empty() || outcome.errors.is_empty();
        let _ = luma_torrent::db::note_indexer_result(
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
        let port = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::TorznabPort>(state)
            .ok_or_else(|| anyhow::anyhow!("torznab search engine unavailable"))?;
        port.search(&luma_indexer::admin::endpoint_of(row), query, &caps)
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

/// The Acquisition module's backend behavior: it serves the search / analyze /
/// add admin routes (behind its enabled-gate) and contributes the search /
/// import / match jobs. Disabling it 404s those routes and no-ops the jobs, so
/// the whole search / grab / auto feature is gated on this module. It reaches the
/// [`luma_torrent::DownloadManager`] through the host service registry.
///
/// Like the download / vpn / indexer modules it orchestrates the app's concrete
/// `AppState` (settings / config / DB), so it is a `ServerModule<SharedState>`.
pub struct AcquisitionModule;

#[luma_module_sdk::host::async_trait]
impl luma_module_sdk::host::ServerModule<luma_module_sdk::engine::state::SharedState> for AcquisitionModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn admin_routes(
        &self,
        _host: &luma_module_sdk::engine::state::SharedState,
    ) -> Option<axum::Router<luma_module_sdk::engine::state::SharedState>> {
        Some(routes::routes())
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module() -> Box<dyn luma_module_sdk::host::ServerModule<luma_module_sdk::engine::state::SharedState>> {
    Box::new(AcquisitionModule)
}
