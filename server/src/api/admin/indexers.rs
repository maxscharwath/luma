//! `/api/admin/indexers` Torznab (Jackett / Prowlarr) indexer management:
//! CRUD + a `t=caps` test. Gated on `settings.manage`. API keys are write-only
//! (views carry `hasApiKey`, never the secret).

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::api::error::{json_error, lerr};
use crate::api::extract::AuthUser;
use crate::api::util::{blocking, query};
use crate::db::{self, IndexerRow};
use crate::model::{IndexerTestResult, IndexerView, IndexersView, Permission, SaveIndexerBody};
use crate::services::jobs::now_ms;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/indexers", get(list).post(create))
        .route("/indexers/:id", axum::routing::put(update).delete(remove))
        .route("/indexers/:id/test", post(test))
}

fn view_of(row: &IndexerRow) -> IndexerView {
    IndexerView {
        id: row.id.clone(),
        name: row.name.clone(),
        url: row.url.clone(),
        has_api_key: !row.api_key.is_empty(),
        categories: row.categories.clone(),
        enabled: row.enabled,
        priority: row.priority,
        last_ok_at: row.last_ok_at,
        last_error: row.last_error.clone(),
        created_at: row.created_at,
    }
}

/// `GET /api/admin/indexers`
pub async fn list(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let view = query(&state.db, |pool| {
        let conn = pool.get()?;
        let indexers = db::list_indexers(&conn)?.iter().map(view_of).collect();
        Ok(IndexersView { indexers })
    })
    .await?;
    Ok(Json(view).into_response())
}

/// `POST /api/admin/indexers` create (name + url required).
pub async fn create(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveIndexerBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let name = body.name.as_deref().map(str::trim).unwrap_or_default().to_string();
    let url = body.url.as_deref().map(str::trim).unwrap_or_default().to_string();
    if name.is_empty() || url.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "name and url are required"));
    }
    let row = IndexerRow {
        id: crate::services::scan::short_hash(&format!(
            "indexer|{url}|{}",
            crate::services::auth::random_token()
        )),
        name,
        url,
        api_key: body.api_key.unwrap_or_default().trim().to_string(),
        categories: body.categories.unwrap_or_else(|| vec![2000, 5000]),
        enabled: body.enabled.unwrap_or(true),
        priority: body.priority.unwrap_or(0),
        last_ok_at: None,
        last_error: None,
        created_at: now_ms(),
    };
    let view = view_of(&row);
    query(&state.db, move |pool| db::insert_indexer(&pool, &row)).await?;
    Ok(Json(view).into_response())
}

/// `PUT /api/admin/indexers/:id` partial update (omitted fields keep values;
/// an omitted / empty apiKey keeps the stored secret).
pub async fn update(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<SaveIndexerBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    let id2 = id.clone();
    let updated = query(&state.db, move |pool| {
        db::update_indexer(
            &pool,
            &id2,
            body.name.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.url.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.categories.as_deref(),
            body.enabled,
            body.priority,
        )
    })
    .await?;
    if !updated {
        return Err(lerr(loc, StatusCode::NOT_FOUND, "error.indexerNotFound"));
    }
    crate::services::acquisition::invalidate_caps(&id);
    let id3 = id.clone();
    let row = query(&state.db, move |pool| {
        let conn = pool.get()?;
        Ok(db::get_indexer(&conn, &id3)?)
    })
    .await?;
    match row {
        Some(r) => Ok(Json(view_of(&r)).into_response()),
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.indexerNotFound")),
    }
}

/// `DELETE /api/admin/indexers/:id`
pub async fn remove(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    let id2 = id.clone();
    let deleted = query(&state.db, move |pool| db::delete_indexer(&pool, &id2)).await?;
    if !deleted {
        return Err(lerr(loc, StatusCode::NOT_FOUND, "error.indexerNotFound"));
    }
    crate::services::acquisition::invalidate_caps(&id);
    Ok(Json(json!({ "ok": true })).into_response())
}

/// `POST /api/admin/indexers/:id/test` a live `t=caps` round-trip, always
/// fresh (drops any cached capability entry first).
pub async fn test(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);
    crate::services::acquisition::invalidate_caps(&id);
    let result = blocking(move || {
        let conn = state.db.get()?;
        let Some(row) = db::get_indexer(&conn, &id)? else {
            return Ok(None);
        };
        drop(conn);
        let started = std::time::Instant::now();
        let outcome = crate::services::acquisition::indexer_caps(&state, &row);
        let latency_ms = started.elapsed().as_millis() as u64;
        Ok(Some(match outcome {
            Ok(caps) => IndexerTestResult {
                ok: true,
                latency_ms,
                server_title: caps.server_title,
                supports_tmdb: caps.search_tmdb || caps.tv_search_tmdb,
                error: None,
            },
            Err(e) => IndexerTestResult {
                ok: false,
                latency_ms,
                server_title: None,
                supports_tmdb: false,
                error: Some(format!("{e:#}")),
            },
        }))
    })
    .await?;
    match result {
        Some(r) => Ok(Json(r).into_response()),
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.indexerNotFound")),
    }
}
