//! The stage driver. One call to [`run`] does a whole stage-drain: reconcile the
//! ledger against the current catalog, then claim -> process -> record in
//! batches until the queue is empty or the run is cancelled.
//!
//! Concurrency model: this runs on the job's blocking thread (the "dispatcher").
//! It owns every `pipeline_tasks` write (claims + finishes, always batched into
//! one transaction) so the many in-memory workers never contend on SQLite's
//! single writer. The workers only do the heavy ffmpeg / chromaprint / TMDB work
//! via `stage.process`, which keeps its own established DB-write pattern.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::db;
use crate::infra::events::ServerEvent;
use crate::services::jobs::{now_ms, JobContext};

use super::stage::Stage;

/// Tasks claimed + recorded per iteration. Small enough that a cancel is observed
/// promptly and progress advances smoothly; large enough that the per-batch DB
/// round-trips are negligible next to the ffmpeg/TMDB work.
const BATCH: usize = 32;
/// Poll interval while paused for active playback.
const PAUSE_POLL_S: u64 = 4;

/// How often the drain logs a progress line (with elapsed + ETA) during a long
/// run, so a multi-minute stage isn't silent between "in scope" and "finished".
const LOG_EVERY_MS: i64 = 10_000;

/// Drain one stage to completion (or cancellation).
pub fn run(stage: &Stage, ctx: &JobContext) -> Result<()> {
    let pool = &ctx.state.db;
    let started = Instant::now();

    // 0. Reclaim orphans. One-run-per-key guarantees no other drain of this
    //    stage is live, so any `running` row at this point was stranded by an
    //    earlier drain that died mid-batch (a claim/finish DB error, a hang, a
    //    crash). Without this, such rows stay `running` until a server restart:
    //    `reconcile` deliberately never touches them.
    match db::pipeline::reset_running(pool, Some(stage.short)) {
        Ok(0) => {}
        Ok(n) => ctx.warn(format!(
            "{}: re-queued {n} task(s) left running by an interrupted earlier drain",
            stage.short
        )),
        Err(e) => {
            ctx.warn(format!("{}: failed to reclaim stranded tasks: {e:#}", stage.short))
        }
    }

    // 1. Reconcile: fold the current subject set into the ledger (new/changed ->
    //    pending, gone -> deleted, transient failures -> retried).
    let subjects = (stage.enumerate)(&ctx.state)?;
    db::pipeline::reconcile(pool, stage.short, stage.subject_kind, &subjects, now_ms())?;
    ctx.info(format!(
        "{}: {} subject(s) in scope (scanned in {})",
        stage.short,
        subjects.len(),
        fmt_dur(started.elapsed()),
    ));

    // 2. Drain. The pending count after reconcile is the progress denominator;
    //    high-priority enqueues arriving mid-run just extend it (progress is
    //    clamped so the bar never exceeds 100%). The cleanup below runs whether
    //    the loop finished, was cancelled, or aborted on a DB error: skipping
    //    `reset_running` on the error path would leave the whole claimed batch
    //    sitting `running` until the next drain.
    let total = pending_count(pool, stage.short)?;
    if total == 0 {
        ctx.info(format!("{}: nothing to do (already up to date)", stage.short));
        return Ok(());
    }
    ctx.info(format!("{}: draining {total} pending task(s)…", stage.short));

    let drained = drain_loop(stage, ctx, total);

    // A mid-batch cancel or an aborted loop can leave tasks claimed-but-
    // unprocessed: flip any leftover `running` for this stage back to `pending`
    // so they aren't stranded. Runs on the error path too (see above).
    if let Err(e) = db::pipeline::reset_running(pool, Some(stage.short)) {
        ctx.warn(format!("{}: failed to reset leftover running tasks: {e:#}", stage.short));
    }
    emit_stats(stage, ctx); // final authoritative push
    drained?;
    let (_pending, _running, done, failed, _blocked) = db::pipeline::counts(pool, stage.short)?;
    ctx.info(format!(
        "{}: finished in {} - {done} done, {failed} failed",
        stage.short,
        fmt_dur(started.elapsed()),
    ));
    Ok(())
}

/// The claim -> process -> record loop of [`run`]. Split out so the caller can
/// guarantee cleanup on every exit path, including a `?` on a DB error here.
fn drain_loop(stage: &Stage, ctx: &JobContext, total: usize) -> Result<()> {
    let pool = &ctx.state.db;
    let drain_started = Instant::now();
    let mut processed = 0usize;
    let mut failed_seen = 0usize;
    let mut stats_flush_ms = 0i64;
    let mut log_flush_ms = now_ms();
    let mut hold_logged = false;
    loop {
        if ctx.cancelled() {
            ctx.info(format!(
                "{}: cancelled after {processed}/{total} in {}",
                stage.short,
                fmt_dur(drain_started.elapsed()),
            ));
            break;
        }
        // Global pause: park the whole drain BEFORE claiming, so a paused pipeline
        // holds nothing `running` (in-flight batches also yield per item below).
        while ctx.state.jobs.pipeline_paused() && !ctx.cancelled() {
            if !hold_logged {
                ctx.info(format!("{}: paused (pipeline held by admin)", stage.short));
                hold_logged = true;
            }
            thread::sleep(Duration::from_secs(PAUSE_POLL_S));
        }
        if ctx.cancelled() {
            ctx.info(format!("{}: cancelled while paused", stage.short));
            break;
        }
        if hold_logged {
            ctx.info(format!("{}: resumed", stage.short));
            hold_logged = false;
        }
        let batch = db::pipeline::claim_batch(pool, stage.short, BATCH, now_ms())?;
        if batch.is_empty() {
            break;
        }
        let results = process_batch(stage, ctx, &batch);
        db::pipeline::finish_batch(pool, stage.short, &results, now_ms())?;
        processed += results.len();
        failed_seen += results.iter().filter(|r| r.error.is_some()).count();
        ctx.progress(processed.min(total), total);
        maybe_emit_stats(stage, ctx, &mut stats_flush_ms);
        maybe_log_progress(ctx, stage.short, processed, total, failed_seen, drain_started, &mut log_flush_ms);
    }
    Ok(())
}

/// Human-readable elapsed time (`820 ms` · `4.3 s` · `2 min 05 s` · `1 h 07 min`).
fn fmt_dur(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let secs = d.as_secs();
    if secs < 60 {
        return format!("{:.1} s", d.as_secs_f64());
    }
    let (m, s) = (secs / 60, secs % 60);
    if m < 60 {
        format!("{m} min {s:02} s")
    } else {
        format!("{} h {:02} min", m / 60, m % 60)
    }
}

/// Throttled progress line during a drain: `storyboard: 605/4146, 0 failed ·
/// 3 min 12 s elapsed · ~18 min 40 s left`.
fn maybe_log_progress(
    ctx: &JobContext,
    short: &str,
    processed: usize,
    total: usize,
    failed: usize,
    drain_started: Instant,
    last_ms: &mut i64,
) {
    let now = now_ms();
    if now - *last_ms < LOG_EVERY_MS {
        return;
    }
    *last_ms = now;
    let elapsed = drain_started.elapsed();
    let rate = processed as f64 / elapsed.as_secs_f64().max(0.001);
    let remaining = total.saturating_sub(processed);
    let eta = if rate > 0.0 {
        fmt_dur(Duration::from_secs_f64((remaining as f64 / rate).min(1e8)))
    } else {
        "?".to_string()
    };
    ctx.info(format!(
        "{short}: {processed}/{total}, {failed} failed · {} elapsed · ~{eta} left",
        fmt_dur(elapsed),
    ));
}

/// Pending + still-running tasks after reconcile = the drain's denominator.
fn pending_count(pool: &db::Pool, stage: &str) -> Result<usize> {
    let (pending, running, ..) = db::pipeline::counts(pool, stage)?;
    Ok((pending + running).max(0) as usize)
}

/// Process a claimed batch on a bounded worker pool, honoring cancellation and
/// the playback-priority pause. Returns one [`db::pipeline::TaskResult`] per
/// task actually processed (a cancel mid-batch may leave some unprocessed;
/// those stay `running` and are reset by the caller).
fn process_batch(
    stage: &Stage,
    ctx: &JobContext,
    batch: &[(String, String)],
) -> Vec<db::pipeline::TaskResult> {
    let next = AtomicUsize::new(0);
    let paused = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<db::pipeline::TaskResult>>> =
        (0..batch.len()).map(|_| Mutex::new(None)).collect();
    // Hardware clamp on top of the per-stage setting: a stage tuned on a dev
    // machine (metadata: 8, probe: 4) must not oversubscribe a 2-core NAS.
    let cores = thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(4);
    let workers = stage
        .concurrency
        .min(cores * 2)
        .max(1)
        .min(batch.len().max(1));
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| process_task_worker(&next, batch, ctx, &paused, &slots, stage));
        }
    });
    slots.into_iter().filter_map(|m| m.into_inner().unwrap()).collect()
}

/// One scoped worker: pull the next task index off the batch until it's drained
/// (or cancelled), yielding to the pause/playback hold before each item, and
/// store its [`db::pipeline::TaskResult`] (a panic in `process` is recorded like
/// a returned `Err` so it can never unwind out of the scope).
fn process_task_worker(
    next: &AtomicUsize,
    batch: &[(String, String)],
    ctx: &JobContext,
    paused: &AtomicBool,
    slots: &[Mutex<Option<db::pipeline::TaskResult>>],
    stage: &Stage,
) {
    loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        if i >= batch.len() || ctx.cancelled() {
            break;
        }
        // Yield per item to the global pause (all stages) and, for the
        // playback-sensitive stages, to a live stream. Keeps an in-flight batch
        // from starting new ffmpeg the moment either fires.
        wait_while_held(ctx, paused, stage.pause_for_playback);
        if ctx.cancelled() {
            break;
        }
        let (id, _sig) = &batch[i];
        let started = Instant::now();
        // Catch a panic in `process` so one bad file can't unwind out of the
        // scope and skip `finish_batch`/`reset_running`, wedging the whole
        // claimed batch as `running`. A panic is recorded like a returned Err.
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (stage.process)(ctx, id)));
        let duration_ms = started.elapsed().as_millis() as i64;
        let error = match outcome {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(format!("{e:#}")),
            Err(_) => Some("panicked during processing".to_string()),
        };
        *slots[i].lock().unwrap() =
            Some(db::pipeline::TaskResult { id: id.clone(), error, duration_ms });
    }
}

/// Block while heavy work should hold off: the global pipeline pause is set, or
/// (for a playback-sensitive stage) a stream is live. Logs the hold/resume
/// transition once per worker (CAS on `paused`). Generalizes the old
/// markers/storyboards playback-yield to also honor the admin pause switch.
fn wait_while_held(ctx: &JobContext, paused: &AtomicBool, pause_for_playback: bool) {
    loop {
        if ctx.cancelled() {
            return;
        }
        let admin_hold = ctx.state.jobs.pipeline_paused();
        let playback_hold = pause_for_playback && !ctx.state.playback.list().is_empty();
        if !admin_hold && !playback_hold {
            if paused.swap(false, Ordering::Relaxed) {
                ctx.info("resuming");
            }
            return;
        }
        if !paused.swap(true, Ordering::Relaxed) {
            ctx.info(hold_reason(admin_hold));
        }
        thread::sleep(Duration::from_secs(PAUSE_POLL_S));
    }
}

/// The log line for a hold, depending on which side asked for it.
fn hold_reason(admin_hold: bool) -> &'static str {
    if admin_hold {
        "paused (pipeline held by admin)"
    } else {
        "playback active, pausing (playback has priority)"
    }
}

/// Publish this stage's counts, throttled to ~1/s (the WS event is cheap but the
/// count query is a round-trip; no need to spam it every batch).
fn maybe_emit_stats(stage: &Stage, ctx: &JobContext, last_ms: &mut i64) {
    let now = now_ms();
    if now - *last_ms < 1000 {
        return;
    }
    *last_ms = now;
    emit_stats(stage, ctx);
}

fn emit_stats(stage: &Stage, ctx: &JobContext) {
    if let Ok(stat) =
        db::pipeline::stage_stat(&ctx.state.db, stage.short, stage.key, stage.subject_kind)
    {
        ctx.state.events.publish(ServerEvent::PipelineStats { stages: vec![stat] });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_dur_scales_units() {
        assert_eq!(fmt_dur(Duration::from_millis(820)), "820 ms");
        assert_eq!(fmt_dur(Duration::from_millis(999)), "999 ms");
        assert_eq!(fmt_dur(Duration::from_millis(4300)), "4.3 s");
        assert_eq!(fmt_dur(Duration::from_secs(59)), "59.0 s");
        assert_eq!(fmt_dur(Duration::from_secs(125)), "2 min 05 s");
        assert_eq!(fmt_dur(Duration::from_secs(3600 + 7 * 60)), "1 h 07 min");
    }

    #[test]
    fn hold_reason_depends_on_source() {
        assert_eq!(hold_reason(true), "paused (pipeline held by admin)");
        assert_eq!(hold_reason(false), "playback active, pausing (playback has priority)");
    }

    #[test]
    fn fmt_dur_boundaries() {
        // Exact one-second and one-minute boundaries flip units.
        assert_eq!(fmt_dur(Duration::from_secs(1)), "1.0 s");
        assert_eq!(fmt_dur(Duration::from_secs(60)), "1 min 00 s");
        assert_eq!(fmt_dur(Duration::from_secs(3600)), "1 h 00 min");
    }

    fn test_pool() -> db::Pool {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-disp-test-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        db::init(&path).unwrap()
    }

    #[test]
    fn pending_count_sums_pending_and_running() {
        let pool = test_pool();
        // An empty ledger for a stage counts zero.
        assert_eq!(pending_count(&pool, "probe").unwrap(), 0);
        // Freshly-enqueued tasks are pending, so they are counted.
        db::pipeline::enqueue(&pool, "probe", "file", "f1", 100, now_ms()).unwrap();
        db::pipeline::enqueue(&pool, "probe", "file", "f2", 100, now_ms()).unwrap();
        assert_eq!(pending_count(&pool, "probe").unwrap(), 2);
        // A different stage's queue is independent.
        assert_eq!(pending_count(&pool, "metadata").unwrap(), 0);
    }
}
