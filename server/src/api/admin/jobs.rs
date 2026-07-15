//! Background-job admin API (`/api/admin/jobs/*`): list jobs with their
//! schedule/last-run/next-fire, drill into a job's run history + logs, trigger
//! or cancel a run, and edit a job's cron schedule / enabled flag.
//!
//! Reads need any admin capability; mutations (run/cancel/edit) need
//! `settings.manage`.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::api::error::json_error;
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::model::{JobsView, Permission};
use crate::services::jobs::{Cron, TriggerError};
use crate::state::SharedState;
use axum::routing::{get, post};
use axum::Router;

/// Background-job scheduler controls. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/jobs", get(list_jobs))
        .route("/job-runs/{run_id}/logs", get(run_logs))
        .route("/jobs/{key}", get(job_detail).patch(update_job))
        .route("/jobs/{key}/run", post(run_job))
        .route("/jobs/{key}/cancel", post(cancel_job))
}

/// Max log lines returned for one run.
const LOG_LIMIT: usize = 500;

/// `GET /api/admin/jobs` → every job with its schedule + latest run + next fire.
pub async fn list_jobs(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let st = state.clone();
    let jobs = blocking(move || Ok(st.jobs.list(&st))).await?;
    Ok(Json(JobsView { jobs }).into_response())
}

/// `GET /api/admin/jobs/:key` → one job plus its recent run history.
pub async fn job_detail(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(key): AxPath<String>,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let st = state.clone();
    let detail = blocking(move || {
        Ok(st.jobs.resolve(&key).and_then(|job| st.jobs.detail(&st, job)))
    })
    .await?;
    match detail {
        Some(d) => Ok(Json(d).into_response()),
        None => Err(json_error(StatusCode::NOT_FOUND, "job not found")),
    }
}

/// `POST /api/admin/jobs/:key/run` → trigger the job now (manual). Returns the
/// new run id, or 409 if it's already running.
pub async fn run_job(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(key): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let job = state.jobs.resolve(&key).ok_or_else(|| json_error(StatusCode::NOT_FOUND, "job not found"))?;
    match state.jobs.trigger(state.clone(), job, "manual") {
        Ok(run_id) => Ok(Json(json!({ "runId": run_id })).into_response()),
        Err(TriggerError::Unknown) => Err(json_error(StatusCode::NOT_FOUND, "job not found")),
        Err(TriggerError::AlreadyRunning) => {
            Err(json_error(StatusCode::CONFLICT, "job already running"))
        }
    }
}

/// `POST /api/admin/jobs/:key/cancel` → request cancellation of the current run.
pub async fn cancel_job(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(key): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let cancelled = state.jobs.resolve(&key).is_some_and(|job| state.jobs.cancel(job));
    Ok(Json(json!({ "cancelled": cancelled })).into_response())
}

/// PATCH body tri-state `schedule` (absent = unchanged, null = clear to
/// manual-only, string = set cron) plus an optional `enabled` flag.
#[derive(Deserialize)]
pub struct UpdateJobBody {
    #[serde(default, deserialize_with = "double_option")]
    pub schedule: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// `PATCH /api/admin/jobs/:key` → update schedule and/or enabled flag.
pub async fn update_job(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(key): AxPath<String>,
    Json(body): Json<UpdateJobBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    // Reject a bad cron before touching the DB, with a precise 400.
    if let Some(Some(expr)) = &body.schedule {
        if !Cron::is_valid(expr) {
            return Err(json_error(StatusCode::BAD_REQUEST, "invalid cron expression"));
        }
    }
    let job = state.jobs.resolve(&key).ok_or_else(|| json_error(StatusCode::NOT_FOUND, "job not found"))?;
    let st = state.clone();
    let (schedule, enabled) = (body.schedule, body.enabled);
    blocking(move || st.jobs.update_schedule(&st.db, job, schedule, enabled)).await?;
    Ok(Json(json!({ "ok": true })).into_response())
}

/// `GET /api/admin/jobs/runs/:runId/logs` → the run's log lines (chronological).
pub async fn run_logs(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(run_id): AxPath<String>,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let logs = blocking(move || crate::db::list_job_logs(&state.db, &run_id, LOG_LIMIT)).await?;
    Ok(Json(json!({ "logs": logs })).into_response())
}

/// Distinguish an absent JSON field from an explicit `null` (→ `Some(None)`),
/// so PATCH can clear a schedule vs. leave it unchanged.
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(de).map(Some)
}
