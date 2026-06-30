//! Background job system: a tiny scheduler + registry that runs named units of
//! work on a cron schedule or on demand, with every run tracked (status,
//! progress, logs, errors) in SQLite and surfaced live in the admin console.
//!
//! ## Authoring a job
//!
//! Each job is a handler file under [`builtins`] plus one row in its typed
//! [`builtins::JOBS`] registry, keyed by a [`JobId`] variant (no magic strings):
//!
//! ```ignore
//! // builtins/cache_cleanup.rs
//! use super::prelude::*;
//! pub(super) fn run(ctx: &JobContext) -> Result<()> { /* … */ Ok(()) }
//!
//! // builtins.rs one row in JOBS:
//! Builtin { id: JobId::CacheCleanup, category: Category::Maintenance,
//!           schedule: Some("0 4 * * *"), triggers: &[], run: cache_cleanup::run },
//! ```
//!
//! The handler receives a [`JobContext`] (`ctx.state` for the whole app,
//! `ctx.info`/`ctx.warn`/`ctx.progress`/`ctx.cancelled`). Returning `Err` records
//! the run as failed with the message; returning `Ok` after an observed
//! cancellation records it as `cancelled`. Jobs run on a blocking thread, so heavy
//! CPU work (re-embedding, LLM section generation, …) is fine.
//!
//! Beyond manual runs + the cron `schedule`, a job can opt into extra trigger
//! sources via [`Trigger`] (file-watch, or chaining after another job).

mod builtins;
mod context;
mod cron;
mod scheduler;
mod views;

pub use builtins::{register_all, Builtin};
pub use context::{JobContext, RunHandle};
pub use cron::Cron;

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};
use time::OffsetDateTime;
use tracing::{info, warn};

use crate::db;
use crate::infra::events::ServerEvent;
use crate::model::{Category, JobId};
use crate::state::SharedState;

/// Recent runs kept per job in the detail view / DB prune.
const RUNS_KEPT: usize = 50;

/// Current time as epoch milliseconds (UTC instant).
pub fn now_ms() -> i64 {
    (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

/// "Now" shifted into the configured scheduler timezone, so cron `0 4 * * *`
/// means 4am local. Offset (in minutes) is the `jobsUtcOffset` setting (0/UTC by
/// default).
fn now_local(state: &SharedState) -> OffsetDateTime {
    let mins = state.settings.get_i64("jobsUtcOffset", 0);
    let offset = time::UtcOffset::from_whole_seconds((mins * 60) as i32)
        .unwrap_or(time::UtcOffset::UTC);
    OffsetDateTime::now_utc().to_offset(offset)
}

/// A trigger source a job opts into, on top of manual runs + its cron schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Run when the library filesystem changes (debounced; fired by the watcher).
    LibraryChange,
    /// Run right after another job's run finishes (chaining). Built-in only, so
    /// authors are trusted not to form cycles.
    AfterJob(JobId),
}

/// Why a [`JobManager::trigger`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerError {
    /// No job is registered under that id.
    Unknown,
    /// The job is already running (one run per id at a time).
    AlreadyRunning,
}

/// A registered job: its static metadata plus the handler (a plain `fn` pointer).
#[derive(Clone, Copy)]
struct Registered {
    category: Category,
    default_schedule: Option<&'static str>,
    triggers: &'static [Trigger],
    run: fn(&JobContext) -> Result<()>,
}

/// The effective, possibly user-overridden schedule + enabled flag for one job.
#[derive(Clone)]
struct ScheduleState {
    schedule: Option<String>,
    enabled: bool,
    /// True once an admin has overridden the built-in default (persisted row).
    customized: bool,
}

/// The job registry + live run state. Built once at startup (see
/// [`crate::state::AppState`]) and shared behind an `Arc`.
pub struct JobManager {
    /// Registration order, for stable listing.
    order: Vec<JobId>,
    jobs: HashMap<JobId, Registered>,
    schedules: RwLock<HashMap<JobId, ScheduleState>>,
    running: RwLock<HashMap<JobId, Arc<RunHandle>>>,
    counter: AtomicU64,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            jobs: HashMap::new(),
            schedules: RwLock::new(HashMap::new()),
            running: RwLock::new(HashMap::new()),
            counter: AtomicU64::new(0),
        }
    }

    /// Register a job from its [`Builtin`] descriptor. Call during startup only
    /// (before wrapping in `Arc`).
    pub fn register(&mut self, b: &Builtin) {
        self.schedules.write().unwrap().insert(
            b.id,
            ScheduleState {
                schedule: b.schedule.map(str::to_string),
                enabled: true,
                customized: false,
            },
        );
        self.order.push(b.id);
        self.jobs.insert(
            b.id,
            Registered {
                category: b.category,
                default_schedule: b.schedule,
                triggers: b.triggers,
                run: b.run,
            },
        );
    }

    /// Overlay persisted schedule overrides from the DB onto the defaults.
    pub fn load_schedules(&self, pool: &db::Pool) {
        let rows = match db::list_job_schedules(pool) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "failed to load job schedules");
                return;
            }
        };
        let mut map = self.schedules.write().unwrap();
        for (key, schedule, enabled) in rows {
            // Ignore rows for jobs that no longer exist.
            if let Some(st) = JobId::from_key(&key).and_then(|id| map.get_mut(&id)) {
                st.schedule = schedule;
                st.enabled = enabled;
                st.customized = true;
            }
        }
    }

    /// Trigger a job now (manual or scheduled). Returns the new run id.
    pub fn trigger(
        self: &Arc<Self>,
        state: SharedState,
        id: JobId,
        trigger: &'static str,
    ) -> std::result::Result<String, TriggerError> {
        let registered = *self.jobs.get(&id).ok_or(TriggerError::Unknown)?;
        let key = id.key();

        // One run per id. Reserve the slot under the lock to avoid a race.
        let started_ms = now_ms();
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("{}-{started_ms}-{n}", key.replace('.', "_"));
        let handle = Arc::new(RunHandle::new(run_id.clone(), key.to_string()));
        {
            let mut running = self.running.write().unwrap();
            if running.contains_key(&id) {
                return Err(TriggerError::AlreadyRunning);
            }
            running.insert(id, handle.clone());
        }

        // Announce immediately so the UI flips to "running" without waiting on
        // the DB insert (which happens on the worker thread below).
        state.events.publish(ServerEvent::JobStarted {
            key: key.to_string(),
            run_id: run_id.clone(),
        });

        let manager = self.clone();
        let returned_id = run_id.clone();
        tokio::task::spawn_blocking(move || {
            let pool = state.db.clone();
            // If this insert fails the run still executes, but the later
            // progress/finish UPDATEs no-op against a missing row and the run
            // leaves no trace so surface it rather than swallowing.
            if let Err(e) = db::insert_job_run(&pool, &run_id, key, trigger, started_ms) {
                warn!(job = key, run = %run_id, error = %e, "failed to record job run start");
            }
            info!(job = key, run = %run_id, trigger, "job started");

            let ctx = JobContext::new(state.clone(), handle.clone());
            let result = catch_unwind(AssertUnwindSafe(|| (registered.run)(&ctx)));

            let finished_ms = now_ms();
            let (status, error): (&str, Option<String>) = match result {
                Ok(Ok(())) if handle.is_cancelled() => ("cancelled", None),
                Ok(Ok(())) => ("success", None),
                Ok(Err(e)) => ("failed", Some(format!("{e:#}"))),
                Err(panic) => ("failed", Some(panic_message(&panic))),
            };
            // Finalize the run row, retrying a few times: if this write keeps
            // failing (e.g. SQLite busy under contention) the row stays `running`
            // with no terminal status, and `reconcile_running_runs` only sweeps at
            // startup so the console would show it running until the next restart.
            let mut finished = false;
            for attempt in 0..3u32 {
                match db::finish_job_run(&pool, &run_id, status, finished_ms, error.as_deref()) {
                    Ok(_) => {
                        finished = true;
                        break;
                    }
                    Err(e) => {
                        warn!(job = key, run = %run_id, attempt, error = %e, "failed to record job run finish; retrying");
                        std::thread::sleep(std::time::Duration::from_millis(200 * u64::from(attempt + 1)));
                    }
                }
            }
            if !finished {
                warn!(job = key, run = %run_id, "gave up recording job finish; run may show as running until restart");
            }
            let _ = db::prune_job_runs(&pool, key, RUNS_KEPT); // cosmetic cleanup
            manager.running.write().unwrap().remove(&id);

            match status {
                "failed" => warn!(job = key, run = %run_id, error = error.as_deref().unwrap_or(""), "job failed"),
                other => info!(job = key, run = %run_id, status = other, "job finished"),
            }
            state.events.publish(ServerEvent::JobFinished {
                key: key.to_string(),
                run_id,
                status: status.to_string(),
            });

            // Chaining: fire any job that opted to run after this one.
            for next in manager.jobs_for_trigger(Trigger::AfterJob(id)) {
                let _ = manager.trigger(state.clone(), next, "chain");
            }
        });

        Ok(returned_id)
    }

    /// Request cancellation of a job's current run. Returns false if not running.
    pub fn cancel(&self, id: JobId) -> bool {
        if let Some(handle) = self.running.read().unwrap().get(&id) {
            handle.request_cancel();
            true
        } else {
            false
        }
    }

    /// Update a job's schedule and/or enabled flag, persisting the override.
    /// `schedule = Some(None)` clears it (manual-only); validates cron syntax.
    pub fn update_schedule(
        &self,
        pool: &db::Pool,
        id: JobId,
        schedule: Option<Option<String>>,
        enabled: Option<bool>,
    ) -> Result<()> {
        let mut map = self.schedules.write().unwrap();
        let st = map.get_mut(&id).ok_or_else(|| anyhow!("unknown job"))?;
        if let Some(new_schedule) = schedule {
            if let Some(expr) = &new_schedule {
                if !Cron::is_valid(expr) {
                    return Err(anyhow!("invalid cron expression"));
                }
            }
            st.schedule = new_schedule;
        }
        if let Some(en) = enabled {
            st.enabled = en;
        }
        st.customized = true;
        db::upsert_job_schedule(pool, id.key(), st.schedule.as_deref(), st.enabled)?;
        Ok(())
    }

    /// Enabled jobs that opted into trigger source `t`, in registration order. A
    /// disabled job is skipped here just as the scheduler's `due_jobs` skips it
    /// so disabling a job in the console stops its watch/chain runs too, not only
    /// its scheduled ones (a manual "Run now" goes through `trigger` directly and
    /// is unaffected).
    pub fn jobs_for_trigger(&self, t: Trigger) -> Vec<JobId> {
        let schedules = self.schedules.read().unwrap();
        self.order
            .iter()
            .copied()
            .filter(|id| self.jobs.get(id).is_some_and(|r| r.triggers.contains(&t)))
            .filter(|id| schedules.get(id).map_or(true, |s| s.enabled))
            .collect()
    }

    // Read models for the API (`list`/`detail`/`info_for`) live in `views.rs`;
    // the cron tick loop (`spawn_scheduler`/`due_jobs`) lives in `scheduler.rs`.
    // Both are `impl JobManager` blocks in sibling files (same module privacy).
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort message from a caught panic payload.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        format!("panicked: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panicked: {s}")
    } else {
        "panicked".to_string()
    }
}
