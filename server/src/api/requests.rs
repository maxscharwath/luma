//! `/api/requests` the media request queue. Users with `requests.create`
//! submit and track their own; `requests.manage` holders see everyone's and
//! approve / deny. Interactive search + manual grab join with the indexer
//! milestone.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::api::error::{json_error, lerr};
use crate::api::extract::AuthUser;
use crate::api::util::{blocking, query};
use crate::db;
use crate::i18n;
use crate::model::{
    CreateRequestBody, MediaRequest, Permission, RequestCounts, RequestStatus, RequestsView, User,
};
use crate::services::jobs::TriggerError;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/requests", get(list).post(create))
        .route("/requests/calendar", get(calendar))
        .route("/requests/missing", get(missing))
        .route("/requests/search-missing", post(search_all_missing))
        .route("/requests/{id}", axum::routing::delete(remove))
        .route("/requests/{id}/approve", post(approve))
        .route("/requests/{id}/deny", post(deny))
        .route("/requests/{id}/search", get(interactive_search))
        .route("/requests/{id}/auto-search", post(auto_search_one))
        .route("/requests/{id}/grab", post(grab))
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

/// The request-tied search + grab routes are core (a moderator drives them from
/// the request page), but the feature they call into lives in the Acquisition
/// module. Gate them on it so disabling Acquisition removes search / grab
/// everywhere, not only the module's own admin routes: 404 when it is off.
fn require_acquisition(state: &SharedState, user: &User) -> Result<(), Response> {
    if luma_engine::modules::module_enabled(&state.settings, "dev.luma.acquisition") {
        Ok(())
    } else {
        Err(lerr(locale(user), StatusCode::NOT_FOUND, "error.moduleDisabled"))
    }
}

/// Run a blocking service call whose failures are user-relevant (bad TMDB id,
/// unknown request...): surface the message as a 400 instead of a mute 500.
async fn service<T, F>(f: F) -> Result<T, Response>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
        Err(_) => Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")),
    }
}

fn counts_of(list: &[MediaRequest]) -> RequestCounts {
    let mut c = RequestCounts::default();
    for r in list {
        c.total += 1;
        match r.status {
            RequestStatus::Pending => c.pending += 1,
            RequestStatus::Denied => c.denied += 1,
            RequestStatus::Failed => c.failed += 1,
            RequestStatus::Available => c.available += 1,
            _ => c.active += 1,
        }
    }
    c
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Force own-requests-only for a manager (the user-facing page).
    #[serde(default)]
    mine: Option<bool>,
}

/// `GET /api/requests` own requests, or everyone's for `requests.manage`.
pub async fn list(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let all = user.can(Permission::RequestsManage) && !params.mine.unwrap_or(false);
    let uid = user.id.clone();
    let view = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let scope = if all { None } else { Some(uid.as_str()) };
        let mut requests = db::list_requests(&conn, scope)?;
        overlay_active_downloads(&conn, &mut requests)?;
        let counts = counts_of(&requests);
        Ok(RequestsView { requests, counts })
    })
    .await?;
    Ok(Json(view).into_response())
}

/// Overlay the transient acquisition phase straight from the download
/// relationship: a request with a live grab shows `downloading` (or `importing`
/// once a grab completed) + its progress, instead of its stored `approved`.
/// Deriving it here (rather than persisting a status) means it self-heals the
/// moment the torrent fails or is deleted.
fn overlay_active_downloads(
    conn: &rusqlite::Connection,
    requests: &mut [MediaRequest],
) -> rusqlite::Result<()> {
    let active = super::downloads_overlay::active_downloads(conn);
    for r in requests.iter_mut() {
        if !matches!(r.status, RequestStatus::Approved | RequestStatus::PartiallyAvailable) {
            continue;
        }
        if let Some(a) = active.get(&r.id) {
            r.status = if a.importing { RequestStatus::Importing } else { RequestStatus::Downloading };
            r.progress = Some(a.progress);
        }
    }
    Ok(())
}

/// `GET /api/requests/calendar` the "coming soon" feed: future-dated wanted rows
/// (a movie's availability date + a show episode's air date) not yet on disk,
/// ascending by date. Own requests, or everyone's for a `requests.manage` holder
/// (unless `?mine=true` forces own-only, like `GET /requests`).
pub async fn calendar(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let all = user.can(Permission::RequestsManage) && !params.mine.unwrap_or(false);
    let uid = user.id.clone();
    let today = crate::services::requests::today_ymd();
    let entries = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let scope = if all { None } else { Some(uid.as_str()) };
        Ok(db::upcoming_calendar(&conn, &today, scope, 300)?)
    })
    .await?;
    Ok(Json(entries).into_response())
}

/// `GET /api/requests/missing` the "missing / wanted" list: aired/released wanted
/// rows still not on disk (the inverse of the calendar), grouped client-side by
/// title. Own requests, or everyone's for a `requests.manage` holder.
pub async fn missing(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let all = user.can(Permission::RequestsManage) && !params.mine.unwrap_or(false);
    let uid = user.id.clone();
    let today = crate::services::requests::today_ymd();
    let entries = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let scope = if all { None } else { Some(uid.as_str()) };
        Ok(db::missing_items(&conn, &today, scope, 500)?)
    })
    .await?;
    Ok(Json(entries).into_response())
}

/// `POST /api/requests/search-missing` (requests.manage) "Search all missing":
/// kick the acquisition search pass now, which auto-grabs the best release for
/// every aired-but-open wanted row. Requires the Acquisition module (its sidecar
/// registered the `acquisition.search` job); returns the job run id, or 409 when
/// a pass is already running.
pub async fn search_all_missing(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    require_acquisition(&state, &user)?;
    let job = state
        .jobs
        .resolve("acquisition.search")
        .ok_or_else(|| lerr(locale(&user), StatusCode::NOT_FOUND, "error.moduleDisabled"))?;
    match state.jobs.trigger(state.clone(), job, "manual") {
        Ok(run_id) => Ok(Json(json!({ "runId": run_id })).into_response()),
        Err(TriggerError::AlreadyRunning) => {
            Err(json_error(StatusCode::CONFLICT, "a search pass is already running"))
        }
        Err(TriggerError::Unknown) => {
            Err(lerr(locale(&user), StatusCode::NOT_FOUND, "error.moduleDisabled"))
        }
    }
}

/// `POST /api/requests/:id/auto-search` (requests.manage) "search this title and
/// grab the best": run the interactive sweep for one request, pick the top
/// accepted, grabbable release, and grab it. This is the per-title "ask to watch"
/// button on the missing list. Slow (a live indexer sweep); the UI shows a
/// spinner. Returns `{ grabbed, title? }`.
pub async fn auto_search_one(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    require_acquisition(&state, &user)?;
    let port = acquisition_search(&state, &user)?;
    let rid = id.clone();
    let grabbed = service(move || {
        let view = port.interactive_search(&state, &rid)?;
        let Some((guid, indexer_id, title)) = best_release(&view) else {
            return Ok(None);
        };
        port.grab(&state, &rid, &guid, &indexer_id)?;
        Ok(Some(title))
    })
    .await?;
    match grabbed {
        Some(title) => Ok(Json(json!({ "grabbed": true, "title": title })).into_response()),
        None => Ok(Json(json!({ "grabbed": false })).into_response()),
    }
}

/// Pick the best grabbable release from an interactive-search view (opaque JSON
/// the acquisition sidecar returned): the highest-scoring release that carries a
/// grabbable link and was not rejected by the decision engine. Returns
/// `(guid, indexerId, title)`.
fn best_release(view: &serde_json::Value) -> Option<(String, String, String)> {
    view.get("releases")?
        .as_array()?
        .iter()
        .filter(|r| r.get("grabbable").and_then(serde_json::Value::as_bool).unwrap_or(false))
        .filter(|r| r.get("rejected").is_none_or(serde_json::Value::is_null))
        .filter_map(|r| {
            let score = r.get("score")?.as_i64()?;
            let guid = r.get("guid")?.as_str()?.to_string();
            let indexer_id = r.get("indexerId")?.as_str()?.to_string();
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            Some((score, guid, indexer_id, title))
        })
        .max_by_key(|(score, ..)| *score)
        .map(|(_, guid, indexer_id, title)| (guid, indexer_id, title))
}

/// `POST /api/requests` submit (duplicate-merging; auto-approve capability
/// honored inside the service).
pub async fn create(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateRequestBody>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let req =
        service(move || crate::services::requests::create_request(&state, &user, &body)).await?;
    Ok(Json(req).into_response())
}

/// `DELETE /api/requests/:id` a manager deletes anything; a requester may
/// withdraw their own request while it is still pending.
pub async fn remove(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsCreate)?;
    let loc = locale(&user);
    let manager = user.can(Permission::RequestsManage);
    let uid = user.id.clone();
    let id_for_event = id.clone();

    /// What the delete attempt found, so each case maps to its own status.
    enum Outcome {
        Deleted,
        NotFound,
        Forbidden,
    }
    let pool = state.db.clone();
    let outcome = blocking(move || {
        let conn = pool.get()?;
        let Some(req) = db::get_request(&conn, &id)? else {
            return Ok(Outcome::NotFound);
        };
        let own_pending =
            req.requested_by.as_deref() == Some(uid.as_str()) && req.status == RequestStatus::Pending;
        if !(manager || own_pending) {
            return Ok(Outcome::Forbidden);
        }
        drop(conn);
        db::delete_request(&pool, &id)?;
        Ok(Outcome::Deleted)
    })
    .await?;
    match outcome {
        Outcome::NotFound => Err(lerr(loc, StatusCode::NOT_FOUND, "error.requestNotFound")),
        Outcome::Forbidden => Err(lerr(loc, StatusCode::FORBIDDEN, "error.permissionDenied")),
        Outcome::Deleted => {
            state.events.publish(crate::infra::events::ServerEvent::RequestUpdated {
                id: id_for_event,
                status: "deleted".into(),
            });
            Ok(Json(json!({ "ok": true })).into_response())
        }
    }
}

/// `POST /api/requests/:id/approve` (requests.manage).
pub async fn approve(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    let reviewer = user.id.clone();
    let req =
        service(move || crate::services::requests::approve_request(&state, &id, Some(&reviewer)))
            .await?;
    Ok(Json(req).into_response())
}

/// `GET /api/requests/:id/search` (requests.manage) live interactive search:
/// sweep every enabled indexer for this request's targets and return scored
/// releases + rejects with reasons. Network-heavy (one or more Torznab round
/// trips per indexer); the UI shows a spinner.
pub async fn interactive_search(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    require_acquisition(&state, &user)?;
    let port = acquisition_search(&state, &user)?;
    let view = service(move || port.interactive_search(&state, &id)).await?;
    Ok(Json(view).into_response())
}

/// The manual-grab body (one release from the last interactive search).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrabBody {
    guid: String,
    indexer_id: String,
}

/// `POST /api/requests/:id/grab` (requests.manage) manually grab one release
/// from the last interactive search of this request.
pub async fn grab(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<GrabBody>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    require_acquisition(&state, &user)?;
    let port = acquisition_search(&state, &user)?;
    // The port enqueues (fast) and backgrounds the slow torrent add on the
    // acquisition sidecar, so the request returns right away.
    let rid = id.clone();
    service(move || port.grab(&state, &rid, &body.guid, &body.indexer_id)).await?;
    Ok(Json(json!({ "ok": true, "id": id })).into_response())
}

/// Resolve the acquisition module's search port (its sidecar), or a localized
/// "module disabled" 404 when it isn't installed / running.
fn acquisition_search(
    state: &SharedState,
    user: &User,
) -> Result<std::sync::Arc<dyn luma_module_sdk::ports::AcquisitionSearchPort>, Response> {
    luma_module_host::resolve_port::<dyn luma_module_sdk::ports::AcquisitionSearchPort>(state)
        .ok_or_else(|| lerr(locale(user), StatusCode::NOT_FOUND, "error.moduleDisabled"))
}

#[derive(Debug, Deserialize)]
pub struct DenyBody {
    #[serde(default)]
    note: Option<String>,
}

/// `POST /api/requests/:id/deny` (requests.manage).
pub async fn deny(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    body: Option<Json<DenyBody>>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    let note = body.and_then(|Json(b)| b.note).filter(|n| !n.trim().is_empty());
    let reviewer = user.id.clone();
    let req = service(move || {
        crate::services::requests::deny_request(&state, &id, &reviewer, note.as_deref())
    })
    .await?;
    Ok(Json(req).into_response())
}

#[cfg(test)]
mod route_tests {
    /// `/requests/calendar` (static) must coexist with `/requests/{id}` (param):
    /// building the router panics on a real matchit conflict, so this is enough.
    #[test]
    fn router_builds_without_conflict() {
        let _r = super::routes();
    }
}
