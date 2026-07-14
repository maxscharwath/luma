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

use luma_module_host::{Event, HostCtx};

use crate::db;
use crate::infra::metadata::discover;
use crate::model::{CreateRequestBody, MediaRequest, Permission, RequestKind, RequestStatus, User};
use crate::services::jobs::now_ms;

// Request orchestration is generic over the host state `S: HostCtx` (settings /
// TMDB config / event bus / job triggers / DB reached through the seam), so it
// runs both in-core (`S = SharedState`, the request API) and out-of-process
// (`S = RemoteHost`, the acquisition `.lmod` calling the same fulfillment logic).

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

    // Normalize the season ask: None/empty = whole show; movies carry None.
    let asked_seasons: Option<Vec<u32>> = match body.kind {
        RequestKind::Movie => None,
        RequestKind::Show => body.seasons.clone().filter(|s| !s.is_empty()).map(|mut s| {
            s.sort_unstable();
            s.dedup();
            s
        }),
    };

    // Duplicate-merge: a second ask for an open title folds in (a show ask can
    // widen the season subset; re-materialized below if already approved).
    let conn = state.db().get()?;
    if let Some(existing) = db::find_open_request(&conn, body.kind, body.tmdb_id)? {
        drop(conn);
        let merged = merge_seasons(existing.seasons.clone(), asked_seasons);
        if body.kind == RequestKind::Show && merged != existing.seasons {
            db::set_request_seasons(state.db(), &existing.id, merged.as_deref(), now_ms())?;
            if matches!(existing.status, RequestStatus::Approved | RequestStatus::PartiallyAvailable) {
                // Already green-lit: extend the wanted ledger to the new seasons.
                materialize_wanted(state, &existing.id)?;
            }
            publish(state, &existing.id, existing.status);
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

/// Merge two season subsets; `None` (whole show) absorbs everything.
fn merge_seasons(a: Option<Vec<u32>>, b: Option<Vec<u32>>) -> Option<Vec<u32>> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            a.sort_unstable();
            a.dedup();
            Some(a)
        }
        _ => None,
    }
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
            air_date: None,
            status: "wanted".into(),
            last_search_at: None,
        }),
        RequestKind::Show => {
            let wanted_seasons: Vec<u32> = match &req.seasons {
                Some(list) => list.clone(),
                None => detail.seasons.iter().map(|s| s.season).collect(),
            };
            for season in wanted_seasons {
                let data = crate::infra::metadata::season_episodes(&key, &lang, req.tmdb_id, season);
                for ep in data.episodes {
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
