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
use crate::DownloadsExt;
use crate::model::{
    CreateRequestBody, MediaRequest, Permission, RequestCounts, RequestStatus, RequestsView, User,
};
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/requests", get(list).post(create))
        .route("/requests/:id", axum::routing::delete(remove))
        .route("/requests/:id/approve", post(approve))
        .route("/requests/:id/deny", post(deny))
        .route("/requests/:id/search", get(interactive_search))
        .route("/requests/:id/grab", post(grab))
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
    let active: std::collections::HashMap<String, luma_torrent::ActiveDownload> = luma_torrent::requests_with_active_downloads(conn)?
        .into_iter()
        .map(|a| (a.request_id.clone(), a))
        .collect();
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
    let view =
        service(move || luma_acquisition::search::interactive_search(&state, &id)).await?;
    Ok(Json(view).into_response())
}

/// `POST /api/requests/:id/grab` (requests.manage) manually grab one release
/// from the last interactive search of this request.
pub async fn grab(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<luma_acquisition::GrabBody>,
) -> Result<Response, Response> {
    require(&user, Permission::RequestsManage)?;
    require_acquisition(&state, &user)?;
    // Enqueue is fast (DB only); the slow torrent add (magnet resolve / .torrent
    // fetch, up to minutes) runs in the background so the request returns right
    // away instead of timing out the browser.
    let enqueue_state = state.clone();
    let (rid, guid, indexer_id) = (id.clone(), body.guid.clone(), body.indexer_id.clone());
    let row = service(move || {
        luma_acquisition::search::grab_cached(&enqueue_state, &rid, &guid, &indexer_id)
    })
    .await?;
    tokio::task::spawn_blocking(move || state.downloads().activate(&state, &row));
    Ok(Json(json!({ "ok": true, "id": id })).into_response())
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
