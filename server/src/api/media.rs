//! Catalogue browse + detail endpoints (libraries / items / movies / shows) plus
//! the server status / scan / logs handlers. All responses are JSON unless noted.
//! DB work runs on `spawn_blocking` threads via [`crate::api::util`].

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tracing::info;

use crate::api::error::json_error;
use crate::api::util::{blocking, query};
use crate::db;
use crate::infra::events::ServerEvent;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct LibraryQuery {
    pub library: Option<String>,
}

/// `GET /api/health`
pub async fn health(State(state): State<SharedState>) -> Result<Response, Response> {
    let (libraries, items, shows) = query(&state.db, move |pool| db::counts(&pool)).await?;
    Ok(Json(super::dto::Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        ffprobe: state.ffprobe_available,
        libraries,
        items,
        shows,
    })
    .into_response())
}

/// `GET /api/libraries` → `Library[]`
pub async fn list_libraries(State(state): State<SharedState>) -> Result<Response, Response> {
    let libs = query(&state.db, move |pool| db::list_libraries(&pool)).await?;
    Ok(Json(libs).into_response())
}

/// `GET /api/items` (optional `?library=`) → all playable items (movies + episodes).
pub async fn list_items(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let items = query(&state.db, move |pool| db::list_items(&pool, q.library.as_deref())).await?;
    Ok(Json(items).into_response())
}

/// `GET /api/movies` (optional `?library=`) → `MediaItem[]` (movies only).
pub async fn list_movies(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let items = query(&state.db, move |pool| db::list_movies(&pool, q.library.as_deref())).await?;
    Ok(Json(items).into_response())
}

/// `GET /api/shows` (optional `?library=`) → `Show[]`
pub async fn list_shows(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let shows = query(&state.db, move |pool| db::list_shows(&pool, q.library.as_deref())).await?;
    Ok(Json(shows).into_response())
}

/// `GET /api/shows/:id` → `{ show, seasons[] }`
pub async fn get_show(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let detail = query(&state.db, move |pool| db::get_show(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "show not found"))?;
    Ok(Json(detail).into_response())
}

/// `GET /api/items/:id` → `MediaItem`
pub async fn get_item(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    Ok(Json(item).into_response())
}

/// `POST /api/scan` → rescan all dirs, reseeding demo content if empty.
pub async fn rescan(State(state): State<SharedState>) -> Result<Response, Response> {
    let defs = crate::services::settings::library_defs(&state.settings, &state.config);

    state.events.publish(ServerEvent::ScanStarted);
    crate::services::activity::scan_started(&state.activity);

    // Phase 1 (fast): walk + stat only, diff-synced (preserves metadata + probed
    // data via the mtime cache). Phase 2 probing is spawned afterwards.
    let data = query(&state.db, move |pool| {
        let mut data = crate::services::scan::scan_all(&defs);
        if data.items.is_empty() {
            info!("scan yielded no items; seeding demo content");
            data = crate::services::demo::demo_data();
        }
        db::sync_all(&pool, &data.libraries, &data.shows, &data.items, &data.mtimes)?;
        Ok(data)
    })
    .await?;

    let (libraries, shows, items) = (data.libraries.len(), data.shows.len(), data.items.len());
    crate::services::activity::scan_completed(&state.activity, libraries, shows, items, crate::services::scan::now_iso8601());
    // Tell live clients the catalog changed, then run phase-2 probing and
    // re-resolve TMDB art in the background (both emit live updates).
    state.events.publish(ServerEvent::ScanCompleted { items, shows, libraries });
    state.events.publish(ServerEvent::LibraryUpdated);
    crate::infra::probe::spawn_probe_pass(
        state.db.clone(),
        state.ffprobe_available,
        state.events.clone(),
        state.activity.clone(),
    );
    crate::services::search::spawn_reindex(state.clone());
    crate::services::enrich::maybe_spawn(&state, &data.items, &data.shows);
    Ok(Json(super::dto::ScanResult { scanned: items, libraries, shows }).into_response())
}

/// `GET /api/status` → live scan/enrichment snapshot.
pub async fn status(State(state): State<SharedState>) -> Response {
    let snap = crate::services::activity::snapshot(&state.activity);
    Json(snap).into_response()
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Number of trailing lines to return (default 200, max 5000).
    pub tail: Option<usize>,
}

/// `GET /api/logs?tail=N` → the last N lines of the current server log, as
/// `text/plain`. Reads the most-recently-modified file under `<data>/logs/`.
pub async fn logs(
    State(state): State<SharedState>,
    Query(q): Query<LogsQuery>,
) -> Result<Response, Response> {
    let dir = state.config.logs_dir();
    let tail = q.tail.unwrap_or(200).min(5000);

    let text = blocking(move || Ok(read_log_tail(&dir, tail))).await?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

/// Read the last `tail` lines of the newest log file in `dir` (empty if none).
fn read_log_tail(dir: &std::path::Path, tail: usize) -> String {
    let newest = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((mtime, p))
        })
        .max_by_key(|(mtime, _)| *mtime)
        .map(|(_, p)| p);

    let Some(path) = newest else {
        return String::new();
    };
    let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        // Keep the always-200, text/plain contract (an empty body for the
        // legitimately-empty-log case) but don't swallow a real read failure.
        tracing::warn!(path = %path.display(), error = %e, "failed to read log file for /api/logs");
        String::new()
    });
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(tail);
    lines[start..].join("\n")
}
