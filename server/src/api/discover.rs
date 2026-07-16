//! `/api/discover/*` TMDB discovery for the request flow: search titles the
//! library may not have, trending for the empty state, and a title detail
//! (with the season list for the picker). Every result is flagged against the
//! local catalog + open requests so cards render Play / chip / Demander
//! directly. Gated on `requests.create`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Deserialize;

use crate::api::error::{json_error, lerr};
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::db;
use crate::i18n;
use crate::infra::metadata::discover;
use crate::model::{
    DiscoverDetail, DiscoverEntry, DiscoverResponse, DiscoverSeason, Permission, RequestKind,
    RequestStatus, User,
};
use crate::services::settings;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/discover/search", get(search))
        .route("/discover/trending", get(trending))
        .route("/discover/{kind}/{tmdb_id}", get(detail))
}

fn locale(user: &User) -> &'static str {
    user.language.as_deref().and_then(i18n::normalize).unwrap_or(i18n::DEFAULT_LOCALE)
}

fn require(user: &User, perm: Permission) -> Result<(), Response> {
    if user.can(perm) {
        Ok(())
    } else {
        Err(lerr(locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

/// TMDB works zero-config via the built-in key, so absence means the operator
/// explicitly blanked it: surface that rather than a silent empty page.
fn require_tmdb(state: &SharedState, user: &User) -> Result<String, Response> {
    state
        .config
        .tmdb_api_key
        .clone()
        .ok_or_else(|| lerr(locale(user), StatusCode::SERVICE_UNAVAILABLE, "error.tmdbUnavailable"))
}

/// Route/query media-type vocabulary (`movie` | `tv`/`show`) -> TMDB scope,
/// shared by the search and trending handlers so the aliases can't drift.
fn scope_from_type(kind: Option<&str>) -> discover::DiscoverScope {
    match kind {
        Some("movie") => discover::DiscoverScope::Movies,
        Some("tv") | Some("show") => discover::DiscoverScope::Shows,
        _ => discover::DiscoverScope::All,
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: String,
    /// `movie` | `tv` | `all` (default).
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    page: Option<u32>,
}

/// `GET /api/discover/search?q=&type=&page=`
pub async fn search(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let key = require_tmdb(&state, &user)?;
    let lang = settings::metadata_language(&state.settings, &state.config);
    let query = params.q.trim().to_string();
    if query.is_empty() {
        return Ok(Json(DiscoverResponse { results: Vec::new(), page: 1, total_pages: 1 }).into_response());
    }
    let scope = scope_from_type(params.kind.as_deref());
    let page = params.page.unwrap_or(1);
    let out = blocking(move || {
        let found = discover::search(&key, &lang, scope, &query, page)
            .map_err(|()| anyhow::anyhow!("TMDB search failed"))?;
        let conn = state.db.get()?;
        Ok(DiscoverResponse {
            results: flag_hits(&conn, found.hits)?,
            page: found.page,
            total_pages: found.total_pages,
        })
    })
    .await?;
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize)]
pub struct TrendingParams {
    /// `movie` | `tv` (default: merged movies + shows).
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    page: Option<u32>,
}

/// `GET /api/discover/trending?type=&page=` this week's trending titles. No
/// `type` = merged movies + shows (page 1) for the discover empty-state rails;
/// `type=movie|tv` = a paginated single-kind list backing the full page.
pub async fn trending(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<TrendingParams>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let key = require_tmdb(&state, &user)?;
    let lang = settings::metadata_language(&state.settings, &state.config);
    let scope = scope_from_type(params.kind.as_deref());
    let page = params.page.unwrap_or(1);
    let out = blocking(move || {
        let found = discover::trending(&key, &lang, scope, page)
            .map_err(|()| anyhow::anyhow!("TMDB trending failed"))?;
        let conn = state.db.get()?;
        Ok(DiscoverResponse {
            results: flag_hits(&conn, found.hits)?,
            page: found.page,
            total_pages: found.total_pages,
        })
    })
    .await?;
    Ok(Json(out).into_response())
}

/// `GET /api/discover/{movie|tv}/:tmdbId` detail + seasons + availability.
pub async fn detail(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path((kind, tmdb_id)): Path<(String, u64)>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let key = require_tmdb(&state, &user)?;
    let loc = locale(&user);
    let kind = match kind.as_str() {
        "movie" => RequestKind::Movie,
        "tv" | "show" => RequestKind::Show,
        _ => return Err(json_error(StatusCode::NOT_FOUND, "unknown media type")),
    };
    let lang = settings::metadata_language(&state.settings, &state.config);
    let out = blocking(move || {
        let detail = discover::detail(&key, &lang, kind, tmdb_id)
            .map_err(|()| anyhow::anyhow!("TMDB detail failed"))?;
        let conn = state.db.get()?;
        detail.map(|d| flag_detail(&conn, d)).transpose()
    })
    .await?;
    match out {
        Some(d) => Ok(Json(d).into_response()),
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.itemNotFound")),
    }
}

// ----- catalog / request flagging ----------------------------------------------

fn local_id_for(conn: &Connection, kind: RequestKind, tmdb_id: u64) -> anyhow::Result<Option<String>> {
    Ok(match kind {
        RequestKind::Movie => db::movie_item_by_tmdb(conn, tmdb_id)?,
        RequestKind::Show => db::show_by_tmdb(conn, tmdb_id)?,
    })
}

fn flag_hits(conn: &Connection, hits: Vec<discover::DiscoverHit>) -> anyhow::Result<Vec<DiscoverEntry>> {
    // One pass over the download ledger (not per-hit), so cards can show the
    // live downloading/importing phase + progress.
    let active = super::downloads_overlay::active_downloads(conn);
    flag_hits_with(conn, hits, &active)
}

/// Flag hits against the catalog + open requests, reusing an already-built
/// active-download map so a caller (the detail page) doesn't re-aggregate the
/// download ledger a second time.
fn flag_hits_with(
    conn: &Connection,
    hits: Vec<discover::DiscoverHit>,
    active: &std::collections::HashMap<String, super::downloads_overlay::ActiveDownload>,
) -> anyhow::Result<Vec<DiscoverEntry>> {
    hits.into_iter()
        .map(|h| {
            let local_id = local_id_for(conn, h.kind, h.tmdb_id)?;
            let request = db::latest_request_for(conn, h.kind, h.tmdb_id)?;
            let (status, progress) = overlay_active(active, request.as_ref());
            Ok(DiscoverEntry {
                kind: h.kind,
                tmdb_id: h.tmdb_id,
                title: h.title,
                year: h.year,
                poster_url: h.poster_url,
                backdrop_url: h.backdrop_url,
                overview: h.overview,
                rating: h.rating,
                in_library: local_id.is_some(),
                local_id,
                request_id: request.as_ref().map(|(id, _)| id.clone()),
                request_status: status,
                request_progress: progress,
            })
        })
        .collect()
}

/// Overlay the live download phase + progress onto a request's stored status.
fn overlay_active(
    active: &std::collections::HashMap<String, super::downloads_overlay::ActiveDownload>,
    request: Option<&(String, RequestStatus)>,
) -> (Option<RequestStatus>, Option<f64>) {
    let mut status = request.map(|(_, s)| *s);
    let mut progress = None;
    if let Some((rid, _)) = request {
        if let Some(a) = active.get(rid) {
            if matches!(status, Some(RequestStatus::Approved | RequestStatus::PartiallyAvailable)) {
                status = Some(if a.importing {
                    RequestStatus::Importing
                } else {
                    RequestStatus::Downloading
                });
                progress = Some(a.progress);
            }
        }
    }
    (status, progress)
}

fn flag_detail(conn: &Connection, d: discover::DiscoverRawDetail) -> anyhow::Result<DiscoverDetail> {
    let local_id = local_id_for(conn, d.kind, d.tmdb_id)?;
    let request = db::latest_request_for(conn, d.kind, d.tmdb_id)?;

    // Overlay the live acquisition phase + progress from the download ledger, so
    // the detail page shows "Téléchargement 45%" the same way the queue does.
    let active = super::downloads_overlay::active_downloads(conn);
    let (request_status, request_progress) = overlay_active(&active, request.as_ref());

    // Season flags: available = every listed episode is on disk; requested =
    // covered by the newest open request (None = whole show).
    let mut seasons: Vec<DiscoverSeason> = Vec::with_capacity(d.seasons.len());
    if d.kind == RequestKind::Show && !d.seasons.is_empty() {
        let present = match &local_id {
            Some(show_id) => db::episodes_present(conn, show_id)?,
            None => Vec::new(),
        };
        let mut per_season: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for (s, _e) in present {
            *per_season.entry(s).or_default() += 1;
        }
        let open_request = db::find_open_request(conn, RequestKind::Show, d.tmdb_id)?;
        let requested_seasons: Option<Vec<u32>> = open_request.as_ref().map(|r| {
            r.seasons.clone().unwrap_or_else(|| d.seasons.iter().map(|s| s.season).collect())
        });
        for s in &d.seasons {
            let have = per_season.get(&s.season).copied().unwrap_or(0);
            seasons.push(DiscoverSeason {
                season: s.season,
                name: s.name.clone(),
                episode_count: s.episode_count,
                air_date: s.air_date.clone(),
                available: s.episode_count > 0 && have >= s.episode_count,
                episodes_available: have,
                requested: requested_seasons
                    .as_ref()
                    .is_some_and(|list| list.contains(&s.season)),
            });
        }
    }

    // Airing signals for the "coming soon" badge (show: next episode; movie:
    // soonest availability), computed before `d` is moved into the struct.
    let air_status = d.status.clone();
    let next_air_date = match d.kind {
        RequestKind::Show => d.next_air.as_ref().map(|(dt, _, _)| dt.clone()),
        RequestKind::Movie => d.available_date.clone(),
    };

    Ok(DiscoverDetail {
        kind: d.kind,
        tmdb_id: d.tmdb_id,
        title: d.title,
        year: d.year,
        poster_url: d.poster_url,
        backdrop_url: d.backdrop_url,
        overview: d.overview,
        tagline: d.tagline,
        genres: d.genres,
        rating: d.rating,
        runtime_min: d.runtime_min,
        seasons,
        cast: d.cast,
        crew: d.crew,
        similar: flag_hits_with(conn, d.similar, &active)?,
        in_library: local_id.is_some(),
        local_id,
        request_id: request.as_ref().map(|(id, _)| id.clone()),
        request_status,
        request_progress,
        air_status,
        next_air_date,
    })
}
