//! Persistence for the per-element processing ledger (`pipeline_tasks`). See
//! `crate::services::pipeline`. One row per `(stage, subject)`; all writes are
//! batched into single transactions so the many stage workers never contend on
//! SQLite's single writer (the dispatcher owns every write here). Timestamps are
//! epoch milliseconds.
//!
//! The module splits into two concerns, both re-exported flat here so the public
//! `crate::pipeline::<item>` paths resolve unchanged:
//! - [`ops`]: ledger write / lifecycle ops (reconcile, enqueue, claim, finish,
//!   reset, retry, reprocess) plus their shared consts/types.
//! - [`query`]: read-only status queries and the lean elements-list row-mappers.

mod ops;
mod query;

#[cfg(test)]
mod tests;

pub use ops::{
    claim_batch, enqueue, finish_batch, reconcile, reprocess, requeue_stage, reset_running, retry,
    retry_backoff_ms, TaskResult, UNREADABLE_SIG,
};
pub use query::{
    counts, failed_tasks, item_titles, raw_items, raw_shows, show_titles, stage_stat,
    stage_statuses, task_status, worst_status, RawItem,
};
