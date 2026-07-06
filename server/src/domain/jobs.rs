//! Wire types for the background-job admin API (`/api/admin/jobs`). Pure data
//! (serde + ts-rs); the registry/scheduler that produces them lives in
//! [`crate::services::jobs`], persistence in [`crate::db`].
//!
//! Timestamps are epoch **milliseconds**. `name`/`description` are i18n keys the
//! clients resolve against the shared catalogs.

use serde::Serialize;
use ts_rs::TS;

// A job's identity is its dotted key, declared per-job in its `SPEC` and modelled
// as `crate::services::jobs::JobKey` (a pure internal type, not a wire type). On
// the wire the admin API speaks that key as a plain string (see [`JobInfo::key`]).

/// UI grouping bucket for a job. Serializes lowercase (`"maintenance"`), which the
/// clients turn into the `jobs.cat.{category}` i18n key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Category {
    Maintenance,
    Library,
    Recommendations,
    /// Per-element processing pipeline stages (probe, metadata, storyboard,
    /// markers, embed). Surfaced in the dedicated admin Pipeline dashboard rather
    /// than the general Tâches list. See [`crate::services::pipeline`].
    Pipeline,
    /// The acquisition stack: request availability matching, wanted-list
    /// indexer searches, download import. See `crate::services::requests` /
    /// `crate::services::acquisition`.
    Acquisition,
}

/// One recorded execution of a job.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobRun {
    pub id: String,
    pub job_key: String,
    /// What started it: `"manual"` | `"schedule"`.
    pub trigger: String,
    /// `"running"` | `"success"` | `"failed"` | `"cancelled"`.
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// Wall-clock duration once finished (ms).
    pub duration_ms: Option<i64>,
    pub progress_done: Option<i64>,
    pub progress_total: Option<i64>,
    /// Failure message when `status == "failed"`.
    pub error: Option<String>,
}

/// One persisted log line of a run.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct JobLog {
    pub ts: i64,
    /// `"info" | "warn" | "error"`.
    pub level: String,
    pub message: String,
}

/// A job's definition + current state, as listed in the admin console.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobInfo {
    /// Stable dotted key (`"library.scan"`) this job's identity on the wire: the
    /// DB key, the `/api/admin/jobs/:key` URL segment, and the i18n base
    /// (`jobs.{key}.name` / `.desc`). Declared in the job's `SPEC`.
    pub key: String,
    /// i18n key for the display name (`jobs.{key}.name`).
    pub name: String,
    /// i18n key for the description (`jobs.{key}.desc`).
    pub description: String,
    /// UI grouping bucket.
    pub category: Category,
    /// Effective cron schedule, or `null` for manual-only.
    pub schedule: Option<String>,
    /// The built-in default schedule (so the UI can offer "reset to default").
    pub default_schedule: Option<String>,
    /// Whether the schedule/enabled flag was overridden from the default.
    pub customized: bool,
    pub enabled: bool,
    pub running: bool,
    /// Run id of the in-flight run, when `running`.
    pub run_id: Option<String>,
    pub progress_done: Option<i64>,
    pub progress_total: Option<i64>,
    /// Next scheduled fire (ms), or `null` when manual/disabled.
    pub next_run_at: Option<i64>,
    pub last_run: Option<JobRun>,
}

/// `GET /api/admin/jobs`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct JobsView {
    pub jobs: Vec<JobInfo>,
}

/// `GET /api/admin/jobs/:key` a job plus its recent run history.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct JobDetail {
    pub info: JobInfo,
    pub runs: Vec<JobRun>,
}

// Job keys are declared per-job in their `SPEC` and guarded for uniqueness at
// compile time in `crate::services::jobs::builtins`; there is no key table to test
// here anymore.
