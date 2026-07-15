//! Background job system: a tiny scheduler + registry that runs named units of
//! work on a cron schedule or on demand, with every run tracked (status,
//! progress, logs, errors) in SQLite and surfaced live in the admin console.
//!
//! ## Authoring a job
//!
//! Each job is a self-contained handler file under [`builtins`] that owns both its
//! handler and its [`Builtin`] descriptor (`SPEC`). The job's identity is its
//! [`JobKey`] (a dotted key, declared right here in the `SPEC`); the roster in
//! [`builtins`] just lists the `SPEC`s and rejects duplicate keys at compile time:
//!
//! ```ignore
//! // builtins/cache_cleanup.rs
//! use super::prelude::*;
//! pub(super) const SPEC: Builtin = Builtin {
//!     key: JobKey("cache.cleanup"), category: Category::Maintenance,
//!     schedule: Some("0 4 * * *"), triggers: &[], run,
//! };
//! pub(super) fn run(ctx: &JobContext) -> Result<()> { /* … */ Ok(()) }
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};
use time::OffsetDateTime;
use tracing::{info, warn};

use crate::db;
use crate::infra::events::ServerEvent;
use crate::state::SharedState;

/// A built-in job's identity: its stable dotted key (`"cache.cleanup"`), which is
/// also the DB key, the `/api/admin/jobs/:key` URL segment and the i18n base
/// (`jobs.{key}.name`). Each job declares its own in its `SPEC`
/// ([`crate::services::jobs::builtins`]); uniqueness is enforced there at compile
/// time. A thin newtype so it reads as a distinct type in signatures rather than a
/// bare string, yet it `Borrow`s as `str` so a runtime request key (e.g. a URL
/// segment) looks one up directly in the keyed maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobKey(pub &'static str);

impl JobKey {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::borrow::Borrow<str> for JobKey {
    fn borrow(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for JobKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Recent runs kept per job in the detail view / DB prune.
const RUNS_KEPT: usize = 50;

/// Current time as epoch milliseconds (UTC instant). Re-exported from luma-primitives,
/// where the primitive now lives (below the persistence layer).
pub use luma_primitives::now_ms;

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
    AfterJob(JobKey),
}

/// Why a [`JobManager::trigger`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerError {
    /// No job is registered under that key.
    Unknown,
    /// The job is already running (one run per key at a time).
    AlreadyRunning,
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
    order: Vec<JobKey>,
    /// The static descriptor per job, borrowed straight from the `'static` roster
    /// (no per-field copy: the `Builtin` already holds everything we need).
    jobs: HashMap<JobKey, &'static Builtin>,
    schedules: RwLock<HashMap<JobKey, ScheduleState>>,
    running: RwLock<HashMap<JobKey, Arc<RunHandle>>>,
    counter: AtomicU64,
    /// Global "hold all pipeline stages" switch. The dispatcher parks every drain
    /// while this is set (heavy background work stops within a poll tick, leftover
    /// tasks stay `pending` and resume on clear). Seeded from the persisted
    /// `pipelinePaused` setting at boot and flipped by the admin pause/resume
    /// endpoints. Separate from the per-stage playback pause (which only yields the
    /// playback-sensitive stages while something is streaming).
    pipeline_paused: AtomicBool,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            jobs: HashMap::new(),
            schedules: RwLock::new(HashMap::new()),
            running: RwLock::new(HashMap::new()),
            counter: AtomicU64::new(0),
            pipeline_paused: AtomicBool::new(false),
        }
    }

    /// Set (or clear) the global pipeline pause. Cheap; the dispatcher reads it
    /// each poll tick, so it takes effect within a couple of seconds.
    pub fn set_pipeline_paused(&self, paused: bool) {
        self.pipeline_paused.store(paused, Ordering::Relaxed);
    }

    /// Whether all pipeline stages are currently held by the global pause.
    pub fn pipeline_paused(&self) -> bool {
        self.pipeline_paused.load(Ordering::Relaxed)
    }

    /// Register a job from its `'static` [`Builtin`] descriptor. Call during
    /// startup only (before wrapping in `Arc`).
    pub fn register(&mut self, b: &'static Builtin) {
        self.schedules.write().unwrap().insert(
            b.key,
            ScheduleState {
                schedule: b.schedule.map(str::to_string),
                enabled: true,
                customized: false,
            },
        );
        self.order.push(b.key);
        self.jobs.insert(b.key, b);
    }

    /// The registered identity for a request/stored key string, or `None` if no
    /// such job exists (stale rows / bad URLs are simply ignored).
    pub fn resolve(&self, key: &str) -> Option<JobKey> {
        self.jobs.get(key).map(|b| b.key)
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
            // Ignore rows for jobs that no longer exist (`JobKey: Borrow<str>` lets
            // the stored key index the map directly).
            if let Some(st) = map.get_mut(key.as_str()) {
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
        job: JobKey,
        trigger: &'static str,
    ) -> std::result::Result<String, TriggerError> {
        let builtin = *self.jobs.get(&job).ok_or(TriggerError::Unknown)?;
        let key = job.as_str();

        // One run per key. Reserve the slot under the lock to avoid a race.
        let started_ms = now_ms();
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("{}-{started_ms}-{n}", key.replace('.', "_"));
        let handle = Arc::new(RunHandle::new(run_id.clone(), key.to_string()));
        {
            let mut running = self.running.write().unwrap();
            if running.contains_key(&job) {
                return Err(TriggerError::AlreadyRunning);
            }
            running.insert(job, handle.clone());
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
            let result = catch_unwind(AssertUnwindSafe(|| (builtin.run)(&ctx)));

            let finished_ms = now_ms();
            let (status, error): (&str, Option<String>) = match result {
                Ok(Ok(())) if handle.is_cancelled() => ("cancelled", None),
                Ok(Ok(())) => ("success", None),
                Ok(Err(e)) => ("failed", Some(format!("{e:#}"))),
                Err(panic) => ("failed", Some(panic_message(&panic))),
            };

            // Mirror a terminal failure into the run's *own* log stream (not only
            // the `error` column), so the Tâches log view always explains why a run
            // ended badly even for a panic or an early `?` that logged nothing
            // itself. Success/cancellation already log their own lines from inside
            // the job body.
            if let ("failed", Some(msg)) = (status, error.as_deref()) {
                let _ = db::insert_job_log(&pool, &run_id, finished_ms, "error", msg);
                state.events.publish(ServerEvent::JobLog {
                    run_id: run_id.clone(),
                    level: "error",
                    message: msg.to_string(),
                });
            }
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
            manager.running.write().unwrap().remove(&job);

            match status {
                "failed" => warn!(job = key, run = %run_id, error = error.as_deref().unwrap_or(""), "job failed"),
                other => info!(job = key, run = %run_id, status = other, "job finished"),
            }
            state.events.publish(ServerEvent::JobFinished {
                key: key.to_string(),
                run_id,
                status: status.to_string(),
            });

            // Chaining: fire any job that opted to run after this one, but only
            // when this run actually succeeded. A failed or cancelled upstream
            // must not start its dependents (a cancelled storyboard drain
            // kicking off subtitles would surprise the admin who just cancelled,
            // and a failed run's outputs are exactly what the next stage needs).
            if status == "success" {
                for next in manager.jobs_for_trigger(Trigger::AfterJob(job)) {
                    if let Err(e) = manager.trigger(state.clone(), next, "chain") {
                        warn!(job = key, next = %next, error = ?e, "chained job did not start");
                    }
                }
            }
        });

        Ok(returned_id)
    }

    /// Request cancellation of every running job (graceful shutdown). Each run
    /// observes its cancel flag at the next poll tick, records itself
    /// `cancelled`, and releases its slot; the caller polls [`running_count`]
    /// to wait for the drain.
    ///
    /// [`running_count`]: Self::running_count
    pub fn cancel_all(&self) {
        for handle in self.running.read().unwrap().values() {
            handle.request_cancel();
        }
    }

    /// How many jobs are currently running.
    pub fn running_count(&self) -> usize {
        self.running.read().unwrap().len()
    }

    /// Request cancellation of a job's current run. Returns false if not running.
    pub fn cancel(&self, job: JobKey) -> bool {
        if let Some(handle) = self.running.read().unwrap().get(&job) {
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
        job: JobKey,
        schedule: Option<Option<String>>,
        enabled: Option<bool>,
    ) -> Result<()> {
        let mut map = self.schedules.write().unwrap();
        let st = map.get_mut(&job).ok_or_else(|| anyhow!("unknown job"))?;
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
        db::upsert_job_schedule(pool, job.as_str(), st.schedule.as_deref(), st.enabled)?;
        Ok(())
    }

    /// Enabled jobs that opted into trigger source `t`, in registration order. A
    /// disabled job is skipped here just as the scheduler's `due_jobs` skips it
    /// so disabling a job in the console stops its watch/chain runs too, not only
    /// its scheduled ones (a manual "Run now" goes through `trigger` directly and
    /// is unaffected).
    pub fn jobs_for_trigger(&self, t: Trigger) -> Vec<JobKey> {
        let schedules = self.schedules.read().unwrap();
        self.order
            .iter()
            .copied()
            .filter(|job| self.jobs.get(job).is_some_and(|b| b.triggers.contains(&t)))
            .filter(|job| schedules.get(job).is_none_or(|s| s.enabled))
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
