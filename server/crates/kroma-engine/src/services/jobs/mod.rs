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
use crate::model::Category;
use crate::state::SharedState;

/// The run logic of a remote (out-of-process module) job, injected from
/// `server/src` (the layer that owns the sidecar supervisor). `kroma-engine` must
/// not depend on the supervisor, so it only ever sees this boxed closure: on a
/// manual or scheduled trigger the manager invokes it with the run's
/// [`JobContext`], and the closure drives the sidecar (a blocking HTTP POST to its
/// `/_job/run/{key}` endpoint). Returning `Err` records the run as failed, exactly
/// like a built-in.
pub type RemoteRun = Arc<dyn Fn(&JobContext) -> anyhow::Result<()> + Send + Sync>;

/// A job contributed at runtime by an out-of-process module. Same console shape as
/// a [`Builtin`], but its `run` is the injected [`RemoteRun`] and its schedule is
/// an owned `String` (it arrives over the wire at registration, not from a
/// `'static` SPEC).
struct RemoteJob {
    key: JobKey,
    category: Category,
    schedule: Option<String>,
    run: RemoteRun,
}

/// The handler to run for a triggered job: either a built-in's `'static` fn or a
/// remote module's injected closure. Computed under the lock in [`JobManager::trigger`]
/// then moved into the worker thread, so the run executes without holding any lock.
enum Runner {
    Local(fn(&JobContext) -> anyhow::Result<()>),
    Remote(RemoteRun),
}

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

/// Current time as epoch milliseconds (UTC instant). Re-exported from kroma-primitives,
/// where the primitive now lives (below the persistence layer).
pub use kroma_primitives::now_ms;

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
    /// Jobs contributed at runtime by out-of-process modules, keyed by their
    /// dotted key string (a `&'static str` leaked once per module+key in
    /// `server/src`). Interior-mutable because a sidecar registers (and
    /// re-registers on every respawn) long after startup, unlike the `'static`
    /// built-in `jobs` map filled once by [`register`](Self::register).
    remote: RwLock<HashMap<&'static str, RemoteJob>>,
    /// Registration order of the remote jobs, listed after the built-ins.
    remote_order: RwLock<Vec<JobKey>>,
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
            remote: RwLock::new(HashMap::new()),
            remote_order: RwLock::new(Vec::new()),
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

    /// Register (or re-register) a job contributed by an out-of-process module, so
    /// it shows in admin Tâches with cron scheduling + run history like a built-in.
    /// Interior-mutable so a sidecar can register after startup and again on every
    /// respawn. `key` is a `&'static str` the caller leaked once per module+key (so
    /// respawns reuse it and the leak stays bounded to the fixed set of job keys).
    ///
    /// The schedule seeds a [`ScheduleState`] ONLY when the key is new: a persisted
    /// DB override (overlaid afterwards via [`load_schedules`](Self::load_schedules))
    /// or an admin customization must survive a re-registration, so an existing
    /// schedule state is left untouched. The `run` closure IS refreshed every call
    /// (a respawn hands us a new port-resolving closure).
    pub fn register_remote(
        &self,
        key: &'static str,
        category: Category,
        schedule: Option<String>,
        run: RemoteRun,
    ) {
        let job = JobKey(key);
        self.schedules.write().unwrap().entry(job).or_insert_with(|| ScheduleState {
            schedule: schedule.clone(),
            enabled: true,
            customized: false,
        });
        {
            let mut order = self.remote_order.write().unwrap();
            if !order.contains(&job) {
                order.push(job);
            }
        }
        self.remote.write().unwrap().insert(key, RemoteJob { key: job, category, schedule, run });
    }

    /// The registered identity for a request/stored key string, or `None` if no
    /// such job exists (stale rows / bad URLs are simply ignored). Checks the
    /// built-ins first, then the remote (module-contributed) jobs.
    pub fn resolve(&self, key: &str) -> Option<JobKey> {
        if let Some(b) = self.jobs.get(key) {
            return Some(b.key);
        }
        self.remote.read().unwrap().get(key).map(|r| r.key)
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
        // The handler is either a built-in's `'static` fn or a remote module's
        // injected closure; resolve it up front (return Unknown if neither has it).
        let runner = if let Some(b) = self.jobs.get(&job) {
            Runner::Local(b.run)
        } else if let Some(r) = self.remote.read().unwrap().get(job.as_str()) {
            Runner::Remote(r.run.clone())
        } else {
            return Err(TriggerError::Unknown);
        };
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
            run_job(manager, state, runner, handle, run_id, key, trigger, job, started_ms)
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
/// Execute one reserved run on the worker thread: record the start, run the
/// handler under `catch_unwind`, classify the outcome, finalize the run row and
/// fire any chained jobs. Runs after [`JobManager::trigger`] has already reserved
/// the one-run-per-key slot.
#[allow(clippy::too_many_arguments)]
fn run_job(
    manager: Arc<JobManager>,
    state: SharedState,
    runner: Runner,
    handle: Arc<RunHandle>,
    run_id: String,
    key: &'static str,
    trigger: &'static str,
    job: JobKey,
    started_ms: i64,
) {
    let pool = state.db.clone();
    // If this insert fails the run still executes, but the later progress/finish
    // UPDATEs no-op against a missing row and the run leaves no trace so surface
    // it rather than swallowing.
    if let Err(e) = db::insert_job_run(&pool, &run_id, key, trigger, started_ms) {
        warn!(job = key, run = %run_id, error = %e, "failed to record job run start");
    }
    info!(job = key, run = %run_id, trigger, "job started");

    let ctx = JobContext::new(state.clone(), handle.clone());
    let result = catch_unwind(AssertUnwindSafe(|| match &runner {
        Runner::Local(f) => f(&ctx),
        Runner::Remote(f) => f(&ctx),
    }));

    let finished_ms = now_ms();
    let (status, error) = classify_result(result, &handle);

    // Mirror a terminal failure into the run's *own* log stream (not only the
    // `error` column), so the Tâches log view always explains why a run ended
    // badly even for a panic or an early `?` that logged nothing itself.
    // Success/cancellation already log their own lines from inside the job body.
    if let ("failed", Some(msg)) = (status, error.as_deref()) {
        let _ = db::insert_job_log(&pool, &run_id, finished_ms, "error", msg);
        state.events.publish(ServerEvent::JobLog {
            run_id: run_id.clone(),
            level: "error",
            message: msg.to_string(),
        });
    }

    if !finalize_run(&pool, &run_id, key, status, finished_ms, error.as_deref()) {
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

    chain_after(&manager, &state, job, key, status);
}

/// Map the `catch_unwind` outcome of a job handler to its `(status, error)`.
fn classify_result(
    result: std::thread::Result<anyhow::Result<()>>,
    handle: &RunHandle,
) -> (&'static str, Option<String>) {
    match result {
        Ok(Ok(())) if handle.is_cancelled() => ("cancelled", None),
        Ok(Ok(())) => ("success", None),
        Ok(Err(e)) => ("failed", Some(format!("{e:#}"))),
        // `panic.as_ref()` yields the inner `dyn Any` (the &str/String payload);
        // `&panic` would unsize the Box itself, so the downcast (and message)
        // would always be lost.
        Err(panic) => ("failed", Some(panic_message(panic.as_ref()))),
    }
}

/// Finalize the run row, retrying a few times: if this write keeps failing (e.g.
/// SQLite busy under contention) the row stays `running` with no terminal status,
/// and `reconcile_running_runs` only sweeps at startup so the console would show
/// it running until the next restart. Returns whether the finish was recorded.
fn finalize_run(
    pool: &db::Pool,
    run_id: &str,
    key: &'static str,
    status: &str,
    finished_ms: i64,
    error: Option<&str>,
) -> bool {
    for attempt in 0..3u32 {
        match db::finish_job_run(pool, run_id, status, finished_ms, error) {
            Ok(_) => return true,
            Err(e) => {
                warn!(job = key, run = %run_id, attempt, error = %e, "failed to record job run finish; retrying");
                std::thread::sleep(std::time::Duration::from_millis(200 * u64::from(attempt + 1)));
            }
        }
    }
    false
}

/// Chaining: fire any job that opted to run after this one, but only when this
/// run actually succeeded. A failed or cancelled upstream must not start its
/// dependents (a cancelled storyboard drain kicking off subtitles would surprise
/// the admin who just cancelled, and a failed run's outputs are exactly what the
/// next stage needs).
fn chain_after(manager: &Arc<JobManager>, state: &SharedState, job: JobKey, key: &'static str, status: &str) {
    if status != "success" {
        return;
    }
    for next in manager.jobs_for_trigger(Trigger::AfterJob(job)) {
        if let Err(e) = manager.trigger(state.clone(), next, "chain") {
            warn!(job = key, next = %next, error = ?e, "chained job did not start");
        }
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        format!("panicked: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panicked: {s}")
    } else {
        "panicked".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> db::Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-jobs-test-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        db::init(&path).unwrap()
    }

    #[test]
    fn job_key_reads_as_str_and_displays() {
        let k = JobKey("cache.cleanup");
        assert_eq!(k.as_str(), "cache.cleanup");
        assert_eq!(k.to_string(), "cache.cleanup");
        // Borrow<str> lets a bare &str index a keyed map.
        let mut map = std::collections::HashMap::new();
        map.insert(k, 7);
        assert_eq!(map.get("cache.cleanup"), Some(&7));
    }

    #[test]
    fn panic_message_downcasts_str_string_or_falls_back() {
        let s: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(panic_message(s.as_ref()), "panicked: boom");
        let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("kaboom"));
        assert_eq!(panic_message(owned.as_ref()), "panicked: kaboom");
        let other: Box<dyn std::any::Any + Send> = Box::new(42u32);
        assert_eq!(panic_message(other.as_ref()), "panicked");
    }

    #[test]
    fn classify_result_maps_every_outcome() {
        let handle = RunHandle::new("run-1".into(), "job".into());

        // Success.
        let ok: std::thread::Result<anyhow::Result<()>> = Ok(Ok(()));
        assert_eq!(classify_result(ok, &handle), ("success", None));

        // A returned error becomes "failed" with its message.
        let errd: std::thread::Result<anyhow::Result<()>> = Ok(Err(anyhow::anyhow!("nope")));
        let (status, msg) = classify_result(errd, &handle);
        assert_eq!(status, "failed");
        assert_eq!(msg.as_deref(), Some("nope"));

        // A caught panic payload becomes "failed" with a panic message.
        let payload: Box<dyn std::any::Any + Send> = Box::new("splat");
        let panicked: std::thread::Result<anyhow::Result<()>> = Err(payload);
        assert_eq!(classify_result(panicked, &handle), ("failed", Some("panicked: splat".to_string())));

        // A clean Ok after a cancel request records "cancelled".
        handle.request_cancel();
        let ok2: std::thread::Result<anyhow::Result<()>> = Ok(Ok(()));
        assert_eq!(classify_result(ok2, &handle), ("cancelled", None));
    }

    #[test]
    fn manager_starts_empty_and_pause_toggles() {
        let m = JobManager::new();
        assert_eq!(m.running_count(), 0);
        assert!(!m.pipeline_paused());
        m.set_pipeline_paused(true);
        assert!(m.pipeline_paused());
        m.set_pipeline_paused(false);
        assert!(!m.pipeline_paused());
        // Cancelling an unknown / not-running job is a no-op false.
        assert!(!m.cancel(JobKey("nothing.here")));
        // No built-ins registered -> no trigger jobs.
        assert!(m.jobs_for_trigger(Trigger::LibraryChange).is_empty());
    }

    #[test]
    fn register_remote_is_resolvable() {
        let m = JobManager::new();
        let run: RemoteRun = Arc::new(|_ctx: &JobContext| Ok(()));
        m.register_remote("mod.job", Category::Maintenance, Some("0 4 * * *".into()), run);
        assert_eq!(m.resolve("mod.job"), Some(JobKey("mod.job")));
        assert_eq!(m.resolve("absent.job"), None);
    }

    #[test]
    fn update_schedule_validates_and_persists() {
        let pool = test_pool();
        let m = JobManager::new();
        // Unknown job cannot be scheduled.
        assert!(m.update_schedule(&pool, JobKey("ghost.job"), None, None).is_err());

        // Seed a schedule slot via a remote registration, then reject bad cron.
        let run: RemoteRun = Arc::new(|_ctx: &JobContext| Ok(()));
        m.register_remote("mod.job", Category::Maintenance, None, run);
        assert!(m
            .update_schedule(&pool, JobKey("mod.job"), Some(Some("not a valid cron".into())), None)
            .is_err());

        // A valid cron + enabled flag persists without error.
        m.update_schedule(&pool, JobKey("mod.job"), Some(Some("0 4 * * *".into())), Some(false))
            .unwrap();
        let rows = db::list_job_schedules(&pool).unwrap();
        let saved =
            rows.iter().find(|(k, ..)| k.as_str() == "mod.job").expect("schedule row persisted");
        assert_eq!(saved.1.as_deref(), Some("0 4 * * *"));
        assert!(!saved.2); // disabled
    }

    // A minimal built-in used to exercise the `'static Builtin` registration path
    // (the roster in `builtins` supplies the real ones at startup).
    fn noop_run(_ctx: &JobContext) -> anyhow::Result<()> {
        Ok(())
    }
    static TEST_BUILTIN: Builtin = Builtin {
        key: JobKey("test.job"),
        category: Category::Maintenance,
        schedule: Some("0 4 * * *"),
        triggers: &[Trigger::LibraryChange],
        run: noop_run,
    };

    #[test]
    fn register_builtin_is_resolvable_and_lists_for_its_trigger() {
        let mut m = JobManager::new();
        m.register(&TEST_BUILTIN);
        // The built-in resolve path (checked before the remote map).
        assert_eq!(m.resolve("test.job"), Some(JobKey("test.job")));
        // Enabled + opted into LibraryChange, so it lists for that trigger only.
        assert_eq!(m.jobs_for_trigger(Trigger::LibraryChange), vec![JobKey("test.job")]);
        assert!(m.jobs_for_trigger(Trigger::AfterJob(JobKey("other.job"))).is_empty());
    }

    #[test]
    fn load_schedules_overlays_overrides_and_ignores_unknown() {
        let pool = test_pool();
        let mut m = JobManager::new();
        m.register(&TEST_BUILTIN);
        // Persisted override disables the job; a row for a job that no longer exists
        // is silently ignored.
        db::upsert_job_schedule(&pool, "test.job", Some("0 6 * * *"), false).unwrap();
        db::upsert_job_schedule(&pool, "ghost.job", Some("0 1 * * *"), true).unwrap();
        m.load_schedules(&pool);
        // Now disabled, so it drops out of its trigger list (watch/chain runs stop too).
        assert!(m.jobs_for_trigger(Trigger::LibraryChange).is_empty());
    }

    #[test]
    fn update_schedule_clears_builtin_to_manual_only() {
        let pool = test_pool();
        let mut m = JobManager::new();
        m.register(&TEST_BUILTIN);
        // Some(None) clears the schedule (manual-only) on a built-in job.
        m.update_schedule(&pool, JobKey("test.job"), Some(None), None).unwrap();
        let rows = db::list_job_schedules(&pool).unwrap();
        let saved = rows.iter().find(|(k, ..)| k.as_str() == "test.job").expect("row persisted");
        assert!(saved.1.is_none(), "schedule cleared");
        assert!(saved.2, "still enabled");
    }

    #[test]
    fn finalize_run_records_terminal_status() {
        let pool = test_pool();
        db::insert_job_run(&pool, "run-x", "test.job", "manual", 1_000).unwrap();
        assert!(finalize_run(&pool, "run-x", "test.job", "success", 2_000, None));
        let runs = db::list_job_runs(&pool, "test.job", 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(runs[0].finished_at, Some(2_000));
    }

    #[test]
    fn finalize_run_records_failure_message() {
        let pool = test_pool();
        db::insert_job_run(&pool, "run-y", "test.job", "manual", 1_000).unwrap();
        assert!(finalize_run(&pool, "run-y", "test.job", "failed", 3_000, Some("boom")));
        let runs = db::list_job_runs(&pool, "test.job", 10).unwrap();
        assert_eq!(runs[0].status, "failed");
        assert_eq!(runs[0].error.as_deref(), Some("boom"));
    }

    #[test]
    fn cancel_all_on_idle_manager_is_a_noop() {
        let m = JobManager::new();
        // No runs in flight: cancel_all must not panic and leaves the count at 0.
        m.cancel_all();
        assert_eq!(m.running_count(), 0);
    }

    #[test]
    fn register_remote_reregistration_stays_resolvable() {
        let m = JobManager::new();
        let run: RemoteRun = Arc::new(|_ctx: &JobContext| Ok(()));
        m.register_remote("mod.job", Category::Maintenance, Some("0 4 * * *".into()), run);
        // A respawn re-registers the same key with a refreshed closure; the entry
        // is not duplicated and stays resolvable to the one JobKey.
        let run2: RemoteRun = Arc::new(|_ctx: &JobContext| Ok(()));
        m.register_remote("mod.job", Category::Recommendations, Some("0 9 * * *".into()), run2);
        assert_eq!(m.resolve("mod.job"), Some(JobKey("mod.job")));
    }

    // ----- End-to-end trigger against a full SharedState. A registered *remote*
    // job (registrable post-construction, unlike a built-in) is triggered and its
    // recorded run row + status asserted. The handlers return immediately, so no
    // unbounded work is spawned. --------------------------------------------------

    use crate::test_support;

    /// Poll (bounded) until the manager has no running job, so the spawned blocking
    /// worker has finished recording its run.
    async fn wait_idle(mgr: &Arc<JobManager>) {
        for _ in 0..300 {
            if mgr.running_count() == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("job run did not finish within the timeout");
    }

    #[tokio::test]
    async fn trigger_runs_a_job_to_success_and_records_the_run() {
        let state = test_support::test_state();
        let run: RemoteRun = Arc::new(|ctx: &JobContext| {
            ctx.info("did the work");
            Ok(())
        });
        state.jobs.register_remote("test.remote.ok", Category::Maintenance, None, run);

        let run_id =
            state.jobs.trigger(state.clone(), JobKey("test.remote.ok"), "manual").expect("triggered");
        wait_idle(&state.jobs).await;

        let runs = db::list_job_runs(&state.db, "test.remote.ok", 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].status, "success");
        assert!(runs[0].finished_at.is_some());
        // The slot was released after the run finished.
        assert_eq!(state.jobs.running_count(), 0);
    }

    #[tokio::test]
    async fn trigger_records_a_failed_run_with_its_message() {
        let state = test_support::test_state();
        let run: RemoteRun = Arc::new(|_ctx: &JobContext| Err(anyhow::anyhow!("kaput")));
        state.jobs.register_remote("test.remote.err", Category::Maintenance, None, run);

        state.jobs.trigger(state.clone(), JobKey("test.remote.err"), "manual").expect("triggered");
        wait_idle(&state.jobs).await;

        let runs = db::list_job_runs(&state.db, "test.remote.err", 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "failed");
        assert!(runs[0].error.as_deref().unwrap().contains("kaput"));
    }

    #[tokio::test]
    async fn trigger_rejects_a_second_run_while_one_is_in_flight() {
        let state = test_support::test_state();
        // A handler that blocks until released, so the first run is provably still
        // in flight when the second trigger is attempted.
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let g = gate.clone();
        let run: RemoteRun = Arc::new(move |_ctx: &JobContext| {
            while !g.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(())
        });
        state.jobs.register_remote("test.remote.slow", Category::Maintenance, None, run);

        state.jobs.trigger(state.clone(), JobKey("test.remote.slow"), "manual").expect("first run");
        // Second trigger while the first holds the one-run-per-key slot.
        let second = state.jobs.trigger(state.clone(), JobKey("test.remote.slow"), "manual");
        assert_eq!(second, Err(TriggerError::AlreadyRunning));
        // Release the handler and let it drain.
        gate.store(true, std::sync::atomic::Ordering::Relaxed);
        wait_idle(&state.jobs).await;
    }

    #[test]
    fn trigger_unknown_job_is_rejected() {
        let state = test_support::test_state();
        assert_eq!(
            state.jobs.trigger(state.clone(), JobKey("does.not.exist"), "manual"),
            Err(TriggerError::Unknown)
        );
    }

    #[test]
    fn chain_after_does_not_fire_dependents_on_non_success() {
        let state = test_support::test_state();
        // A failed / cancelled upstream must not start any chained job. No built-in
        // depends on this key either, so the count stays at zero (no spawn happens).
        chain_after(&state.jobs, &state, JobKey("test.remote.ok"), "test.remote.ok", "failed");
        chain_after(&state.jobs, &state, JobKey("test.remote.ok"), "test.remote.ok", "cancelled");
        assert_eq!(state.jobs.running_count(), 0);
    }
}
