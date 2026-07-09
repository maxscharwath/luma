//! `/api/admin/download-clients` torrent engine management: the seeded
//! embedded engine row plus external Transmission / qBittorrent connectors,
//! with a live connection test. Gated on `settings.manage`; passwords are
//! write-only.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::api::error::{json_error, lerr};
use crate::api::extract::AuthUser;
use crate::api::util::{blocking, query};
use crate::db::{self, DownloadClientRow, EMBEDDED_CLIENT_ID};
use crate::model::{
    ClientTestResult, DownloadClientView, DownloadClientsView, Permission, SaveDownloadClientBody,
};
use crate::services::jobs::now_ms;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/download-clients", get(list).post(create))
        .route("/download-clients/:id", axum::routing::put(update).delete(remove))
        .route("/download-clients/:id/test", post(test))
}

fn view_of(row: &DownloadClientRow) -> DownloadClientView {
    DownloadClientView {
        id: row.id.clone(),
        kind: row.kind.clone(),
        name: row.name.clone(),
        url: row.url.clone(),
        username: row.username.clone(),
        has_password: !row.password.is_empty(),
        enabled: row.enabled,
        priority: row.priority,
        created_at: row.created_at,
        builtin: row.id == EMBEDDED_CLIENT_ID,
    }
}

/// `GET /api/admin/download-clients`
pub async fn list(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let view = query(&state.db, |pool| {
        let conn = pool.get()?;
        let clients = db::list_download_clients(&conn)?.iter().map(view_of).collect();
        Ok(DownloadClientsView { clients, rqbit_compiled: luma_torrent::RQBIT_COMPILED })
    })
    .await?;
    Ok(Json(view).into_response())
}

/// `POST /api/admin/download-clients` add an external engine.
pub async fn create(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveDownloadClientBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let kind = body.kind.as_deref().unwrap_or_default().trim().to_string();
    if !matches!(kind.as_str(), "transmission" | "qbittorrent") {
        return Err(json_error(StatusCode::BAD_REQUEST, "kind must be transmission or qbittorrent"));
    }
    let url = body.url.as_deref().map(str::trim).unwrap_or_default().to_string();
    if url.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "url is required"));
    }
    let row = DownloadClientRow {
        id: crate::services::scan::short_hash(&format!(
            "dlclient|{url}|{}",
            crate::services::auth::random_token()
        )),
        name: body
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(kind.as_str())
            .to_string(),
        kind,
        url,
        username: body.username.unwrap_or_default().trim().to_string(),
        password: body.password.unwrap_or_default(),
        enabled: body.enabled.unwrap_or(true),
        priority: body.priority.unwrap_or(0),
        created_at: now_ms(),
    };
    let view = view_of(&row);
    query(&state.db, move |pool| db::insert_download_client(&pool, &row)).await?;
    Ok(Json(view).into_response())
}

/// `PUT /api/admin/download-clients/:id` partial update (kind is immutable;
/// empty password keeps the stored secret).
pub async fn update(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<SaveDownloadClientBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    let id2 = id.clone();
    let updated = query(&state.db, move |pool| {
        db::update_download_client(
            &pool,
            &id2,
            body.name.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.url.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.username.as_deref().map(str::trim),
            body.password.as_deref().filter(|s| !s.is_empty()),
            body.enabled,
            body.priority,
        )
    })
    .await?;
    if !updated {
        return Err(lerr(loc, StatusCode::NOT_FOUND, "error.clientNotFound"));
    }
    // Toggling the embedded engine fully starts/stops its BitTorrent session, so
    // "disabled" means zero traffic (no download, no seed, no DHT), not just a
    // gate on new grabs.
    if body.enabled.is_some() && id == crate::db::EMBEDDED_CLIENT_ID {
        let enabled = body.enabled.unwrap();
        if enabled {
            state.downloads.start_rqbit(&state).await;
            let downloads = state.downloads.clone();
            let state2 = state.clone();
            blocking(move || {
                downloads.resume_after_enable(&state2);
                Ok(())
            })
            .await?;
        } else {
            let downloads = state.downloads.clone();
            let state2 = state.clone();
            blocking(move || {
                downloads.disable_embedded(&state2);
                Ok(())
            })
            .await?;
        }
    }
    let id3 = id.clone();
    let row = query(&state.db, move |pool| {
        let conn = pool.get()?;
        Ok(db::get_download_client(&conn, &id3)?)
    })
    .await?;
    match row {
        Some(r) => Ok(Json(view_of(&r)).into_response()),
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.clientNotFound")),
    }
}

/// `DELETE /api/admin/download-clients/:id` (the embedded row is permanent).
pub async fn remove(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    if id == EMBEDDED_CLIENT_ID {
        return Err(json_error(StatusCode::BAD_REQUEST, "the embedded engine cannot be deleted"));
    }
    let deleted = query(&state.db, move |pool| db::delete_download_client(&pool, &id)).await?;
    if !deleted {
        return Err(lerr(loc, StatusCode::NOT_FOUND, "error.clientNotFound"));
    }
    Ok(Json(json!({ "ok": true })).into_response())
}

/// `POST /api/admin/download-clients/:id/test` live reachability probe.
pub async fn test(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    let result = blocking(move || {
        let conn = state.db.get()?;
        let Some(row) = db::get_download_client(&conn, &id)? else {
            return Ok(None);
        };
        drop(conn);
        let outcome = state.downloads.engine_for(&row).and_then(|engine| engine.test());
        Ok(Some(match outcome {
            Ok(version) => ClientTestResult { ok: true, version: Some(version), error: None },
            Err(e) => ClientTestResult { ok: false, version: None, error: Some(format!("{e:#}")) },
        }))
    })
    .await?;
    match result {
        Some(r) => Ok(Json(r).into_response()),
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.clientNotFound")),
    }
}
