//! Stage- and element-level mutations for the pipeline admin panel: run a stage
//! now, cancel its running drain, retry its failures, reprocess it wholesale,
//! retry a single failed task, retry one stage for one element, and force one
//! element through the whole pipeline. All mutations need `settings.manage`.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::admin::require;
use crate::api::error::json_error;
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::model::Permission;
use crate::services::jobs::TriggerError;
use crate::state::SharedState;

use super::{kick, resolve, unknown_stage};

/// Body for `POST /api/admin/pipeline/element/retry`.
#[derive(Deserialize)]
pub struct RetryStageBody {
    /// `"item"` (movie/episode) or `"show"`.
    pub kind: String,
    pub id: String,
    /// Short stage key: `probe|metadata|storyboard|subtitles|markers|embed`.
    pub stage: String,
}

/// `POST /api/admin/pipeline/element/retry` → re-run ONE stage for ONE element
/// (the drawer's "retry this stage" action).
pub async fn retry_element_stage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<RetryStageBody>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let st = state.clone();
    blocking(move || crate::services::pipeline::reprocess::stage_for(&st, &body.kind, &body.id, &body.stage))
        .await?;
    Ok(Json(json!({ "ok": true })).into_response())
}

/// `POST /api/admin/pipeline/:stage/run` → trigger the stage's drain now.
pub async fn run_stage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let (_, key, _) = resolve(&stage).ok_or_else(unknown_stage)?;
    let job = state.jobs.resolve(key).ok_or_else(unknown_stage)?;
    match state.jobs.trigger(state.clone(), job, "manual") {
        Ok(run_id) => Ok(Json(json!({ "runId": run_id })).into_response()),
        Err(TriggerError::Unknown) => Err(unknown_stage()),
        Err(TriggerError::AlreadyRunning) => {
            Err(json_error(StatusCode::CONFLICT, "stage already running"))
        }
    }
}

/// `POST /api/admin/pipeline/:stage/cancel` → cancel the stage's running drain.
pub async fn cancel_stage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let (_, key, _) = resolve(&stage).ok_or_else(unknown_stage)?;
    let cancelled = state.jobs.resolve(key).is_some_and(|job| state.jobs.cancel(job));
    Ok(Json(json!({ "cancelled": cancelled })).into_response())
}

/// Body for `POST /api/admin/pipeline/pause`.
#[derive(Deserialize)]
pub struct PauseBody {
    pub paused: bool,
}

/// `POST /api/admin/pipeline/pause` → hold (or release) all pipeline stages. A
/// held pipeline parks every drain within a poll tick (leftover work stays
/// `pending`); releasing resumes the parked drains where they left off. Persisted
/// so the hold survives a restart. This is the manual "free the box now" switch,
/// on top of the automatic playback-priority yield.
pub async fn set_pause(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<PauseBody>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    state.jobs.set_pipeline_paused(body.paused);
    // Persist so a reboot keeps the operator's choice (seeded in AppState::new).
    let mut patch = std::collections::BTreeMap::new();
    patch.insert("pipelinePaused".to_string(), json!(body.paused));
    state.settings.set_patch(&state.db, patch);
    Ok(Json(json!({ "paused": body.paused })).into_response())
}

/// `POST /api/admin/pipeline/:stage/retry` → reset this stage's failed tasks to
/// pending (they run on the next drain / an immediate `run`).
pub async fn retry_stage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let (short, key, _) = resolve(&stage).ok_or_else(unknown_stage)?;
    let st = state.clone();
    let n = blocking(move || crate::db::pipeline::retry(&st.db, short, None)).await?;
    kick(&state, key);
    Ok(Json(json!({ "requeued": n })).into_response())
}

/// `POST /api/admin/pipeline/:stage/reprocess` → force a full re-run of the stage
/// (every non-running task back to pending). The per-artifact skip still applies
/// (a cached storyboard is a no-op), so this re-invokes the stage over all
/// subjects rather than deleting artifacts.
pub async fn reprocess_stage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let (short, key, _) = resolve(&stage).ok_or_else(unknown_stage)?;
    let st = state.clone();
    let n = blocking(move || crate::db::pipeline::reprocess(&st.db, short)).await?;
    kick(&state, key);
    Ok(Json(json!({ "requeued": n })).into_response())
}

#[derive(Deserialize)]
pub struct RetryTaskBody {
    #[serde(rename = "subjectId")]
    pub subject_id: String,
}

/// `POST /api/admin/pipeline/:stage/task/retry` → reset one failed task to
/// pending (subject id in the body, since season ids contain `#`).
pub async fn retry_task(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
    Json(body): Json<RetryTaskBody>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let (short, key, _) = resolve(&stage).ok_or_else(unknown_stage)?;
    let st = state.clone();
    let n = blocking(move || crate::db::pipeline::retry(&st.db, short, Some(&body.subject_id)))
        .await?;
    kick(&state, key);
    Ok(Json(json!({ "requeued": n })).into_response())
}

/// PATCH body: which element to reprocess.
#[derive(Deserialize)]
pub struct ReprocessSubjectBody {
    /// `"item"` (a movie or single episode) or `"show"`.
    pub kind: String,
    pub id: String,
}

/// `POST /api/admin/pipeline/subject/reprocess` → force one element through the
/// whole pipeline now: clear its artifacts, requeue its tasks HIGH, kick the
/// stages. Returns how many tasks were queued and which stages ran.
pub async fn reprocess_subject(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ReprocessSubjectBody>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let st = state.clone();
    let outcome =
        blocking(move || crate::services::pipeline::reprocess::reprocess(&st, &body.kind, &body.id))
            .await?;
    Ok(Json(json!({ "subjects": outcome.subjects, "stages": outcome.stages })).into_response())
}
