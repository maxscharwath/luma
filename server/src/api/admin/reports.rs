//! `/api/admin/reports` the problem-report triage queue. Reads + writes are gated
//! on `reports.manage`. Users file reports via `POST /api/reports`
//! (`crate::api::reports`); admins list, resolve / dismiss / reopen and delete
//! them here. Each write publishes a `report.updated` event so the console's
//! queue refreshes live.

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::error::lerr;
use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::db;
use crate::infra::events::ServerEvent;
use crate::model::{
    Permission, Report, ReportCategory, ReportCounts, ReportStatus, ReportSubjectKind, ReportsView,
    User,
};
use crate::services::jobs::now_ms;
use crate::state::SharedState;

/// Admin report triage. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/{id}", axum::routing::delete(remove))
        .route("/reports/{id}/resolve", post(resolve))
        .route("/reports/{id}/dismiss", post(dismiss))
        .route("/reports/{id}/reopen", post(reopen))
}

/// Filters for the queue view. Each is an exact match on the parsed enum, except
/// `q` which is a case-insensitive substring over the subject title / id and the
/// reporter's name.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

fn counts_of(list: &[Report]) -> ReportCounts {
    let mut c = ReportCounts::default();
    for r in list {
        c.total += 1;
        match r.status {
            ReportStatus::Open => c.open += 1,
            ReportStatus::Resolved => c.resolved += 1,
            ReportStatus::Dismissed => c.dismissed += 1,
        }
    }
    c
}

/// Apply the query filters to the full list. Unparseable enum filters match
/// nothing (a client typo yields an empty result, not the unfiltered list).
fn filter_reports(all: Vec<Report>, params: &ListParams) -> Vec<Report> {
    let status = params.status.as_deref().map(|s| ReportStatus::parse(s));
    let category = params.category.as_deref().map(|s| ReportCategory::parse(s));
    let kind = params.kind.as_deref().map(|s| ReportSubjectKind::parse(s));
    let needle = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
    all.into_iter()
        .filter(|r| status.is_none_or(|s| s == Some(r.status)))
        .filter(|r| category.is_none_or(|c| c == Some(r.category)))
        .filter(|r| kind.is_none_or(|k| k == Some(r.subject_kind)))
        .filter(|r| {
            needle.as_deref().is_none_or(|q| {
                r.subject_title.to_lowercase().contains(q)
                    || r.subject_id.to_lowercase().contains(q)
                    || r.reported_by_name.as_deref().is_some_and(|n| n.to_lowercase().contains(q))
            })
        })
        .collect()
}

/// `GET /api/admin/reports` the triage queue (filtered list + status tallies over
/// the whole queue for the filter chips).
pub async fn list(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Response, Response> {
    super::require(&user, Permission::ReportsManage)?;
    let view = query(&state.db, move |pool| {
        let conn = pool.get()?;
        let all = db::list_reports(&conn, None)?;
        let counts = counts_of(&all);
        let reports = filter_reports(all, &params);
        Ok(ReportsView { reports, counts })
    })
    .await?;
    Ok(Json(view).into_response())
}

/// Shared transition: set `status`, publish, return the updated report. A missing
/// id is a localized 404.
async fn transition(
    state: SharedState,
    user: &User,
    id: String,
    status: ReportStatus,
) -> Result<Response, Response> {
    super::require(user, Permission::ReportsManage)?;
    let loc = super::user_locale(user);
    let actor = user.id.clone();
    let id_for_query = id.clone();
    let updated = query(&state.db, move |pool| {
        if !db::set_report_status(&pool, &id_for_query, status, Some(&actor), now_ms())? {
            return Ok(None);
        }
        let conn = pool.get()?;
        db::get_report(&conn, &id_for_query).map_err(Into::into)
    })
    .await?;
    match updated {
        Some(report) => {
            state.events.publish(ServerEvent::ReportUpdated {
                id: report.id.clone(),
                status: report.status.as_str().into(),
            });
            Ok(Json(report).into_response())
        }
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.reportNotFound")),
    }
}

/// `POST /api/admin/reports/:id/resolve`.
pub async fn resolve(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    transition(state, &user, id, ReportStatus::Resolved).await
}

/// `POST /api/admin/reports/:id/dismiss`.
pub async fn dismiss(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    transition(state, &user, id, ReportStatus::Dismissed).await
}

/// `POST /api/admin/reports/:id/reopen` back to `open` (clears the resolver
/// fields).
pub async fn reopen(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    transition(state, &user, id, ReportStatus::Open).await
}

/// `DELETE /api/admin/reports/:id`.
pub async fn remove(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::ReportsManage)?;
    let loc = super::user_locale(&user);
    let id_for_query = id.clone();
    let deleted =
        query(&state.db, move |pool| db::delete_report(&pool, &id_for_query).map_err(Into::into))
            .await?;
    if !deleted {
        return Err(lerr(loc, StatusCode::NOT_FOUND, "error.reportNotFound"));
    }
    state.events.publish(ServerEvent::ReportUpdated { id, status: "deleted".into() });
    Ok(StatusCode::NO_CONTENT.into_response())
}
