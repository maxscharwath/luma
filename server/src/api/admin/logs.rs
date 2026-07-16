//! Server-log admin API (`/api/admin/logs`): the in-memory ring of recent log
//! lines (core tracing events + every module sidecar's piped output), backing
//! the admin "Journaux" page. Read-only; needs any admin capability.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::api::extract::AuthUser;
use crate::infra::logbuf::LOG_BUFFER;
use crate::state::SharedState;

/// Max lines one request returns (the buffer itself holds more).
const MAX_LIMIT: usize = 2000;
const DEFAULT_LIMIT: usize = 500;

pub fn routes() -> Router<SharedState> {
    Router::new().route("/logs", get(list_logs))
}

#[derive(Deserialize)]
struct LogsQuery {
    /// Minimum severity (`warn` shows warn + error). Omit for everything.
    level: Option<String>,
    /// `core` or a module id. Omit for everything.
    source: Option<String>,
    /// Case-insensitive substring over message/target/source.
    q: Option<String>,
    limit: Option<usize>,
}

/// `GET /api/admin/logs` → newest-last recent log lines + the distinct sources
/// present (for the filter dropdown).
async fn list_logs(
    State(_state): State<SharedState>,
    AuthUser(user): AuthUser,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let entries = LOG_BUFFER.snapshot(
        limit,
        query.level.as_deref().filter(|s| !s.is_empty()),
        query.source.as_deref().filter(|s| !s.is_empty()),
        query.q.as_deref(),
    );
    Ok(Json(json!({ "entries": entries, "sources": LOG_BUFFER.sources() })).into_response())
}
