//! The Acquisition module's admin API (`/api/admin/acquisition/*`): the
//! free-text manual indexer search, the torrent analysis (file list + kind),
//! and the manual add (grab a pasted magnet / .torrent and import it). Mounted
//! behind the module's enabled-gate by the host, so the whole surface 404s while
//! the module is disabled. The download QUEUE routes stay in `kroma-torrent`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use kroma_module_sdk::domain::{Permission, User};
use kroma_module_sdk::host::{blocking, json_error, AuthUser, HostCtx};

use crate::dtos::{
    AnalyzeBody, ManualAddBody, ManualSearchBody, ManualSearchView, TorrentAnalysis, TorrentFileView,
};

pub fn routes<S: HostCtx + Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/acquisition/search", post(manual_search::<S>))
        .route("/acquisition/analyze", post(analyze::<S>))
        .route("/acquisition/add", post(manual_add::<S>))
}

/// Acquisition access: the requests moderator or a settings admin.
fn require_acquisition<S: HostCtx>(state: &S, user: &User) -> Result<(), Response> {
    if user.can(Permission::RequestsManage) || user.can(Permission::SettingsManage) {
        Ok(())
    } else {
        state.require(user, Permission::SettingsManage)
    }
}

/// `POST /api/admin/acquisition/search` free-text sweep of every indexer,
/// returning parsed releases best-first for the admin to pick from.
pub async fn manual_search<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
    Json(body): Json<ManualSearchBody>,
) -> Result<Response, Response> {
    require_acquisition(&state, &user)?;
    let view: ManualSearchView =
        match tokio::task::spawn_blocking(move || crate::search::manual_search(&state, &body.query))
            .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(json_error(StatusCode::BAD_REQUEST, &format!("{e:#}"))),
            Err(_) => return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")),
        };
    Ok(Json(view).into_response())
}

/// `POST /api/admin/acquisition/analyze` fetch the torrent's file list (metadata
/// only, no download) and classify what it holds, so the admin can select
/// episodes / confirm the entity before grabbing.
pub async fn analyze<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
    Json(body): Json<AnalyzeBody>,
) -> Result<Response, Response> {
    require_acquisition(&state, &user)?;
    let magnet = body.magnet_or_url.trim().to_string();
    if magnet.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "a magnet or .torrent URL is required"));
    }
    let analysis = match tokio::task::spawn_blocking(move || {
        let entries = crate::downloads(&state).list_files(&state, &magnet)?;
        let files: Vec<(String, u64)> =
            entries.iter().map(|e| (e.path.clone(), e.size_bytes)).collect();
        let content = kroma_module_sdk::scene::classify(&files);
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

/// `POST /api/admin/acquisition/add` grab a pasted magnet / `.torrent` URL (or a
/// manual-search result) and import it as the given kind into the right library.
pub async fn manual_add<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
    Json(body): Json<ManualAddBody>,
) -> Result<Response, Response> {
    require_acquisition(&state, &user)?;
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
    let spec = kroma_module_sdk::ports::GrabSpec {
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
    let result = blocking(move || Ok(crate::downloads(&grab_state).grab(&grab_state, spec))).await?;
    match result {
        Ok(row) => {
            let id = row.id.clone();
            // Slow engine add runs in the background so the request returns now.
            tokio::task::spawn_blocking(move || crate::downloads(&state).activate(&state, &row));
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
