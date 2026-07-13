//! `/api/admin/downloads` the download queue + history, with pause / resume
//! / remove (optionally deleting data). Readable and drivable by either
//! `requests.manage` (the moderator who grabbed) or `settings.manage`.

use std::sync::Arc;

use axum::extract::{Path as AxPath, Query as AxQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::db;
use luma_module_sdk::domain::{Permission, User};

use crate::{DownloadView, DownloadsView};
use luma_module_sdk::engine::state::SharedState;
use luma_module_sdk::host::{blocking, json_error, query, service, AuthUser, HostCtx};

use crate::DownloadManager;

/// Resolve the module's download manager from the host service registry.
fn dm(state: &SharedState) -> Arc<DownloadManager> {
    service::<DownloadManager>(&**state).expect("download manager registered")
}

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/downloads", get(list))
        .route("/downloads/pause-all", post(pause_all))
        .route("/downloads/resume-all", post(resume_all))
        .route("/downloads/reannounce", post(reannounce_all))
        .route("/downloads/:id/pause", post(pause))
        .route("/downloads/:id/resume", post(resume))
        .route("/downloads/:id/retry", post(retry))
        .route("/downloads/:id/reannounce", post(reannounce))
        .route("/downloads/:id", axum::routing::delete(remove))
}

/// Queue access: the requests moderator or a settings admin.
fn require_downloads(state: &SharedState, user: &User) -> Result<(), Response> {
    if user.can(Permission::RequestsManage) || user.can(Permission::SettingsManage) {
        Ok(())
    } else {
        state.require(user, Permission::SettingsManage)
    }
}

const HISTORY_LIMIT: usize = 200;

/// `GET /api/admin/downloads`
pub async fn list(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let vpn = dm(&state).vpn_status();
    // Indexer display names come from the indexer module via its port, resolved
    // here (before the blocking closure, which can't borrow the host).
    let indexers: std::collections::HashMap<String, String> =
        luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerDbPort>(&state)
            .and_then(|p| p.list_indexers(&state).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|i| (i.id, i.name))
            .collect();
    let view = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let rows = db::list_downloads(&conn, HISTORY_LIMIT)?;
        // Hydrate display names in one pass (few clients, few requests).
        let clients: std::collections::HashMap<String, String> =
            db::list_download_clients(&conn)?.into_iter().map(|c| (c.id, c.name)).collect();
        let downloads = rows
            .into_iter()
            .map(|d| {
                let req = d.request_id.as_deref().and_then(|rid| db::get_request(&conn, rid).ok().flatten());
                let title = req.as_ref().map(|r| r.title.clone()).unwrap_or_else(|| d.release_title.clone());
                let poster_url = req.as_ref().and_then(|r| r.poster_url.clone());
                let indexer_name = d.indexer_id.as_deref().and_then(|id| indexers.get(id).cloned());
                // Link to the LUMA fiche once the title is in the library.
                let local_id = req.as_ref().and_then(|r| {
                    if d.kind == "movie" {
                        db::movie_item_by_tmdb(&conn, r.tmdb_id).ok().flatten()
                    } else {
                        db::show_by_tmdb(&conn, r.tmdb_id).ok().flatten()
                    }
                });
                DownloadView {
                    id: d.id,
                    client_name: clients.get(&d.client_id).cloned().unwrap_or_else(|| d.client_id.clone()),
                    client_id: d.client_id,
                    request_id: d.request_id,
                    kind: d.kind,
                    title,
                    release_title: d.release_title,
                    season: d.season,
                    episodes: d.episodes,
                    status: d.status,
                    progress: d.progress,
                    size_bytes: d.size_bytes,
                    score: d.score,
                    error: d.error,
                    grabbed_at: d.grabbed_at,
                    completed_at: d.completed_at,
                    imported_at: d.imported_at,
                    indexer_name,
                    details_url: d.details_url,
                    info_hash: d.info_hash,
                    poster_url,
                    local_id,
                }
            })
            .collect();
        Ok(DownloadsView { downloads, vpn })
    })
    .await?;
    Ok(Json(view).into_response())
}

async fn act(
    state: SharedState,
    user: User,
    id: String,
    f: impl FnOnce(&SharedState, &str) -> anyhow::Result<()> + Send + 'static,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let st = state.clone();
    let outcome = blocking(move || Ok(f(&st, &id))).await?;
    match outcome {
        Ok(()) => Ok(Json(json!({ "ok": true })).into_response()),
        Err(e) if format!("{e:#}").contains("not found") => {
            Err(state.lerr(&user, StatusCode::NOT_FOUND, "error.downloadNotFound"))
        }
        Err(e) => Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
    }
}

/// `POST /api/admin/downloads/:id/pause`
pub async fn pause(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    let downloads = dm(&state);
    act(state.clone(), user, id, move |st, id| downloads.pause(st, id)).await
}

/// `POST /api/admin/downloads/:id/resume`
pub async fn resume(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    let downloads = dm(&state);
    act(state.clone(), user, id, move |st, id| downloads.resume(st, id)).await
}

/// `POST /api/admin/downloads/:id/reannounce` "ask more peers" for one download.
pub async fn reannounce(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    let downloads = dm(&state);
    act(state.clone(), user, id, move |st, id| downloads.reannounce(st, id)).await
}

/// `{ "count": N }` from a bulk queue action.
fn bulk_response(out: anyhow::Result<usize>) -> Result<Response, Response> {
    match out {
        Ok(count) => Ok(Json(json!({ "count": count })).into_response()),
        Err(e) => Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
    }
}

/// `POST /api/admin/downloads/pause-all`
pub async fn pause_all(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    bulk_response(blocking(move || Ok(dm(&state).pause_all(&state))).await?)
}

/// `POST /api/admin/downloads/resume-all`
pub async fn resume_all(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    bulk_response(blocking(move || Ok(dm(&state).resume_all(&state))).await?)
}

/// `POST /api/admin/downloads/reannounce` force a tracker re-announce ("ask more
/// peers") on every active download.
pub async fn reannounce_all(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    bulk_response(blocking(move || Ok(dm(&state).reannounce_all(&state))).await?)
}

/// `POST /api/admin/downloads/:id/retry` re-attempt a failed step. A `completed`
/// download whose import failed (e.g. the library volume was offline) is
/// re-IMPORTED without re-downloading; a `failed` grab is reset and re-added.
/// Both run in the background so the request returns immediately.
pub async fn retry(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let status = {
        let conn = state.db.get().map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "db"))?;
        match db::get_download(&conn, &id).ok().flatten() {
            Some(row) => row.status,
            None => return Err(json_error(StatusCode::NOT_FOUND, "download not found")),
        }
    };
    if status == "completed" || status == "imported" {
        // Import failed earlier (or re-run to re-fulfill): re-import in the
        // background through the Acquisition module's import job, which owns the
        // import logic now (this crate names no acquisition type, so no cycle).
        // The import pass only considers `completed` rows, so an already-imported
        // row is flipped back to `completed` first; the import is idempotent, so
        // existing library files are skipped and the request is re-fulfilled.
        if status == "imported" {
            let _ = db::set_download_status(&state.db, &id, "completed", None);
        }
        state.trigger_job("acquisition.import", "retry-import");
        return Ok(Json(json!({ "ok": true })).into_response());
    }
    // Otherwise re-download: reset the row + re-add the torrent in the background.
    let reset_state = state.clone();
    let row = match blocking(move || Ok(dm(&reset_state).retry(&reset_state, &id))).await? {
        Ok(row) => row,
        Err(e) if format!("{e:#}").contains("not found") => {
            return Err(json_error(StatusCode::NOT_FOUND, "download not found"))
        }
        Err(e) => return Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
    };
    tokio::task::spawn_blocking(move || dm(&state).activate(&state, &row));
    Ok(Json(json!({ "ok": true })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct RemoveParams {
    #[serde(rename = "deleteData", default)]
    delete_data: bool,
}

/// `DELETE /api/admin/downloads/:id?deleteData=true`
pub async fn remove(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    AxQuery(params): AxQuery<RemoveParams>,
) -> Result<Response, Response> {
    let downloads = dm(&state);
    act(state.clone(), user, id, move |st, id| downloads.remove(st, id, params.delete_data)).await
}
