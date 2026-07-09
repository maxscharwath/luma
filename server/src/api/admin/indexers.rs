//! `/api/admin/indexers` indexer management, for both kinds:
//! external Torznab (Jackett / Prowlarr) endpoints and native `builtin`
//! Cardigann definitions. CRUD + a test call, plus the definition catalog
//! (browse + sync). Gated on `settings.manage`. Secrets (api key, per-indexer
//! passwords) are write-only and never leave the server.

use std::collections::HashMap;

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
use crate::model::{
    IndexerDefinitionDetailView, IndexerDefinitionSettingView, IndexerDefinitionView,
    IndexerDefinitionsView, IndexerTestResult, IndexerView, IndexersView, Permission,
    SaveIndexerBody, SyncDefinitionsResult,
};
use crate::services::acquisition::KIND_BUILTIN;
use crate::services::jobs::now_ms;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/indexers", get(list).post(create))
        // Static `definitions` paths are registered before the `:id` dynamic
        // route; matchit prioritizes static segments, so order is not load-bearing.
        .route("/indexers/definitions", get(list_definitions))
        .route("/indexers/definitions/sync", post(sync_definitions))
        .route("/indexers/definitions/:defId", get(definition_detail))
        .route("/indexers/:id", axum::routing::put(update).delete(remove))
        .route("/indexers/:id/test", post(test))
}

/// Names of settings that currently hold a (non-empty) value.
fn configured_settings(row: &IndexerRow) -> Vec<String> {
    let map: HashMap<String, String> = serde_json::from_str(&row.settings).unwrap_or_default();
    let mut names: Vec<String> = map.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| k).collect();
    names.sort();
    names
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
        kind: row.kind.clone(),
        definition_id: row.definition_id.clone(),
        configured_settings: configured_settings(row),
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

/// `POST /api/admin/indexers` create. `kind: "builtin"` creates a native
/// Cardigann indexer (needs `definitionId`); otherwise a Torznab endpoint
/// (needs `name` + `url`).
pub async fn create(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveIndexerBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let kind = body.kind.as_deref().unwrap_or("torznab").to_string();

    let row = if kind == KIND_BUILTIN {
        build_builtin_row(&state, &body).await?
    } else {
        let name = body.name.as_deref().map(str::trim).unwrap_or_default().to_string();
        let url = body.url.as_deref().map(str::trim).unwrap_or_default().to_string();
        if name.is_empty() || url.is_empty() {
            return Err(json_error(StatusCode::BAD_REQUEST, "name and url are required"));
        }
        IndexerRow {
            id: new_indexer_id(&url),
            name,
            url,
            api_key: body.api_key.unwrap_or_default().trim().to_string(),
            categories: body.categories.unwrap_or_else(default_cats),
            enabled: body.enabled.unwrap_or(true),
            priority: body.priority.unwrap_or(0),
            kind: "torznab".to_string(),
            definition_id: None,
            settings: "{}".to_string(),
            last_ok_at: None,
            last_error: None,
            created_at: now_ms(),
        }
    };
    let view = view_of(&row);
    query(&state.db, move |pool| db::insert_indexer(&pool, &row)).await?;
    Ok(Json(view).into_response())
}

/// Assemble a built-in indexer row from the chosen definition + submitted
/// settings.
async fn build_builtin_row(state: &SharedState, body: &SaveIndexerBody) -> Result<IndexerRow, Response> {
    let def_id = body
        .definition_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "definitionId is required for a built-in indexer"))?
        .to_string();

    let state2 = state.clone();
    let def_id2 = def_id.clone();
    let def = blocking(move || {
        crate::services::acquisition::definition_store(&state2)
            .load(&def_id2)
            .map_err(anyhow::Error::from)
    })
    .await
    .map_err(|_| json_error(StatusCode::BAD_REQUEST, "unknown definition (sync the catalog first)"))?;

    // Base link: admin override, else the definition's first candidate.
    let url = body
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| def.links.first().cloned())
        .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "definition has no site link; provide url"))?;

    let name = body.name.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&def.name).to_string();
    let settings = body.settings.clone().unwrap_or_default();

    Ok(IndexerRow {
        id: new_indexer_id(&format!("{def_id}|{url}")),
        name,
        url,
        api_key: String::new(),
        categories: body.categories.clone().unwrap_or_else(default_cats),
        enabled: body.enabled.unwrap_or(true),
        priority: body.priority.unwrap_or(0),
        kind: KIND_BUILTIN.to_string(),
        definition_id: Some(def_id),
        settings: serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string()),
        last_ok_at: None,
        last_error: None,
        created_at: now_ms(),
    })
}

fn new_indexer_id(seed: &str) -> String {
    crate::services::scan::short_hash(&format!(
        "indexer|{seed}|{}",
        crate::services::auth::random_token()
    ))
}

fn default_cats() -> Vec<u32> {
    vec![2000, 5000]
}

/// `PUT /api/admin/indexers/:id` partial update. For built-in rows the
/// `settings` map is merged into the stored one (an omitted/empty password
/// keeps its stored value).
pub async fn update(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<SaveIndexerBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let loc = super::user_locale(&user);

    // Merge settings for built-in rows (needs the current row + definition to
    // preserve secrets).
    let merged_settings = if let Some(incoming) = &body.settings {
        let state2 = state.clone();
        let id2 = id.clone();
        let incoming = incoming.clone();
        Some(blocking(move || merge_settings(&state2, &id2, &incoming)).await?)
    } else {
        None
    };

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
            merged_settings.as_deref(),
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

/// Merge submitted settings into the stored map, preserving a stored password
/// when the incoming value for a `password`-type setting is empty.
fn merge_settings(state: &SharedState, id: &str, incoming: &HashMap<String, String>) -> anyhow::Result<String> {
    let conn = state.db.get()?;
    let row = db::get_indexer(&conn, id)?.ok_or_else(|| anyhow::anyhow!("indexer not found"))?;
    drop(conn);
    let mut current: HashMap<String, String> = serde_json::from_str(&row.settings).unwrap_or_default();

    // Which settings are secret (password), per the definition.
    let secret: std::collections::HashSet<String> = row
        .definition_id
        .as_deref()
        .and_then(|d| crate::services::acquisition::definition_store(state).load(d).ok())
        .map(|def| {
            def.settings.iter().filter(|s| s.kind == "password").map(|s| s.name.clone()).collect()
        })
        .unwrap_or_default();

    for (k, v) in incoming {
        if v.is_empty() && secret.contains(k) {
            continue; // keep the stored secret
        }
        current.insert(k.clone(), v.clone());
    }
    Ok(serde_json::to_string(&current)?)
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

/// `POST /api/admin/indexers/:id/test`. Torznab: a live `t=caps` round-trip.
/// Built-in: derive caps from the definition and verify login/reachability.
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
        let caps = crate::services::acquisition::any_indexer_caps(&state, &row);
        let reachable = if row.kind == KIND_BUILTIN {
            // Verify the session (drives a login for private trackers).
            crate::services::acquisition::build_builtin_session(&state, &row)
                .and_then(|s| s.test())
                .map(|_| ())
        } else {
            caps.as_ref().map(|_| ()).map_err(|e| anyhow::anyhow!("{e:#}"))
        };
        let latency_ms = started.elapsed().as_millis() as u64;
        Ok(Some(match (reachable, caps) {
            (Ok(()), Ok(caps)) => IndexerTestResult {
                ok: true,
                latency_ms,
                server_title: caps.server_title,
                supports_tmdb: caps.search_tmdb || caps.tv_search_tmdb,
                error: None,
            },
            (Err(e), _) | (_, Err(e)) => IndexerTestResult {
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

// ----- definition catalog ---------------------------------------------------------

/// `GET /api/admin/indexers/definitions` the browsable Cardigann catalog.
pub async fn list_definitions(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let view = blocking(move || {
        let store = crate::services::acquisition::definition_store(&state);
        let synced = store.is_populated();
        let definitions = store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|m| IndexerDefinitionView {
                id: m.id,
                name: m.name,
                kind: m.kind,
                description: m.description,
                links: m.links,
            })
            .collect();
        anyhow::Ok(IndexerDefinitionsView { definitions, synced })
    })
    .await?;
    Ok(Json(view).into_response())
}

/// `GET /api/admin/indexers/definitions/:defId` the settings schema for the
/// add form.
pub async fn definition_detail(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(def_id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let detail = blocking(move || {
        crate::services::acquisition::definition_store(&state).load(&def_id).map(|def| {
            IndexerDefinitionDetailView {
                id: def.id.clone(),
                name: def.name.clone(),
                kind: def.kind.clone(),
                description: def.description.clone(),
                links: def.links.clone(),
                settings: def
                    .settings
                    .iter()
                    .map(|s| IndexerDefinitionSettingView {
                        name: s.name.clone(),
                        kind: s.kind.clone(),
                        label: s.label.clone().unwrap_or_else(|| s.name.clone()),
                        default: s.default.clone(),
                        options: s.options.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    })
                    .collect(),
            }
        })
    })
    .await;
    match detail {
        Ok(d) => Ok(Json(d).into_response()),
        Err(_) => Err(json_error(StatusCode::NOT_FOUND, "unknown definition")),
    }
}

/// `POST /api/admin/indexers/definitions/sync` fetch the current definition set.
pub async fn sync_definitions(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Not via `blocking`: we want the real network/extract error message to reach
    // the admin, and `blocking` collapses errors to a generic 500.
    let report = tokio::task::spawn_blocking(move || {
        crate::services::acquisition::definition_store(&state).sync()
    })
    .await
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "sync task failed"))?
    .map_err(|e| json_error(StatusCode::BAD_GATEWAY, &format!("sync failed: {e:#}")))?;
    Ok(Json(SyncDefinitionsResult { count: report.count, version: report.version }).into_response())
}
