//! The per-run handle (live state behind the registry) and the [`JobContext`]
//! handed to a running job its only interface to the outside world: structured
//! logging, progress reporting and cooperative cancellation.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use crate::db;
use crate::infra::events::ServerEvent;
use crate::state::SharedState;

use super::now_ms;

/// Live state of an in-flight run, kept in the manager's `running` map so the
/// admin API can report progress and request cancellation without touching the
/// DB. Atomics so the job thread and HTTP handlers don't contend on a lock.
pub struct RunHandle {
    pub run_id: String,
    pub key: String,
    pub(super) cancel: AtomicBool,
    pub(super) done: AtomicI64,
    pub(super) total: AtomicI64,
    /// Throttle stamp for the DB/WS progress writes (epoch ms of the last flush).
    last_flush_ms: AtomicI64,
}

impl RunHandle {
    pub fn new(run_id: String, key: String) -> Self {
        Self {
            run_id,
            key,
            cancel: AtomicBool::new(false),
            done: AtomicI64::new(0),
            total: AtomicI64::new(0),
            last_flush_ms: AtomicI64::new(0),
        }
    }

    /// Request cooperative cancellation; the job observes it via
    /// [`JobContext::cancelled`].
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Current progress `(done, total)` `total == 0` means "indeterminate".
    pub fn progress(&self) -> (i64, i64) {
        (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed))
    }
}

/// The handle a job body uses to talk to the system. Cheap to pass by reference.
pub struct JobContext {
    pub state: SharedState,
    handle: Arc<RunHandle>,
}

impl JobContext {
    pub(super) fn new(state: SharedState, handle: Arc<RunHandle>) -> Self {
        Self { state, handle }
    }

    /// Whether an admin has requested cancellation. Long jobs should poll this
    /// between units of work and return early (returning `Ok(())` → the run is
    /// recorded as `cancelled`).
    pub fn cancelled(&self) -> bool {
        self.handle.is_cancelled()
    }

    /// Report progress. `total == 0` renders as an indeterminate/among-N bar.
    /// DB + WS writes are throttled to ~1/s; the in-memory value updates every
    /// call so the API always sees the latest.
    pub fn progress(&self, done: usize, total: usize) {
        self.handle.done.store(done as i64, Ordering::Relaxed);
        self.handle.total.store(total as i64, Ordering::Relaxed);
        let now = now_ms();
        let last = self.handle.last_flush_ms.load(Ordering::Relaxed);
        // Always flush the terminal (done == total) update; otherwise rate-limit.
        let terminal = total > 0 && done >= total;
        if !terminal && now - last < 1000 {
            return;
        }
        self.handle.last_flush_ms.store(now, Ordering::Relaxed);
        let pool = self.state.db.clone();
        let (rid, d, t) = (self.handle.run_id.clone(), done as i64, total as i64);
        let _ = db::update_job_run_progress(&pool, &rid, d, t);
        self.state.events.publish(ServerEvent::JobProgress {
            key: self.handle.key.clone(),
            run_id: self.handle.run_id.clone(),
            done,
            total,
        });
    }

    /// Append a log line (persisted, streamed over the WS bus, and mirrored to
    /// the server's own tracing log). `level` is `"debug" | "info" | "warn" |
    /// "error"`. All levels persist so the admin Tâches run view can show the
    /// full story (debug reasoning, warnings, and errors), not just `info`.
    pub fn log(&self, level: &'static str, message: impl Into<String>) {
        let message = message.into();
        let ts = now_ms();
        let pool = self.state.db.clone();
        let _ = db::insert_job_log(&pool, &self.handle.run_id, ts, level, &message);
        match level {
            "error" => tracing::error!(job = %self.handle.key, run = %self.handle.run_id, "{message}"),
            "warn" => tracing::warn!(job = %self.handle.key, run = %self.handle.run_id, "{message}"),
            "debug" => tracing::debug!(job = %self.handle.key, run = %self.handle.run_id, "{message}"),
            _ => tracing::info!(job = %self.handle.key, run = %self.handle.run_id, "{message}"),
        }
        self.state.events.publish(ServerEvent::JobLog {
            run_id: self.handle.run_id.clone(),
            level,
            message,
        });
    }

    /// Verbose detail for diagnosing a run (skip reasons, request/response sizes,
    /// per-item outcomes). Persisted + shown in the Tâches log, tagged `debug`.
    pub fn debug(&self, message: impl Into<String>) {
        self.log("debug", message);
    }

    /// An owned `debug`-level logger that outlives a borrow of `self` for
    /// helpers run within the job that log on their own (e.g. the LLM connector's
    /// per-tool-call lines). Captures cloned handles, so it writes to this same
    /// run exactly like [`debug`](Self::debug).
    pub fn debug_logger(&self) -> Box<dyn Fn(String) + Send + Sync> {
        let pool = self.state.db.clone();
        let events = self.state.events.clone();
        let run_id = self.handle.run_id.clone();
        Box::new(move |message: String| {
            let ts = now_ms();
            let _ = db::insert_job_log(&pool, &run_id, ts, "debug", &message);
            tracing::debug!(run = %run_id, "{message}");
            events.publish(ServerEvent::JobLog { run_id: run_id.clone(), level: "debug", message });
        })
    }

    pub fn info(&self, message: impl Into<String>) {
        self.log("info", message);
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.log("warn", message);
    }

    /// A genuine failure within the run (an LLM call errored, a reply wouldn't
    /// parse). The run can still complete; this surfaces *why* something was
    /// skipped instead of swallowing it.
    pub fn error(&self, message: impl Into<String>) {
        self.log("error", message);
    }
}
