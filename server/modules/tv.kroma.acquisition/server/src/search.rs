//! The search pipeline shared by interactive search and the automatic
//! wanted-list job: wanted rows -> Torznab queries -> decision-engine scoring
//! -> ordered candidate views.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use kroma_module_sdk::scene::{Candidate, Target};
use kroma_module_sdk::ports::{Query, Release};

use crate::dtos::{
    InteractiveSearchView, ManualReleaseView, ManualSearchView, ScoreLineView, ScoredReleaseView,
};
use kroma_module_sdk::engine::model::RequestKind;
use kroma_module_sdk::engine::services::requests::today_ymd;
use kroma_module_sdk::ports::IndexerRow;
use kroma_module_sdk::db::{self, WantedRow};

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
/// request/title. Formerly `GrabSpec::from_release` in `kroma-torrent`; it moved
/// here with `ScoredReleaseView` so the Downloads module names no acquisition type.
pub fn grab_spec_from_release(
    release: &ScoredReleaseView,
    magnet_or_url: &str,
    tmdb_id: u64,
    title: Option<String>,
    year: Option<u32>,
    request_id: Option<String>,
    wanted_ids: Vec<String>,
) -> kroma_module_sdk::ports::GrabSpec {
    kroma_module_sdk::ports::GrabSpec {
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

/// Build the search targets for a request. `today` (YYYY-MM-DD) decides, per
/// season, whether it is COMPLETE (every episode has aired) or AIRING (an episode
/// is still to come). An AIRING season searches each AIRED-but-open episode on
/// its own (SxxExx), since a full season pack does not exist mid-airing so a pack
/// query is wasteful. A COMPLETE season searches the season pack FIRST (one
/// efficient release), then the aired episodes individually as a fallback the
/// auto pass skips once the pack grab covers them (`covered` set), so a season
/// with no pack still fills in per episode instead of stalling. Unaired episodes
/// (air_date in the future) are never searched.
pub fn targets_for_wanted(kind: RequestKind, wanted: &[WantedRow], today: &str) -> Vec<SearchTarget> {
    let open: Vec<&WantedRow> = wanted.iter().filter(|w| w.status == "wanted").collect();
    // A row with no air date is treated as aired (older ledgers, specials).
    let aired = |w: &WantedRow| w.air_date.as_deref().is_none_or(|d| d <= today);
    let mut out: Vec<SearchTarget> = Vec::new();
    match kind {
        RequestKind::Movie => {
            if let Some(w) = open.iter().copied().find(|w| aired(w)) {
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
                let rows: Vec<&WantedRow> =
                    open.iter().copied().filter(|w| w.season == Some(season)).collect();
                let aired_eps: Vec<u32> =
                    rows.iter().copied().filter(|w| aired(w)).filter_map(|w| w.episode).collect();
                if aired_eps.is_empty() {
                    continue; // nothing has aired for this season yet
                }
                let has_future =
                    rows.iter().any(|w| w.air_date.as_deref().is_some_and(|d| d > today));
                let sample = rows[0];
                let episode_target = |episode: u32| SearchTarget {
                    query: Query::Episode {
                        tmdb_id: Some(sample.tmdb_id),
                        title: sample.title.clone(),
                        season,
                        episode,
                    },
                    target: Target::Episode { season, episode },
                    kind: "episode",
                    season: Some(season),
                    episodes: Some(vec![episode]),
                };
                if !has_future {
                    // Complete season: the pack (covers everything) comes first.
                    out.push(SearchTarget {
                        query: Query::Season {
                            tmdb_id: Some(sample.tmdb_id),
                            title: sample.title.clone(),
                            season,
                        },
                        target: Target::Season { season, episodes: aired_eps.len() as u32 },
                        kind: "season",
                        season: Some(season),
                        episodes: Some(aired_eps.clone()),
                    });
                }
                // Per-episode targets: the only search for an airing season, or the
                // fallback after a complete season's pack (skipped once covered).
                for ep in aired_eps {
                    out.push(episode_target(ep));
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
    profile: &kroma_module_sdk::scene::Profile,
) -> ScoredReleaseView {
    let parsed = kroma_module_sdk::scene::parse_release_name(&release.title);
    let candidate = Candidate {
        size_bytes: release.size_bytes,
        seeders: release.seeders,
        indexer_priority: indexer.priority,
    };
    let (score, breakdown, rejected) =
        match kroma_module_sdk::scene::score(&parsed, &candidate, &st.target, profile, &release.title) {
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
pub fn interactive_search<S: kroma_module_sdk::host::HostCtx>(state: &S, request_id: &str) -> Result<InteractiveSearchView> {
    let conn = state.db().get()?;
    let req = db::get_request(&conn, request_id)?.ok_or_else(|| anyhow!("request not found"))?;
    let mut wanted = db::wanted_for_request(&conn, request_id)?;
    let indexers = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::IndexerDbPort>(state).ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?.enabled_indexers(state)?;
    drop(conn);
    if indexers.is_empty() {
        return Err(anyhow!("no enabled indexer; add one under Admin > Indexeurs"));
    }
    // A pending request has no ledger yet: search as if it were approved, so a
    // moderator can look before green-lighting.
    if wanted.is_empty() {
        kroma_module_sdk::engine::services::requests::preview_wanted(state, &req, &mut wanted)?;
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
    let targets = targets_for_wanted(req.kind, &search_wanted, &today_ymd());
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
pub fn grab_cached<S: kroma_module_sdk::host::HostCtx>(state: &S,
    request_id: &str,
    guid: &str,
    indexer_id: &str,
) -> Result<kroma_module_sdk::ports::DownloadRow> {
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
        let row = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::IndexerDbPort>(state)
            .ok_or_else(|| anyhow!("indexer module unavailable"))?
            .get_indexer(state, &cached.view.indexer_id)?;
        match row {
            Some(r) if r.kind == kroma_module_sdk::ports::KIND_BUILTIN => crate::resolve_builtin_download(
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
    let conn = state.db().get()?;
    let req = db::get_request(&conn, request_id)?.ok_or_else(|| anyhow!("request not found"))?;
    drop(conn);
    let needs_approval = matches!(
        req.status,
        kroma_module_sdk::engine::model::RequestStatus::Pending | kroma_module_sdk::engine::model::RequestStatus::Failed
    );
    if needs_approval {
        kroma_module_sdk::engine::services::requests::approve_request(state, request_id, None)?;
    }
    let conn = state.db().get()?;
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
pub fn manual_search<S: kroma_module_sdk::host::HostCtx>(state: &S, query: &str) -> Result<ManualSearchView> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(ManualSearchView { releases: Vec::new(), indexer_errors: Vec::new() });
    }
    let conn = state.db().get()?;
    let indexers = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::IndexerDbPort>(state).ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?.enabled_indexers(state)?;
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
                    let p = kroma_module_sdk::scene::parse_release_name(&r.title);
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

#[cfg(test)]
mod target_tests {
    use super::*;

    fn ep(season: u32, episode: u32, air_date: Option<&str>) -> WantedRow {
        WantedRow {
            id: format!("s{season}e{episode}"),
            request_id: "r1".into(),
            kind: "episode".into(),
            tmdb_id: 42,
            imdb_id: None,
            title: "Show".into(),
            year: Some(2026),
            season: Some(season),
            episode: Some(episode),
            air_date: air_date.map(str::to_string),
            status: "wanted".into(),
            last_search_at: None,
        }
    }

    #[test]
    fn airing_season_searches_aired_episodes_per_episode_only() {
        // Ep1-2 aired, ep3 airs in the future: airing season -> per-episode for
        // the two aired ones, NO season pack, and never the unaired episode.
        let rows = vec![
            ep(1, 1, Some("2026-07-01")),
            ep(1, 2, Some("2026-07-08")),
            ep(1, 3, Some("2026-07-22")),
        ];
        let t = targets_for_wanted(RequestKind::Show, &rows, "2026-07-16");
        assert_eq!(t.len(), 2, "two aired episodes, no pack, no future ep");
        assert!(t.iter().all(|x| x.kind == "episode"));
        assert_eq!(t.iter().filter_map(|x| x.episodes.as_ref()?.first()).copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn complete_season_searches_pack_first_then_episode_fallback() {
        // All aired: complete season -> pack first, then per-episode fallback.
        let rows = vec![ep(2, 1, Some("2025-01-01")), ep(2, 2, Some("2025-01-08"))];
        let t = targets_for_wanted(RequestKind::Show, &rows, "2026-07-16");
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].kind, "season");
        assert!(t[1..].iter().all(|x| x.kind == "episode"));
    }

    #[test]
    fn no_air_date_is_treated_as_aired_complete() {
        let rows = vec![ep(1, 1, None), ep(1, 2, None)];
        let t = targets_for_wanted(RequestKind::Show, &rows, "2026-07-16");
        assert_eq!(t[0].kind, "season"); // no future -> complete -> pack + fallback
        assert_eq!(t.len(), 3);
    }
}
