//! A pipeline *stage* descriptor: one kind of per-subject processing (probe,
//! metadata, storyboard, markers, embed).
//!
//! Declared as a `const STAGE` next to its `enumerate`/`process` in each
//! `stages/<name>.rs`, mirroring the job `SPEC` pattern. The
//! [`super::dispatcher`] drives it; a job `SPEC` (a `Builtin`) wraps
//! `dispatcher::run(&STAGE, ctx)` so the stage plugs into the existing scheduler,
//! console, cancel and chaining for free.

use anyhow::Result;

use crate::services::jobs::JobContext;
use crate::state::SharedState;

/// Enumerate every subject a stage currently applies to, each paired with a cheap
/// signature of its inputs: `(subject_id, signature)`.
type EnumerateFn = fn(&SharedState) -> Result<Vec<(String, String)>>;

/// One stage of the per-element pipeline.
pub struct Stage {
    /// Short key stored in `pipeline_tasks.stage` and used as the i18n base
    /// (`pipeline.stage.{short}`), e.g. `"probe"`.
    pub short: &'static str,
    /// Full job key of the drain `Builtin` (`"pipeline.probe"`), so the ledger
    /// stats correlate with the existing `/api/admin/jobs` run/schedule surface.
    pub key: &'static str,
    /// What one task operates on: `"file" | "item" | "show" | "season"`.
    pub subject_kind: &'static str,
    /// Max concurrent workers within one drain.
    pub concurrency: usize,
    /// Yield entirely to live playback (heavy disk/CPU stages pause while anyone
    /// is streaming, exactly like the old markers/storyboards jobs did).
    pub pause_for_playback: bool,
    /// Every subject this stage currently applies to, each with a cheap signature
    /// of its inputs. Dependencies are encoded *here* (e.g. storyboard only
    /// enumerates items whose file is already probed), so there is no separate DAG
    /// gate to maintain. Should be one set-based query, not N point lookups.
    pub enumerate: EnumerateFn,
    /// Process ONE subject, addressed by its id. Wraps existing code and may write
    /// the DB with that code's established pattern. Returning `Err` records the
    /// task as `failed` (with the message); `Ok` records it `done`.
    pub process: fn(&JobContext, &str) -> Result<()>,
}
