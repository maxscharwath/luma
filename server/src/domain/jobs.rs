//! Wire types for the background-job admin API (`/api/admin/jobs`). Pure data
//! (serde + ts-rs); the registry/scheduler that produces them lives in
//! [`crate::services::jobs`], persistence in [`crate::db`].
//!
//! Timestamps are epoch **milliseconds**. `name`/`description` are i18n keys the
//! clients resolve against the shared catalogs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Typed identity of a built-in job the single source of truth for "what jobs
/// exist". Used everywhere instead of magic strings (so references are
/// compiler-checked + autocompleted), and exported to the clients so the UI keys
/// its job actions to this union rather than free strings. Serializes as the
/// stable dotted key (`"library.scan"`), which is also the DB key, the URL
/// segment, and the i18n base (`jobs.{key}.name` / `.desc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum JobId {
    #[serde(rename = "cache.cleanup")]
    CacheCleanup,
    #[serde(rename = "recommendations.refresh")]
    RecommendationsRefresh,
    #[serde(rename = "recommendations.reembed")]
    RecommendationsReembed,
    #[serde(rename = "sections.personalize")]
    SectionsPersonalize,
    #[serde(rename = "sections.curate")]
    SectionsCurate,
    #[serde(rename = "library.scan")]
    LibraryScan,
    #[serde(rename = "metadata.enrich")]
    MetadataEnrich,
    #[serde(rename = "search.reindex")]
    SearchReindex,
    #[serde(rename = "markers.detect")]
    MarkersDetect,
}

impl JobId {
    /// Every variant, in admin-listing order. Adding a job? add it here too.
    pub const ALL: [JobId; 9] = [
        JobId::CacheCleanup,
        JobId::RecommendationsRefresh,
        JobId::RecommendationsReembed,
        JobId::SectionsPersonalize,
        JobId::SectionsCurate,
        JobId::LibraryScan,
        JobId::MetadataEnrich,
        JobId::SearchReindex,
        JobId::MarkersDetect,
    ];

    /// The stable string key (DB / URL / i18n base). Must match the `serde`
    /// rename above guarded by a test.
    pub const fn key(self) -> &'static str {
        match self {
            JobId::CacheCleanup => "cache.cleanup",
            JobId::RecommendationsRefresh => "recommendations.refresh",
            JobId::RecommendationsReembed => "recommendations.reembed",
            JobId::SectionsPersonalize => "sections.personalize",
            JobId::SectionsCurate => "sections.curate",
            JobId::LibraryScan => "library.scan",
            JobId::MetadataEnrich => "metadata.enrich",
            JobId::SearchReindex => "search.reindex",
            JobId::MarkersDetect => "markers.detect",
        }
    }

    /// Parse a stored/requested key back into a typed id (`None` if it names a job
    /// that no longer exists stale DB rows are simply ignored).
    pub fn from_key(key: &str) -> Option<JobId> {
        JobId::ALL.into_iter().find(|id| id.key() == key)
    }
}

/// UI grouping bucket for a job. Serializes lowercase (`"maintenance"`), which the
/// clients turn into the `jobs.cat.{category}` i18n key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Category {
    Maintenance,
    Library,
    Recommendations,
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
    pub key: JobId,
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

#[cfg(test)]
mod tests {
    use super::JobId;

    /// The `serde` rename (wire/DB/ts-rs format) and [`JobId::key`] must agree,
    /// and every key must round-trip through [`JobId::from_key`].
    #[test]
    fn jobid_serde_matches_key() {
        for id in JobId::ALL {
            assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{}\"", id.key()));
            assert_eq!(JobId::from_key(id.key()), Some(id));
        }
    }
}
