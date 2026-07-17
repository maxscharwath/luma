//! Request lifecycle orchestration: create (with duplicate-merge and
//! auto-approve), approve (materializes the episode-level wanted ledger from
//! TMDB season data), deny, and the availability matcher that flips requests
//! once their titles appear in the local catalog (whether imported by the
//! acquisition stack or added by hand).
//!
//! Everything here is synchronous and does network/DB work: call it from a
//! blocking context (`api::util::blocking`, a job thread), never inline in an
//! async handler.

use anyhow::{anyhow, bail, Result};

use serde_json::json;

use kroma_module_host::{Event, HostCtx};

use crate::db;
use crate::infra::metadata::discover;
use crate::model::{
    CreateRequestBody, EpisodeRef, MediaRequest, Permission, RequestKind, RequestStatus, User,
};
use crate::services::jobs::now_ms;

// Request orchestration is generic over the host state `S: HostCtx` (settings /
// TMDB config / event bus / job triggers / DB reached through the seam), so it
// runs both in-core (`S = SharedState`, the request API) and out-of-process
// (`S = RemoteHost`, the acquisition `.kmod` calling the same fulfillment logic).

fn tmdb_key<S: HostCtx>(state: &S) -> Result<String> {
    state.tmdb_api_key().ok_or_else(|| anyhow!("TMDB is not configured"))
}

fn language<S: HostCtx>(state: &S) -> String {
    state.metadata_language()
}

fn publish<S: HostCtx>(state: &S, req_id: &str, status: RequestStatus) {
    // The wire shape matches the former `ServerEvent::RequestUpdated`
    // (`{ "type": "request.updated", id, status }`): HostCtx::publish merges the
    // topic under the `type` key, so clients see identical bytes.
    state.publish(Event::new(
        "request.updated",
        json!({ "id": req_id, "status": status.as_str() }),
    ));
}

/// Create (or duplicate-merge) a request. Auto-approves when the requester
/// holds `requests.auto`.
pub fn create_request<S: HostCtx>(state: &S, user: &User, body: &CreateRequestBody) -> Result<MediaRequest> {
    let key = tmdb_key(state)?;
    let lang = language(state);
    let detail = discover::detail(&key, &lang, body.kind, body.tmdb_id)
        .map_err(|()| anyhow!("TMDB lookup failed"))?
        .ok_or_else(|| anyhow!("title not found on TMDB"))?;

    // Normalize the ask: None/empty = whole show; movies carry None for both.
    let asked_seasons: Option<Vec<u32>> = match body.kind {
        RequestKind::Movie => None,
        RequestKind::Show => body.seasons.clone().filter(|s| !s.is_empty()).map(|mut s| {
            s.sort_unstable();
            s.dedup();
            s
        }),
    };
    let asked_episodes: Option<Vec<EpisodeRef>> = match body.kind {
        RequestKind::Movie => None,
        RequestKind::Show => normalize_episodes(body.episodes.clone()),
    };

    // Duplicate-merge: a second ask for an open title folds in (a show ask can
    // widen the season subset and/or the individual-episode subset;
    // re-materialized below if already approved).
    let conn = state.db().get()?;
    if let Some(existing) = db::find_open_request(&conn, body.kind, body.tmdb_id)? {
        drop(conn);
        if body.kind == RequestKind::Show {
            let (merged_seasons, merged_episodes) = merge_target(
                existing.seasons.clone(),
                existing.episodes.clone(),
                asked_seasons,
                asked_episodes,
            );
            let seasons_changed = merged_seasons != existing.seasons;
            let episodes_changed = merged_episodes != existing.episodes;
            if seasons_changed {
                db::set_request_seasons(state.db(), &existing.id, merged_seasons.as_deref(), now_ms())?;
            }
            if episodes_changed {
                db::set_request_episodes(state.db(), &existing.id, merged_episodes.as_deref(), now_ms())?;
            }
            if seasons_changed || episodes_changed {
                if matches!(existing.status, RequestStatus::Approved | RequestStatus::PartiallyAvailable) {
                    // Already green-lit: extend the wanted ledger to the new target.
                    materialize_wanted(state, &existing.id)?;
                }
                publish(state, &existing.id, existing.status);
            }
        }
        let conn = state.db().get()?;
        return db::get_request(&conn, &existing.id)?
            .ok_or_else(|| anyhow!("request vanished during merge"));
    }
    drop(conn);

    let id = crate::services::scan::short_hash(&format!(
        "request|{}|{}|{}",
        body.kind.as_str(),
        body.tmdb_id,
        crate::services::auth::random_token()
    ));
    let new = db::NewRequest {
        id: id.clone(),
        kind: body.kind,
        tmdb_id: body.tmdb_id,
        title: detail.title.clone(),
        year: detail.year,
        poster_url: detail.poster_url.clone(),
        seasons: asked_seasons,
        episodes: asked_episodes,
        status: RequestStatus::Pending,
        requested_by: Some(user.id.clone()),
    };
    db::insert_request(state.db(), &new, now_ms())?;
    publish(state, &id, RequestStatus::Pending);

    if user.can(Permission::RequestsAuto) {
        approve_request(state, &id, Some(&user.id))?;
    } else {
        // The title may already sit in the library (e.g. season subset overlap):
        // let the matcher flip it right away rather than waiting for the cron.
        let _ = match_one(state, &id)?;
    }

    let conn = state.db().get()?;
    db::get_request(&conn, &id)?.ok_or_else(|| anyhow!("request vanished after insert"))
}

/// Approve: materialize the wanted ledger, mark approved, match availability,
/// and kick the automatic search.
pub fn approve_request<S: HostCtx>(state: &S, id: &str, reviewer: Option<&str>) -> Result<MediaRequest> {
    let conn = state.db().get()?;
    let req = db::get_request(&conn, id)?.ok_or_else(|| anyhow!("request not found"))?;
    drop(conn);
    if matches!(req.status, RequestStatus::Denied) {
        bail!("request was denied; delete it and ask again");
    }
    db::set_request_status(state.db(), id, RequestStatus::Approved, reviewer, None, now_ms())?;
    materialize_wanted(state, id)?;
    let status = match_one(state, id)?.unwrap_or(RequestStatus::Approved);
    publish(state, id, status);
    // Fire the wanted-list search right away (registered with the downloads
    // milestone; until then the trigger is a no-op on the unknown key).
    state.trigger_job("acquisition.search", "request-approved");
    let conn = state.db().get()?;
    db::get_request(&conn, id)?.ok_or_else(|| anyhow!("request vanished after approve"))
}

pub fn deny_request<S: HostCtx>(state: &S, id: &str, reviewer: &str, note: Option<&str>) -> Result<MediaRequest> {
    let changed =
        db::set_request_status(state.db(), id, RequestStatus::Denied, Some(reviewer), note, now_ms())?;
    if !changed {
        bail!("request not found");
    }
    publish(state, id, RequestStatus::Denied);
    let conn = state.db().get()?;
    db::get_request(&conn, id)?.ok_or_else(|| anyhow!("request vanished after deny"))
}

/// Canonicalize an individual-episode ask: empty -> `None`, else sorted + deduped.
fn normalize_episodes(episodes: Option<Vec<EpisodeRef>>) -> Option<Vec<EpisodeRef>> {
    let mut list = episodes.filter(|e| !e.is_empty())?;
    list.sort_unstable_by_key(|e| (e.season, e.episode));
    list.dedup();
    Some(list)
}

/// A Show request targets the WHOLE show only when it names neither full seasons
/// nor individual episodes; that is the maximal target.
fn is_whole_show(seasons: &Option<Vec<u32>>, episodes: &Option<Vec<EpisodeRef>>) -> bool {
    seasons.is_none() && episodes.is_none()
}

/// Union of two Show targets (existing + a second ask), each expressed as an
/// optional full-season set plus an optional individual-episode set. A whole-show
/// side absorbs everything (stays whole show); otherwise a `None` set means the
/// EMPTY set (not "all", which only whole show denotes), so the two sets union
/// cleanly without a single-episode ask ever narrowing a broader request.
fn merge_target(
    ex_seasons: Option<Vec<u32>>,
    ex_episodes: Option<Vec<EpisodeRef>>,
    add_seasons: Option<Vec<u32>>,
    add_episodes: Option<Vec<EpisodeRef>>,
) -> (Option<Vec<u32>>, Option<Vec<EpisodeRef>>) {
    if is_whole_show(&ex_seasons, &ex_episodes) || is_whole_show(&add_seasons, &add_episodes) {
        return (None, None);
    }
    let mut seasons: Vec<u32> =
        ex_seasons.unwrap_or_default().into_iter().chain(add_seasons.unwrap_or_default()).collect();
    let seasons = if seasons.is_empty() {
        None
    } else {
        seasons.sort_unstable();
        seasons.dedup();
        Some(seasons)
    };
    let episodes: Vec<EpisodeRef> = ex_episodes
        .unwrap_or_default()
        .into_iter()
        .chain(add_episodes.unwrap_or_default())
        .collect();
    (seasons, normalize_episodes(Some(episodes)))
}

/// (Re)build a request's wanted rows from TMDB. Movies: one row. Shows: one
/// row per episode of every requested season, with air dates so unaired
/// episodes wait their turn. Idempotent: replaces the request's ledger.
fn materialize_wanted<S: HostCtx>(state: &S, id: &str) -> Result<()> {
    let conn = state.db().get()?;
    let req = db::get_request(&conn, id)?.ok_or_else(|| anyhow!("request not found"))?;
    drop(conn);
    let rows = build_wanted_rows(state, &req)?;
    db::replace_wanted(state.db(), &req.id, &rows, now_ms())
}

/// Fill `out` with the wanted rows a request WOULD get on approval, without
/// persisting anything (interactive search on a still-pending request).
pub fn preview_wanted<S: HostCtx>(state: &S, req: &MediaRequest, out: &mut Vec<db::WantedRow>) -> Result<()> {
    *out = build_wanted_rows(state, req)?;
    Ok(())
}

fn build_wanted_rows<S: HostCtx>(state: &S, req: &MediaRequest) -> Result<Vec<db::WantedRow>> {
    let key = tmdb_key(state)?;
    let lang = language(state);
    let detail = discover::detail(&key, &lang, req.kind, req.tmdb_id)
        .map_err(|()| anyhow!("TMDB lookup failed"))?
        .ok_or_else(|| anyhow!("title not found on TMDB"))?;
    build_wanted_rows_from(state, req, &detail)
}

/// The wanted rows a request targets, given an already-fetched TMDB detail (the
/// refresh pass fetches the detail once, for the air signals AND the ledger).
/// Still does one TMDB `season_episodes` call per requested season for the
/// per-episode air dates.
fn build_wanted_rows_from<S: HostCtx>(
    state: &S,
    req: &MediaRequest,
    detail: &discover::DiscoverRawDetail,
) -> Result<Vec<db::WantedRow>> {
    let key = tmdb_key(state)?;
    let lang = language(state);

    let mut rows: Vec<db::WantedRow> = Vec::new();
    let mint = |salt: &str| {
        crate::services::scan::short_hash(&format!("wanted|{}|{}|{salt}", req.id, req.tmdb_id))
    };
    match req.kind {
        RequestKind::Movie => rows.push(db::WantedRow {
            id: mint("movie"),
            request_id: req.id.clone(),
            kind: "movie".into(),
            tmdb_id: req.tmdb_id,
            imdb_id: detail.imdb_id.clone(),
            title: req.title.clone(),
            year: req.year,
            season: None,
            episode: None,
            // The soonest availability date (digital > theatrical > release) gates
            // an unreleased movie out of search until it is out, then the search
            // pass auto-grabs it (wanted_searchable: air_date NULL or <= today).
            // A movie already out / with no TMDB date has a past / NULL date and
            // stays immediately searchable.
            air_date: detail.available_date.clone(),
            status: "wanted".into(),
            last_search_at: None,
        }),
        RequestKind::Show => {
            use std::collections::{BTreeSet, HashSet};
            // Full seasons to pull whole: the explicit `seasons` ask, or EVERY
            // season only when neither seasons nor episodes were specified.
            let full_seasons: HashSet<u32> = match (&req.seasons, &req.episodes) {
                (Some(list), _) => list.iter().copied().collect(),
                (None, None) => detail.seasons.iter().map(|s| s.season).collect(),
                (None, Some(_)) => HashSet::new(),
            };
            // Individual (season, episode) asks unioned on top of the full seasons.
            let individual: HashSet<(u32, u32)> = req
                .episodes
                .as_ref()
                .map(|eps| eps.iter().map(|e| (e.season, e.episode)).collect())
                .unwrap_or_default();
            // One TMDB call per distinct season across either source (sorted for a
            // stable order).
            let needed: BTreeSet<u32> =
                full_seasons.iter().copied().chain(individual.iter().map(|(s, _)| *s)).collect();
            for season in needed {
                let want_whole = full_seasons.contains(&season);
                let data = crate::infra::metadata::season_episodes(&key, &lang, req.tmdb_id, season);
                for ep in data.episodes {
                    // Include if the whole season is wanted OR this exact episode is;
                    // iterating distinct seasons once yields one row per episode.
                    if !want_whole && !individual.contains(&(season, ep.episode)) {
                        continue;
                    }
                    rows.push(db::WantedRow {
                        id: mint(&format!("s{season:02}e{:03}", ep.episode)),
                        request_id: req.id.clone(),
                        kind: "episode".into(),
                        tmdb_id: req.tmdb_id,
                        imdb_id: detail.imdb_id.clone(),
                        title: req.title.clone(),
                        year: req.year,
                        season: Some(season),
                        episode: Some(ep.episode),
                        air_date: ep.air_date.clone(),
                        status: "wanted".into(),
                        last_search_at: None,
                    });
                }
            }
            if rows.is_empty() {
                bail!("TMDB lists no episodes for the requested seasons");
            }
        }
    }
    Ok(rows)
}

// ----- availability matching ------------------------------------------------------

/// Outcome of one matcher pass, for job logs.
#[derive(Debug, Default)]
pub struct MatchSummary {
    pub checked: usize,
    pub changed: usize,
}

/// Re-derive availability for every non-terminal request. Runs after each
/// library scan (chained) and on a daily safety-net cron: enrichment writes
/// `metadata.tmdbId` some time after the scan itself.
pub fn availability_pass<S: HostCtx>(state: &S) -> Result<MatchSummary> {
    let conn = state.db().get()?;
    let all = db::list_requests(&conn, None)?;
    drop(conn);
    let mut summary = MatchSummary::default();
    for req in all {
        if matches!(req.status, RequestStatus::Denied | RequestStatus::Failed) {
            continue;
        }
        summary.checked += 1;
        if let Some(new_status) = match_one(state, &req.id)? {
            if new_status != req.status {
                summary.changed += 1;
                publish(state, &req.id, new_status);
            }
        }
    }
    Ok(summary)
}

// ----- TMDB refresh (keep the wanted ledger + air signals current) ---------------

/// Minimum spacing between TMDB refreshes of the same request (~3h), so the
/// 6-hourly refresh cron never re-hits TMDB for a request touched recently
/// (e.g. just approved / merged).
const REFRESH_MIN_INTERVAL_MS: i64 = 3 * 60 * 60 * 1000;

/// TMDB `status` strings for a show whose episode set is fixed forever: no new
/// episodes will air, so once refreshed we never need to fetch it again.
fn is_ended(air_status: Option<&str>) -> bool {
    matches!(air_status, Some("Ended") | Some("Canceled"))
}

/// Should this request be re-fetched from TMDB this pass? Skips terminal
/// requests, throttles by `last_refresh_at`, and skips the requests that CANNOT
/// change: a released movie already on disk, and an ended/canceled show (its
/// episode set is complete once we've recorded that status). Ongoing shows and
/// unreleased / not-yet-available movies always refresh (subject to the throttle).
fn needs_refresh(req: &MediaRequest, now: i64) -> bool {
    if matches!(req.status, RequestStatus::Denied | RequestStatus::Failed) {
        return false;
    }
    if let Some(ts) = req.last_refresh_at {
        if now - ts < REFRESH_MIN_INTERVAL_MS {
            return false;
        }
    }
    match req.kind {
        // A released movie already in the library will not change.
        RequestKind::Movie => req.status != RequestStatus::Available,
        // An ended/canceled show gets no new episodes; refresh it once (to record
        // the ended status + backfill air dates), then never again.
        RequestKind::Show => !is_ended(req.air_status.as_deref()),
    }
}

/// Re-fetch every refreshable request from TMDB: ADDITIVELY extend its wanted
/// ledger with newly-aired episodes, backfill air dates TMDB now knows, and
/// store the airing signals (air_status / next_air_date). Bounded by
/// [`needs_refresh`] + the throttle so TMDB is never hammered. Called by the
/// `acquisition.refresh` job.
pub fn refresh_pass<S: HostCtx>(state: &S) -> Result<usize> {
    let conn = state.db().get()?;
    let all = db::list_requests(&conn, None)?;
    drop(conn);
    let now = now_ms();
    let mut refreshed = 0usize;
    for req in all {
        if !needs_refresh(&req, now) {
            continue;
        }
        // Per-request failures (a vanished TMDB id, a transient error) must not
        // abort the whole pass; the next cron retries.
        if let Err(e) = refresh_one(state, &req) {
            tracing::warn!(target: "requests", request = %req.id, "refresh failed: {e:#}");
            continue;
        }
        refreshed += 1;
    }
    Ok(refreshed)
}

/// Refresh one request: one TMDB detail fetch, then additive ledger merge + air
/// signals. A show additively extends its episode ledger; a movie has nothing to
/// extend but backfills its wanted row's air (release) date once TMDB knows it.
fn refresh_one<S: HostCtx>(state: &S, req: &MediaRequest) -> Result<()> {
    let key = tmdb_key(state)?;
    let lang = language(state);
    let detail = discover::detail(&key, &lang, req.kind, req.tmdb_id)
        .map_err(|()| anyhow!("TMDB lookup failed"))?
        .ok_or_else(|| anyhow!("title not found on TMDB"))?;

    if req.kind == RequestKind::Show {
        refresh_wanted(state, req, &detail)?;
    } else if let Some(avail) = detail.available_date.as_deref() {
        // Movie: backfill the wanted row's air date once TMDB publishes it, so a
        // movie requested before its release date was known still gets gated out
        // of search until release, then auto-grabbed. set_wanted_air_date writes
        // only rows whose air_date IS NULL, so a known date is never overwritten
        // (and a grabbed/available row is left untouched by the search gate).
        let conn = state.db().get()?;
        let rows = db::wanted_for_request(&conn, &req.id)?;
        drop(conn);
        for w in rows.iter().filter(|w| w.air_date.is_none()) {
            db::set_wanted_air_date(state.db(), &w.id, avail, now_ms())?;
        }
    }

    // Airing signals: show = its next episode's date; movie = its soonest
    // availability, but only while still in the future (a past date is "already
    // out", so no upcoming badge).
    let today = today_ymd();
    let next_air_date = match req.kind {
        RequestKind::Show => detail.next_air.as_ref().map(|(d, _, _)| d.clone()),
        RequestKind::Movie => detail.available_date.clone().filter(|d| d.as_str() > today.as_str()),
    };
    db::set_request_air(
        state.db(),
        &req.id,
        detail.status.as_deref(),
        next_air_date.as_deref(),
        now_ms(),
    )?;
    Ok(())
}

/// ADDITIVELY reconcile a show's wanted ledger against TMDB: INSERT rows for
/// (season, episode) pairs not yet present (status `wanted`, with air date), and
/// backfill an air date onto an existing row that lacked one. NEVER deletes a
/// row and NEVER changes a row's status, so grabbed/available episodes are
/// untouched. This is what lets an ongoing show's newly-aired episodes finally
/// enter the ledger (and thus get searched / matched) over time.
fn refresh_wanted<S: HostCtx>(
    state: &S,
    req: &MediaRequest,
    detail: &discover::DiscoverRawDetail,
) -> Result<()> {
    let conn = state.db().get()?;
    let existing = db::wanted_for_request(&conn, &req.id)?;
    drop(conn);
    // Only EXTEND an existing ledger, never create one: a pending (not-yet-
    // approved) request has no wanted rows and must stay that way, else the
    // search pass would start grabbing before a moderator green-lit it. Approval
    // (materialize_wanted) always seeds a non-empty ledger, so an approved show
    // reaches here with rows.
    if existing.is_empty() {
        return Ok(());
    }
    let desired = build_wanted_rows_from(state, req, detail)?;

    use std::collections::HashMap;
    // Key on (season, episode) rather than id so a row created under an older id
    // formula still dedups correctly. Movie rows key on (None, None).
    let have: HashMap<(Option<u32>, Option<u32>), &db::WantedRow> =
        existing.iter().map(|w| ((w.season, w.episode), w)).collect();

    let mut to_insert: Vec<db::WantedRow> = Vec::new();
    for d in desired {
        match have.get(&(d.season, d.episode)) {
            // New (season, episode): add it as a plain wanted row.
            None => to_insert.push(d),
            // Present already: only backfill a now-known air date (the writer
            // guards on `air_date IS NULL`, so a set date is never overwritten
            // and the row's status is left alone).
            Some(existing_row) => {
                if existing_row.air_date.is_none() {
                    if let Some(air) = d.air_date.as_deref() {
                        db::set_wanted_air_date(state.db(), &existing_row.id, air, now_ms())?;
                    }
                }
            }
        }
    }
    db::insert_wanted(state.db(), &to_insert, now_ms())?;
    Ok(())
}

/// Directly fulfill a request from a completed import, WITHOUT waiting for the
/// scan -> TMDB-enrich -> match-by-tmdbId chain (which is fragile: enrichment
/// may not recover the id). We already know this download satisfied the request,
/// so flip its `grabbed` wanted rows to `available` and recompute the request
/// status. Called from the importer when a request-linked download lands.
pub fn on_download_imported<S: HostCtx>(state: &S, request_id: &str) -> Result<()> {
    let conn = state.db().get()?;
    let Some(req) = db::get_request(&conn, request_id)? else {
        return Ok(());
    };
    let wanted = db::wanted_for_request(&conn, request_id)?;
    drop(conn);
    // Rows this request grabbed are now on disk = available.
    let grabbed: Vec<String> =
        wanted.iter().filter(|w| w.status == "grabbed").map(|w| w.id.clone()).collect();
    if !grabbed.is_empty() {
        db::set_wanted_status(state.db(), &grabbed, "available", now_ms())?;
    }
    // Recompute from the (updated) ledger: all available -> available, some -> partial.
    let conn = state.db().get()?;
    let wanted = db::wanted_for_request(&conn, request_id)?;
    drop(conn);
    let status = if wanted.is_empty() || wanted.iter().all(|w| w.status == "available") {
        RequestStatus::Available
    } else if wanted.iter().any(|w| w.status == "available") {
        RequestStatus::PartiallyAvailable
    } else {
        return Ok(()); // nothing became available (shouldn't happen post-import)
    };
    if req.status != status {
        db::set_request_status(state.db(), request_id, status, None, None, now_ms())?;
        publish(state, request_id, status);
    }
    Ok(())
}

/// Match one request against the local catalog. Returns the (possibly
/// unchanged) derived status, or `None` when no judgement is possible (show
/// not yet in library, pending show with no wanted ledger...). Never
/// downgrades a request that already reached `available`.
pub fn match_one<S: HostCtx>(state: &S, id: &str) -> Result<Option<RequestStatus>> {
    let conn = state.db().get()?;
    let Some(req) = db::get_request(&conn, id)? else {
        return Ok(None);
    };
    match req.kind {
        RequestKind::Movie => {
            let Some(_item) = db::movie_item_by_tmdb(&conn, req.tmdb_id)? else {
                return Ok(None);
            };
            let wanted_ids: Vec<String> =
                db::wanted_for_request(&conn, &req.id)?.into_iter().map(|w| w.id).collect();
            drop(conn);
            db::set_wanted_status(state.db(), &wanted_ids, "available", now_ms())?;
            if req.status != RequestStatus::Available {
                db::set_request_status(state.db(), &req.id, RequestStatus::Available, None, None, now_ms())?;
            }
            Ok(Some(RequestStatus::Available))
        }
        RequestKind::Show => {
            let Some(show_id) = db::show_by_tmdb(&conn, req.tmdb_id)? else {
                return Ok(None);
            };
            let present: std::collections::HashSet<(u32, u32)> =
                db::episodes_present(&conn, &show_id)?.into_iter().collect();
            let wanted = db::wanted_for_request(&conn, &req.id)?;
            drop(conn);
            if wanted.is_empty() {
                // Pending show: no episode ledger yet, so no exact judgement.
                return Ok(None);
            }
            let today = today_ymd();
            let mut newly_available: Vec<String> = Vec::new();
            let (mut aired, mut have) = (0usize, 0usize);
            for w in &wanted {
                let (Some(s), Some(e)) = (w.season, w.episode) else { continue };
                let is_aired = w.air_date.as_deref().is_none_or(|d| d <= today.as_str());
                if !is_aired {
                    continue;
                }
                aired += 1;
                if present.contains(&(s, e)) {
                    have += 1;
                    if w.status != "available" {
                        newly_available.push(w.id.clone());
                    }
                }
            }
            db::set_wanted_status(state.db(), &newly_available, "available", now_ms())?;
            let new_status = if aired > 0 && have == aired {
                RequestStatus::Available
            } else if have > 0 {
                RequestStatus::PartiallyAvailable
            } else {
                return Ok(None);
            };
            // Never regress a fully-available request (e.g. temporary unmount).
            if req.status == RequestStatus::Available && new_status != RequestStatus::Available {
                return Ok(Some(RequestStatus::Available));
            }
            if new_status != req.status {
                db::set_request_status(state.db(), &req.id, new_status, None, None, now_ms())?;
            }
            Ok(Some(new_status))
        }
    }
}

/// Today as `YYYY-MM-DD` (UTC), the wanted ledger's air-date vocabulary.
pub fn today_ymd() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}-{:02}", now.year(), u8::from(now.month()), now.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(season: u32, episode: u32) -> EpisodeRef {
        EpisodeRef { season, episode }
    }

    #[test]
    fn merge_target_whole_show_absorbs_any_ask() {
        // Whole show (both None) + an episode ask stays whole show, never narrows.
        assert_eq!(merge_target(None, None, None, Some(vec![ep(1, 3)])), (None, None));
        // A subset ask merged into whole show also stays whole show.
        assert_eq!(merge_target(None, None, Some(vec![2]), None), (None, None));
        // Whole-show ask widens an existing subset back to whole show.
        assert_eq!(merge_target(Some(vec![1]), None, None, None), (None, None));
    }

    #[test]
    fn merge_target_unions_seasons_and_episodes() {
        // Seasons union (a None side here is the EMPTY set, not "all").
        assert_eq!(
            merge_target(Some(vec![1]), None, Some(vec![2, 1]), None),
            (Some(vec![1, 2]), None)
        );
        // Episode-only ask unions onto an existing episode-only request.
        assert_eq!(
            merge_target(None, Some(vec![ep(1, 3)]), None, Some(vec![ep(1, 4)])),
            (None, Some(vec![ep(1, 3), ep(1, 4)]))
        );
        // A season subset plus a stray episode coexist as a mixed target.
        assert_eq!(
            merge_target(Some(vec![2]), None, None, Some(vec![ep(1, 5)])),
            (Some(vec![2]), Some(vec![ep(1, 5)]))
        );
        // Duplicate episode across asks collapses to one.
        assert_eq!(
            merge_target(None, Some(vec![ep(1, 3)]), None, Some(vec![ep(1, 3)])),
            (None, Some(vec![ep(1, 3)]))
        );
    }

    #[test]
    fn normalize_episodes_empty_is_none() {
        assert_eq!(normalize_episodes(Some(vec![])), None);
        assert_eq!(normalize_episodes(None), None);
        assert_eq!(
            normalize_episodes(Some(vec![ep(2, 1), ep(1, 2), ep(1, 2)])),
            Some(vec![ep(1, 2), ep(2, 1)])
        );
    }
}
