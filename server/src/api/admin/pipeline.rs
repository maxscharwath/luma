//! Per-element pipeline admin API (`/api/admin/pipeline/*`): per-stage health
//! counts for the dashboard, the failed-task drill-down, and the granular
//! controls (run a stage now, cancel it, retry failures, reprocess everything).
//!
//! A stage's drain is a normal background job (`pipeline.<stage>`), so run/cancel
//! reuse the job trigger and the run history/logs come from the existing
//! `/api/admin/jobs/:key` surface. This module adds only what the ledger knows:
//! aggregate counts + per-subject failures + retry.
//!
//! Reads need any admin capability; mutations need `settings.manage`.
//!
//! The handlers are grouped per concern in the submodules below and re-exported
//! here so the module's public surface (`crate::api::admin::pipeline::*`) is
//! unchanged: [`health`] (stage-health + elements list + failed drill-down),
//! [`processing`] (per item/show treatments) and [`actions`] (stage/element
//! run/cancel/retry/reprocess). The stage-key helpers shared by several of them
//! live in this root.

use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;

use crate::api::error::json_error;
use crate::services::pipeline::STAGE_KEYS;
use crate::state::SharedState;

mod actions;
mod health;
mod processing;

pub use actions::{
    cancel_stage, reprocess_stage, reprocess_subject, retry_element_stage, retry_stage, retry_task,
    run_stage, set_pause,
};
pub use health::{failed_tasks, list_elements, list_pipeline};
pub use processing::{item_processing, show_processing};

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/pipeline", get(list_pipeline))
        .route("/pipeline/pause", post(set_pause))
        .route("/pipeline/{stage}/failed", get(failed_tasks))
        .route("/pipeline/{stage}/run", post(run_stage))
        .route("/pipeline/{stage}/cancel", post(cancel_stage))
        .route("/pipeline/{stage}/retry", post(retry_stage))
        .route("/pipeline/{stage}/reprocess", post(reprocess_stage))
        .route("/pipeline/{stage}/task/retry", post(retry_task))
        .route("/pipeline/subject/reprocess", post(reprocess_subject))
        .route("/pipeline/item/{id}", get(item_processing))
        .route("/pipeline/show/{id}", get(show_processing))
        .route("/pipeline/elements", get(list_elements))
        .route("/pipeline/element/retry", post(retry_element_stage))
}

/// Resolve a short stage key (`"markers"`) to `(short, full job key, subject_kind)`.
fn resolve(short: &str) -> Option<(&'static str, &'static str, &'static str)> {
    STAGE_KEYS.iter().copied().find(|(s, _, _)| *s == short)
}

/// Trigger a stage's drain now (so a retry/requeue runs promptly, not only on the
/// next schedule). Best-effort: a 409 (already running) is fine, it will absorb
/// the new pending tasks.
fn kick(state: &SharedState, full_key: &str) {
    if let Some(job) = state.jobs.resolve(full_key) {
        let _ = state.jobs.trigger(state.clone(), job, "manual");
    }
}

fn unknown_stage() -> Response {
    json_error(StatusCode::NOT_FOUND, "unknown pipeline stage")
}
