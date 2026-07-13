//! The search pipeline shared by interactive search and the automatic
//! wanted-list job: wanted rows -> Torznab queries -> decision-engine scoring
//! -> ordered candidate views.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use luma_module_sdk::scene::{Candidate, Target};
use luma_module_sdk::ports::{Query, Release};

use crate::dtos::{
    InteractiveSearchView, ManualReleaseView, ManualSearchView, ScoreLineView, ScoredReleaseView,
};
use luma_module_sdk::engine::model::RequestKind;
use luma_module_sdk::engine::state::SharedState;
use luma_module_sdk::ports::IndexerRow;
use luma_torrent::db::{self, WantedRow};

/// A release remembered from the last interactive search of a request, so a
/// manual grab can hand its magnet/.torrent link to the download manager
/// without a re-sweep (and without putting grab URLs on the wire).
#[derive(Clone)]
pub struct CachedRelease {
    pub view: ScoredReleaseView,
    pub magnet_or_url: String,
    pub tmdb_id: u64,
}

/// Last interactive-search results per request id (bounded: one entry per
/// request, cleared on grab).
static SEARCH_CACHE: Mutex<Option<HashMap<String, Vec<CachedRelease>>>> = Mutex::new(None);

pub fn cached_release(request_id: &str, guid: &str, indexer_id: &str) -> Option<CachedRelease> {
    SEARCH_CACHE
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(request_id))
        .and_then(|list| {
            list.iter().find(|c| c.view.guid == guid && c.view.indexer_id == indexer_id).cloned()
        })
}

fn cache_results(request_id: &str, releases: Vec<CachedRelease>) {
    let mut guard = SEARCH_CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    // Keep the cache small: it only serves "search then grab" round-trips.
    if map.len() > 24 {
        map.clear();
    }
    map.insert(request_id.to_string(), releases);
}

/// Build a grab spec from a scored release the search chose, for a specific
/// request/title. Formerly `GrabSpec::from_release` in `luma-torrent`; it moved
/// here with `ScoredReleaseView` so the Downloads module names no acquisition type.
pub fn grab_spec_from_release(
    release: &ScoredReleaseView,
    magnet_or_url: &str,
    tmdb_id: u64,
    title: Option<String>,
    year: Option<u32>,
    request_id: Option<String>,
    wanted_ids: Vec<String>,
) -> luma_torrent::GrabSpec {
    luma_torrent::GrabSpec {
        magnet_or_url: magnet_or_url.to_string(),
        kind: release.target.clone(),
        tmdb_id,
        title,
        year,
        season: release.season,
        episodes: release.episodes.clone(),
        release_title: release.title.clone(),
        indexer_id: Some(release.indexer_id.clone()),
        size_bytes: release.size_bytes,
        score: release.score,
        score_breakdown: serde_json::to_string(&release.breakdown).ok(),
        request_id,
        wanted_ids,
        only_files: None,
        details_url: release.details_url.clone(),
    }
}

/// One thing worth searching for: the Torznab query + the decision target +
/// what a grab of it would cover.
pub struct SearchTarget {
    pub query: Query,
    pub target: Target,
    /// `movie` | `episode` | `season`.
    pub kind: &'static str,
    pub season: Option<u32>,
    pub episodes: Option<Vec<u32>>,
}

/// Derive the searchable targets from a request's wanted ledger. Shows group
/// into one season-pack target per season with open episodes; when only a few
/// episodes are missing overall, per-episode targets are added too (a pack is
/// overkill for a 1-2 episode gap and often does not exist for airing seasons).
pub fn targets_for_wanted(kind: RequestKind, wanted: &[WantedRow]) -> Vec<SearchTarget> {
    let open: Vec<&WantedRow> = wanted.iter().filter(|w| w.status == "wanted").collect();
    let mut out: Vec<SearchTarget> = Vec::new();
    match kind {
        RequestKind::Movie => {
            if let Some(w) = open.first() {
                out.push(SearchTarget {
                    query: Query::Movie {
                        tmdb_id: Some(w.tmdb_id),
                        imdb_id: w.imdb_id.clone(),
                        title: w.title.clone(),
                        year: w.year,
                    },
                    target: Target::Movie { year: w.year },
                    kind: "movie",
                    season: None,
                    episodes: None,
                });
            }
        }
        RequestKind::Show => {
            let mut seasons: Vec<u32> = open.iter().filter_map(|w| w.season).collect();
            seasons.sort_unstable();
            seasons.dedup();
            for season in seasons {
                let eps: Vec<u32> = open
                    .iter()
                    .filter(|w| w.season == Some(season))
                    .filter_map(|w| w.episode)
                    .collect();
                let sample = open.iter().find(|w| w.season == Some(season)).expect("season has rows");
                out.push(SearchTarget {
                    query: Query::Season {
                        tmdb_id: Some(sample.tmdb_id),
                        title: sample.title.clone(),
                        season,
                    },
                    target: Target::Season { season, episodes: eps.len() as u32 },
                    kind: "season",
                    season: Some(season),
                    episodes: Some(eps),
                });
            }
            if open.len() <= 3 {
                for w in &open {
                    let (Some(season), Some(episode)) = (w.season, w.episode) else { continue };
                    out.push(SearchTarget {
                        query: Query::Episode {
                            tmdb_id: Some(w.tmdb_id),
                            title: w.title.clone(),
                            season,
                            episode,
                        },
                        target: Target::Episode { season, episode },
                        kind: "episode",
                        season: Some(season),
                        episodes: Some(vec![episode]),
                    });
                }
            }
        }
    }
    out
}

/// Score one Torznab release against a target, into the wire view.
pub fn score_release(
    release: &Release,
    indexer: &IndexerRow,
    st: &SearchTarget,
    profile: &luma_module_sdk::scene::Profile,
) -> ScoredReleaseView {
    let parsed = luma_module_sdk::scene::parse_release_name(&release.title);
    let candidate = Candidate {
        size_bytes: release.size_bytes,
        seeders: release.seeders,
        indexer_priority: indexer.priority,
    };
    let (score, breakdown, rejected) =
        match luma_module_sdk::scene::score(&parsed, &candidate, &st.target, profile, &release.title) {
            Ok(s) => (
                Some(s.score),
                s.breakdown
                    .into_iter()
                    .map(|l| ScoreLineView { rule: l.rule, delta: l.delta, note: l.note })
                    .collect(),
                None,
            ),
            Err(r) => (None, Vec::new(), Some(format!("{}: {}", r.rule, r.note))),
        };
    ScoredReleaseView {
        title: release.title.clone(),
        guid: release.guid.clone(),
        indexer_id: indexer.id.clone(),
        indexer_name: indexer.name.clone(),
        size_bytes: release.size_bytes,
        seeders: release.seeders,
        leechers: release.leechers,
        published_at: release.published_at.clone(),
        target: st.kind.to_string(),
        season: st.season,
        episodes: st.episodes.clone(),
        score,
        breakdown,
        rejected,
        // A details URL is grabbable too: built-in indexers resolve the actual
        // magnet/.torrent from the details page (the definition's `download`
        // block) at grab time. Torznab releases always carry magnet/link, so
        // this only widens grabbability for the built-in download-block case.
        grabbable: release.magnet.is_some()
            || release.link.is_some()
            || release.details_url.is_some(),
        details_url: release.details_url.clone(),
    }
}

/// Interactive search for one request: sweep every enabled indexer over the
/// request's targets and return everything, scored or rejected-with-reason,
/// accepted-best first. Synchronous and network-heavy: call from a blocking
/// context only.
pub fn interactive_search(state: &SharedState, request_id: &str) -> Result<InteractiveSearchView> {
    let conn = state.db.get()?;
    let req = db::get_request(&conn, request_id)?.ok_or_else(|| anyhow!("request not found"))?;
    let mut wanted = db::wanted_for_request(&conn, request_id)?;
    let indexers = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerDbPort>(state).ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?.enabled_indexers(state)?;
    drop(conn);
    if indexers.is_empty() {
        return Err(anyhow!("no enabled indexer; add one under Admin > Indexeurs"));
    }
    // A pending request has no ledger yet: search as if it were approved, so a
    // moderator can look before green-lighting.
    if wanted.is_empty() {
        luma_module_sdk::engine::services::requests::preview_wanted(state, &req, &mut wanted)?;
    }
    // An interactive search is an explicit admin action: search the request's
    // FULL content regardless of ledger status, so a request that's already
    // available / grabbed / partially available can still be searched and
    // (re)grabbed. targets_for_wanted only considers `wanted` rows, so force
    // every row to `wanted` for target generation (this clone is not persisted).
    let mut search_wanted = wanted.clone();
    for w in &mut search_wanted {
        w.status = "wanted".into();
    }

    let profile = crate::profile_from_settings(state);
    let targets = targets_for_wanted(req.kind, &search_wanted);
    if targets.is_empty() {
        return Ok(InteractiveSearchView { releases: Vec::new(), indexer_errors: Vec::new() });
    }

    let mut cached: Vec<CachedRelease> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for indexer in &indexers {
        for st in &targets {
            match crate::search_indexer(state, indexer, &st.query) {
                Ok(found) => {
                    for release in found {
                        if seen.insert((indexer.id.clone(), release.guid.clone())) {
                            let view = score_release(&release, indexer, st, &profile);
                            let magnet_or_url = release
                                .magnet
                                .clone()
                                .or_else(|| release.link.clone())
                                .unwrap_or_default();
                            cached.push(CachedRelease { view, magnet_or_url, tmdb_id: req.tmdb_id });
                        }
                    }
                }
                Err(e) => errors.push(format!("{}: {e:#}", indexer.name)),
            }
        }
    }

    // Accepted first (best score), then rejects; capped so a broad tracker
    // sweep stays a readable list.
    cached.sort_by_key(|c| (c.view.score.is_none(), -(c.view.score.unwrap_or(0))));
    cached.truncate(150);
    errors.sort();
    errors.dedup();
    let releases = cached.iter().map(|c| c.view.clone()).collect();
    cache_results(&req.id, cached);
    Ok(InteractiveSearchView { releases, indexer_errors: errors })
}

/// Grab one release from the last interactive search of this request. Returns
/// the queued download row; the caller kicks off the (slow) engine add in the
/// background via `DownloadManager::activate`.
pub fn grab_cached(
    state: &SharedState,
    request_id: &str,
    guid: &str,
    indexer_id: &str,
) -> Result<db::DownloadRow> {
    // The search cache is in-memory, so a server restart (common in dev with
    // cargo-watch) or a direct grab with no prior search would miss it. On a
    // miss, re-run the interactive search to repopulate, then look up again.
    let cached = match cached_release(request_id, guid, indexer_id) {
        Some(c) => c,
        None => {
            interactive_search(state, request_id)?;
            cached_release(request_id, guid, indexer_id).ok_or_else(|| {
                anyhow!("release not found on the indexer anymore; run the search again")
            })?
        }
    };
    // Resolve the grabbable target. A built-in indexer may need a details-page
    // fetch (the definition's `download` block) to turn a search row into a
    // magnet / .torrent link.
    let magnet_or_url = {
        let row = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerDbPort>(state)
            .ok_or_else(|| anyhow!("indexer module unavailable"))?
            .get_indexer(state, &cached.view.indexer_id)?;
        match row {
            Some(r) if r.kind == luma_module_sdk::ports::KIND_BUILTIN => crate::resolve_builtin_download(
                state,
                &r,
                &cached.view.title,
                cached.view.details_url.as_deref(),
                &cached.magnet_or_url,
            )?,
            _ => cached.magnet_or_url.clone(),
        }
    };
    if magnet_or_url.is_empty() {
        return Err(anyhow!("release has no magnet or download link"));
    }
    // Grabbing a release implies approval. A still-pending request has no wanted
    // ledger, so the grab would create a download but leave the request stuck in
    // "pending" forever. Approve it first (materializes the ledger + moves it out
    // of pending), so the grab can flip the right rows to "grabbed".
    let conn = state.db.get()?;
    let req = db::get_request(&conn, request_id)?.ok_or_else(|| anyhow!("request not found"))?;
    drop(conn);
    let needs_approval = matches!(
        req.status,
        luma_module_sdk::engine::model::RequestStatus::Pending | luma_module_sdk::engine::model::RequestStatus::Failed
    );
    if needs_approval {
        luma_module_sdk::engine::services::requests::approve_request(state, request_id, None)?;
    }
    let conn = state.db.get()?;
    let req = db::get_request(&conn, request_id)?.ok_or_else(|| anyhow!("request not found"))?;
    let wanted = db::wanted_for_request(&conn, request_id)?;
    drop(conn);
    let wanted_ids = wanted_ids_for(&wanted, &cached.view);
    let spec = grab_spec_from_release(
        &cached.view,
        &magnet_or_url,
        cached.tmdb_id,
        Some(req.title),
        req.year,
        Some(request_id.to_string()),
        wanted_ids,
    );
    crate::downloads(state).grab(state, spec)
}

/// Free-text manual search across every enabled indexer: parse each result for
/// quality/episode hints and sort best-first, but do NOT accept/reject (there
/// is no specific target). The admin picks and grabs via the add endpoint.
pub fn manual_search(state: &SharedState, query: &str) -> Result<ManualSearchView> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(ManualSearchView { releases: Vec::new(), indexer_errors: Vec::new() });
    }
    let conn = state.db.get()?;
    let indexers = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerDbPort>(state).ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?.enabled_indexers(state)?;
    drop(conn);
    if indexers.is_empty() {
        return Err(anyhow!("no enabled indexer; add one under Admin > Indexeurs"));
    }

    let torznab_query = Query::Movie {
        tmdb_id: None,
        imdb_id: None,
        title: q.to_string(),
        year: None,
    };
    let mut releases: Vec<ManualReleaseView> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for indexer in &indexers {
        // Free text: the Movie query's last attempt is the `q` fallback (torznab)
        // / the keywords search (built-in).
        match crate::search_indexer(state, indexer, &torznab_query) {
            Ok(found) => {
                for r in found {
                    if !seen.insert((indexer.id.clone(), r.guid.clone())) {
                        continue;
                    }
                    let p = luma_module_sdk::scene::parse_release_name(&r.title);
                    releases.push(ManualReleaseView {
                        title: r.title.clone(),
                        guid: r.guid.clone(),
                        indexer_name: indexer.name.clone(),
                        download_url: r.magnet.clone().or_else(|| r.link.clone()),
                        size_bytes: r.size_bytes,
                        seeders: r.seeders,
                        leechers: r.leechers,
                        published_at: r.published_at.clone(),
                        resolution: p.resolution.map(|res| format!("{res:?}").replace('R', "")),
                        codec: p.codec.map(|c| format!("{c:?}")),
                        source: p.source.map(|s| format!("{s:?}")),
                        parsed_title: p.title,
                        year: p.year,
                        season: p.season,
                        episode: p.episode,
                        full_season: p.full_season,
                        details_url: r.details_url.clone(),
                    });
                }
            }
            Err(e) => errors.push(format!("{}: {e:#}", indexer.name)),
        }
    }
    // Best-first: more seeders, then bigger (rough quality proxy).
    releases.sort_by(|a, b| {
        b.seeders.unwrap_or(0).cmp(&a.seeders.unwrap_or(0)).then(b.size_bytes.unwrap_or(0).cmp(&a.size_bytes.unwrap_or(0)))
    });
    releases.truncate(150);
    errors.sort();
    errors.dedup();
    Ok(ManualSearchView { releases, indexer_errors: errors })
}

/// The wanted rows a grab of this release covers (flip to `grabbed`).
pub fn wanted_ids_for(wanted: &[WantedRow], view: &ScoredReleaseView) -> Vec<String> {
    wanted_ids_by(wanted, &view.target, view.season, view.episodes.as_deref())
}

/// Coverage rule keyed on the target shape alone (target / season / episodes),
/// so callers holding a `SearchTarget` don't need to fabricate a full
/// `ScoredReleaseView` just to reach it.
pub fn wanted_ids_by(
    wanted: &[WantedRow],
    target: &str,
    season: Option<u32>,
    episodes: Option<&[u32]>,
) -> Vec<String> {
    match target {
        "movie" => wanted.iter().filter(|w| w.kind == "movie").map(|w| w.id.clone()).collect(),
        "season" => wanted.iter().filter(|w| w.season == season).map(|w| w.id.clone()).collect(),
        _ => wanted
            .iter()
            .filter(|w| {
                w.season == season
                    && w.episode.is_some_and(|e| episodes.is_some_and(|list| list.contains(&e)))
            })
            .map(|w| w.id.clone())
            .collect(),
    }
}
