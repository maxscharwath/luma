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

use luma_db as db;
use luma_domain::{Permission, User};

use crate::{
    AnalyzeBody, DownloadView, DownloadsView, ManualAddBody, ManualSearchBody, ManualSearchView,
    TorrentAnalysis, TorrentFileView,
};
use luma_engine::state::SharedState;
use luma_module_host::{blocking, json_error, query, service, AuthUser, HostCtx};

use crate::{DownloadManager, GrabSpec};

/// Resolve the module's download manager from the host service registry.
fn dm(state: &SharedState) -> Arc<DownloadManager> {
    service::<DownloadManager>(&**state).expect("download manager registered")
}

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/downloads", get(list))
        .route("/downloads/search", post(manual_search))
        .route("/downloads/analyze", post(analyze))
        .route("/downloads/add", post(manual_add))
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
    let view = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let rows = db::list_downloads(&conn, HISTORY_LIMIT)?;
        // Hydrate display names in one pass (few clients, few requests).
        let clients: std::collections::HashMap<String, String> =
            db::list_download_clients(&conn)?.into_iter().map(|c| (c.id, c.name)).collect();
        let indexers: std::collections::HashMap<String, String> =
            db::list_indexers(&conn)?.into_iter().map(|i| (i.id, i.name)).collect();
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

/// `POST /api/admin/downloads/search` free-text sweep of every indexer,
/// returning parsed releases best-first for the admin to pick from.
pub async fn manual_search(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ManualSearchBody>,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let view: ManualSearchView =
        match tokio::task::spawn_blocking(move || {
            crate::acquisition::search::manual_search(&state, &body.query)
        })
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
            Err(_) => return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")),
        };
    Ok(Json(view).into_response())
}

/// `POST /api/admin/downloads/analyze` fetch the torrent's file list (metadata
/// only, no download) and classify what it holds, so the admin can select
/// episodes / confirm the entity before grabbing.
pub async fn analyze(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<AnalyzeBody>,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let magnet = body.magnet_or_url.trim().to_string();
    if magnet.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "a magnet or .torrent URL is required"));
    }
    let analysis = match tokio::task::spawn_blocking(move || {
        let entries = dm(&state).list_files(&state, &magnet)?;
        let files: Vec<(String, u64)> =
            entries.iter().map(|e| (e.path.clone(), e.size_bytes)).collect();
        let content = luma_scene::classify(&files);
        anyhow::Ok((entries, content))
    })
    .await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
        Err(_) => return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")),
    };
    let (entries, content) = analysis;
    let files = entries
        .iter()
        .zip(content.files.iter())
        .map(|(e, c)| TorrentFileView {
            index: e.index,
            path: e.path.clone(),
            size_bytes: e.size_bytes,
            is_video: c.is_video,
            season: c.season,
            episode: c.episode,
        })
        .collect();
    Ok(Json(TorrentAnalysis { kind: content.kind.as_str().to_string(), seasons: content.seasons, files })
        .into_response())
}

/// `POST /api/admin/downloads/add` grab a pasted magnet / `.torrent` URL (or a
/// manual-search result) and import it as the given kind into the right library.
pub async fn manual_add(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ManualAddBody>,
) -> Result<Response, Response> {
    require_downloads(&state, &user)?;
    let magnet = body.magnet_or_url.trim().to_string();
    if magnet.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "a magnet or .torrent URL is required"));
    }
    if !matches!(body.kind.as_str(), "movie" | "episode" | "season") {
        return Err(json_error(StatusCode::BAD_REQUEST, "kind must be movie, episode or season"));
    }
    let title = body.title.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    // A readable release label (magnet dn=, else the title, else "manual").
    let release_title = magnet_display_name(&magnet)
        .or_else(|| title.clone())
        .unwrap_or_else(|| "manual".to_string());
    let episodes = body.episode.map(|e| vec![e]);
    let only_files = body.only_files.filter(|f| !f.is_empty());
    let spec = GrabSpec {
        magnet_or_url: magnet,
        kind: body.kind,
        tmdb_id: body.tmdb_id.unwrap_or(0),
        title,
        year: body.year,
        season: body.season,
        episodes,
        release_title,
        only_files,
        details_url: body.details_url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
        ..Default::default()
    };
    let grab_state = state.clone();
    let result = blocking(move || Ok(dm(&grab_state).grab(&grab_state, spec))).await?;
    match result {
        Ok(row) => {
            let id = row.id.clone();
            // Slow engine add runs in the background so the request returns now.
            tokio::task::spawn_blocking(move || dm(&state).activate(&state, &row));
            Ok(Json(json!({ "id": id })).into_response())
        }
        Err(e) => Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
    }
}

/// Best-effort human name from a magnet's `dn=` parameter.
fn magnet_display_name(magnet: &str) -> Option<String> {
    let idx = magnet.find("dn=")?;
    let raw: String = magnet[idx + 3..].chars().take_while(|&c| c != '&').collect();
    let decoded = raw.replace('+', " ").replace("%20", " ");
    let trimmed = decoded.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
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
        // background (a big copy, so don't block the request).
        tokio::task::spawn_blocking(move || {
            if let Err(e) = crate::acquisition::import::import_single(&state, &id) {
                tracing::warn!(id = %id, error = %format!("{e:#}"), "retry import failed");
            }
        });
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
