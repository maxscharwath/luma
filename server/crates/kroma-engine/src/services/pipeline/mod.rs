//! Per-element processing pipeline.
//!
//! Turns the heavy monolithic library jobs (marker detection, storyboard
//! pre-generation, and eventually probe/metadata/embed) into resumable,
//! incremental, per-subject work tracked in the `pipeline_tasks` ledger
//! ([`crate::db::pipeline`]). Each **stage** ([`stage::Stage`]) processes one
//! subject (a file / item / show / season) at a time; the [`dispatcher`] drains a
//! stage's queue (reconcile -> claim -> process -> record), and a job `SPEC` per
//! stage (in [`stages`]) plugs the drain into the existing scheduler/console so
//! it gets cron, manual-run, cancel, progress, logs and `AfterJob`/`LibraryChange`
//! chaining for free.
//!
//! Why: a re-run only does the non-done work (a `done` task with an unchanged
//! input signature is skipped), failures are individually visible + retriable,
//! and a freshly added file flows through automatically instead of waiting on the
//! next whole-library sweep.

pub mod dispatcher;
pub mod elements;
pub mod reprocess;
pub mod stage;
pub mod stages;

/// Short keys of the stages that ship today, in DAG order. The admin Pipeline
/// dashboard iterates this to show a card per stage even before any task exists.
pub const STAGE_KEYS: &[(&str, &str, &str)] = &[
    // (short, full job key, subject_kind) in DAG order.
    (stages::probe::STAGE.short, stages::probe::STAGE.key, stages::probe::STAGE.subject_kind),
    (
        stages::loudness::STAGE.short,
        stages::loudness::STAGE.key,
        stages::loudness::STAGE.subject_kind,
    ),
    (stages::metadata::STAGE.short, stages::metadata::STAGE.key, stages::metadata::STAGE.subject_kind),
    (
        stages::storyboard::STAGE.short,
        stages::storyboard::STAGE.key,
        stages::storyboard::STAGE.subject_kind,
    ),
    (
        stages::subtitles::STAGE.short,
        stages::subtitles::STAGE.key,
        stages::subtitles::STAGE.subject_kind,
    ),
    (stages::markers::STAGE.short, stages::markers::STAGE.key, stages::markers::STAGE.subject_kind),
    (stages::embed::STAGE.short, stages::embed::STAGE.key, stages::embed::STAGE.subject_kind),
];

/// Startup crash-recovery: any ledger task left `running` by a process that died
/// mid-drain is reset to `pending`, mirroring [`crate::db::reconcile_running_runs`]
/// for job runs. Call once at startup.
pub fn recover_on_boot(pool: &crate::db::Pool) {
    match crate::db::pipeline::reset_running(pool, None) {
        Ok(n) if n > 0 => {
            tracing::info!(reset = n, "pipeline: reset stranded running tasks to pending");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "pipeline: failed to reset stranded running tasks"),
    }
}
