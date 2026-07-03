//! Wire types for the per-element processing pipeline admin API
//! (`/api/admin/pipeline`) and the `pipeline.stats` live event. Pure data (serde
//! + ts-rs); the engine that produces them lives in
//! [`crate::services::pipeline`], persistence in [`crate::db::pipeline`].
//!
//! A *stage* (probe, metadata, storyboard, markers, embed) processes one subject
//! (a file / item / show / season) at a time and records the outcome in the
//! `pipeline_tasks` ledger, so a re-run only does the non-done work and failures
//! are individually visible + retriable.

use serde::Serialize;
use ts_rs::TS;

/// Health counters for one pipeline stage, aggregated from the ledger. Carried by
/// both the REST view and the throttled `pipeline.stats` WS event, so the
/// dashboard updates live without polling per-task rows.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StageStat {
    /// Short stage key (`"probe"`, `"markers"`, …); i18n base `pipeline.stage.{stage}`.
    pub stage: String,
    /// Full job key of the stage's drain job (`"pipeline.probe"`), to correlate
    /// with the existing `/api/admin/jobs` run/schedule/log surface.
    pub key: String,
    /// What one task operates on: `"file" | "item" | "show" | "season"`.
    pub subject_kind: String,
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
    pub blocked: i64,
}

/// `GET /api/admin/pipeline`: every stage's health, in DAG order, plus whether the
/// whole pipeline is currently held by the global admin pause.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PipelineView {
    pub stages: Vec<StageStat>,
    pub paused: bool,
}

/// The status of one treatment (stage) as applied to a single catalog element,
/// for the per-element "Traitements" panel on a film/episode/show page.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Treatment {
    /// Short stage key (`"probe"`, `"metadata"`, `"storyboard"`, `"markers"`, `"embed"`).
    pub key: String,
    /// `"done" | "missing" | "pending" | "running" | "failed"`.
    pub status: String,
    /// Failure message when `status == "failed"` (only populated in the elements
    /// list, for the detail drawer). `None` elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Per-series aggregate over its episodes, for the elements list (the client
/// formats the localized "28 episodes · probed 28/28 · …" line).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EpStats {
    pub episodes: i64,
    pub probed: i64,
    pub storyboarded: i64,
    pub seasons: i64,
    pub marker_seasons: i64,
}

/// One catalog element (film / series / episode) with the status of each
/// treatment applied to it and an overall roll-up, for the pipeline elements list.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElementRow {
    pub id: String,
    /// `"film" | "series" | "episode"`.
    pub kind: String,
    pub title: String,
    /// Cached poster URL (`/api/images/…`) resolved from TMDB metadata; `None`
    /// falls back to a placeholder client-side. Episodes borrow their show's.
    pub poster: Option<String>,
    /// Structured hints so the client builds a localized subtitle.
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub duration_ms: Option<u64>,
    pub season_count: Option<u32>,
    pub treatments: Vec<Treatment>,
    /// `"ok" | "pending" | "running" | "failed"`.
    pub overall: String,
    /// Series only: per-episode aggregate for the detail drawer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ep_stats: Option<EpStats>,
}

/// Status tally over ALL elements (unfiltered), for the filter chips + header.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElementCounts {
    pub total: i64,
    pub ok: i64,
    pub pending: i64,
    pub running: i64,
    pub failed: i64,
    pub film: i64,
    pub series: i64,
    pub episode: i64,
}

/// `GET /api/admin/pipeline/elements`: a filtered, paginated page of the catalog
/// with per-element treatment state, plus the full-catalog counts for the chips.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PipelineElements {
    pub total: i64,
    pub page: i64,
    pub pages: i64,
    pub counts: ElementCounts,
    pub elements: Vec<ElementRow>,
}

/// `GET /api/admin/pipeline/item|show/:id`: every treatment that applies to this
/// element + whether it has been done, derived from the real artifacts (probed
/// flag, metadata, cached sheet, markers, vector) with the ledger overlaid for
/// in-progress / failed states.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElementProcessing {
    pub treatments: Vec<Treatment>,
}

/// One failed (or otherwise notable) ledger row, for the stage drill-down. The
/// human-readable `title` is resolved against the catalog by the API layer.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PipelineTaskView {
    pub stage: String,
    pub subject_kind: String,
    pub subject_id: String,
    /// Best-effort catalog title for the subject (falls back to the id).
    pub title: String,
    pub status: String,
    pub attempts: i64,
    pub error: Option<String>,
    pub finished_at: Option<i64>,
}
