//! Stage-health + catalog-elements views for the pipeline admin panel: the
//! per-stage aggregate counts (`GET /pipeline`), the filtered/paginated elements
//! list (`GET /pipeline/elements`), and the failed-task drill-down with resolved
//! titles (`GET /pipeline/:stage/failed`). Reads need any admin capability.

use axum::extract::{Path as AxPath, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::admin::require_any_admin;
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::model::{PipelineTaskView, PipelineView};
use crate::services::pipeline::elements::Filter;
use crate::services::pipeline::STAGE_KEYS;
use crate::state::SharedState;

use super::{resolve, unknown_stage};

/// Max failed tasks returned for one stage's drill-down.
const FAILED_LIMIT: usize = 200;

/// Query for `GET /api/admin/pipeline/elements`.
#[derive(Deserialize)]
pub struct ElementsQuery {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub q: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /api/admin/pipeline/elements` → the catalog as a filtered, paginated list
/// of elements with per-treatment status + full-catalog counts.
pub async fn list_elements(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(qy): Query<ElementsQuery>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let filter = Filter {
        status: qy.status.unwrap_or_else(|| "attention".to_string()),
        kind: qy.kind.unwrap_or_else(|| "all".to_string()),
        query: qy.q.unwrap_or_default(),
        page: qy.page.unwrap_or(0),
        limit: qy.limit.unwrap_or(30),
    };
    let out = blocking(move || crate::services::pipeline::elements::list(&state, &filter)).await?;
    Ok(Json(out).into_response())
}

/// `GET /api/admin/pipeline` → every stage's health counts, in DAG order.
pub async fn list_pipeline(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let paused = state.jobs.pipeline_paused();
    let stages = blocking(move || {
        let mut out = Vec::with_capacity(STAGE_KEYS.len());
        for (short, key, kind) in STAGE_KEYS.iter().copied() {
            out.push(crate::db::pipeline::stage_stat(&state.db, short, key, kind)?);
        }
        Ok(out)
    })
    .await?;
    Ok(Json(PipelineView { stages, paused }).into_response())
}

/// `GET /api/admin/pipeline/:stage/failed` → the stage's failed tasks, newest
/// first, with a best-effort catalog title resolved for each subject.
pub async fn failed_tasks(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(stage): AxPath<String>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let (short, ..) = resolve(&stage).ok_or_else(unknown_stage)?;
    let tasks = blocking(move || {
        let mut tasks = crate::db::pipeline::failed_tasks(&state.db, short, FAILED_LIMIT)?;
        resolve_titles(&state.db, &mut tasks)?;
        Ok(tasks)
    })
    .await?;
    Ok(Json(json!({ "tasks": tasks })).into_response())
}

/// Best-effort human titles for a batch of failed tasks, resolved in TWO queries
/// (items + shows) rather than a per-task catalog lookup (never the heavy
/// `get_show`). Items resolve to their title; shows to the show title; seasons
/// (`"{show}#{n}"`) to the show title + `S{n}`; anything unresolved falls back to
/// the raw id.
fn resolve_titles(pool: &crate::db::Pool, tasks: &mut [PipelineTaskView]) -> anyhow::Result<()> {
    // Candidate ids: item/file/show subjects use their id directly; season
    // subjects ("{show}#{n}") contribute their show id.
    let mut ids: Vec<String> = Vec::with_capacity(tasks.len());
    for task in tasks.iter() {
        match task.subject_kind.as_str() {
            "season" => {
                if let Some((show_id, _)) = task.subject_id.rsplit_once('#') {
                    ids.push(show_id.to_string());
                }
            }
            _ => ids.push(task.subject_id.clone()),
        }
    }
    let items = crate::db::pipeline::item_titles(pool, &ids)?;
    let shows = crate::db::pipeline::show_titles(pool, &ids)?;
    for task in tasks.iter_mut() {
        task.title = match task.subject_kind.as_str() {
            "season" => match task.subject_id.rsplit_once('#') {
                Some((show_id, num)) => shows
                    .get(show_id)
                    .map(|title| format!("{title} S{num}"))
                    .unwrap_or_else(|| task.subject_id.clone()),
                None => task.subject_id.clone(),
            },
            // item / file: a movie/episode item, or a show (metadata/embed
            // subjects are item-kind but their id may be a show).
            _ => items
                .get(&task.subject_id)
                .or_else(|| shows.get(&task.subject_id))
                .cloned()
                .unwrap_or_else(|| task.subject_id.clone()),
        };
    }
    Ok(())
}
