//! Persistence for the background job system: schedule overrides, run records
//! and per-run log lines. See `crate::services::jobs`. All timestamps are
//! epoch milliseconds.

use super::*;

use kroma_domain::{JobLog, JobRun};

const RUN_COLS: &str = "id,job_key,trigger_kind,status,started_at,finished_at,\
    progress_done,progress_total,error";

fn row_to_run(r: &Row) -> rusqlite::Result<JobRun> {
    let started_at: i64 = r.get(4)?;
    let finished_at: Option<i64> = r.get(5)?;
    Ok(JobRun {
        id: r.get(0)?,
        job_key: r.get(1)?,
        trigger: r.get(2)?,
        status: r.get(3)?,
        started_at,
        finished_at,
        duration_ms: finished_at.map(|f| f - started_at),
        progress_done: r.get(6)?,
        progress_total: r.get(7)?,
        error: r.get(8)?,
    })
}

// ----- schedules --------------------------------------------------------------

/// Every persisted schedule override as `(key, schedule, enabled)`.
pub fn list_job_schedules(pool: &Pool) -> Result<Vec<(String, Option<String>, bool)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT key, schedule, enabled FROM job_schedules")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)? != 0))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Insert or update one job's schedule override.
pub fn upsert_job_schedule(pool: &Pool, key: &str, schedule: Option<&str>, enabled: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO job_schedules (key, schedule, enabled, updated_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(key) DO UPDATE SET \
            schedule=excluded.schedule, enabled=excluded.enabled, updated_at=excluded.updated_at",
        params![key, schedule, enabled as i64, kroma_primitives::now_ms()],
    )?;
    Ok(())
}

// ----- runs -------------------------------------------------------------------

/// Record a freshly-started run (status = `running`).
pub fn insert_job_run(pool: &Pool, id: &str, key: &str, trigger: &str, started_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO job_runs (id, job_key, trigger_kind, status, started_at) \
         VALUES (?1, ?2, ?3, 'running', ?4)",
        params![id, key, trigger, started_ms],
    )?;
    Ok(())
}

/// Update a run's live progress counters.
pub fn update_job_run_progress(pool: &Pool, id: &str, done: i64, total: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE job_runs SET progress_done=?2, progress_total=?3 WHERE id=?1",
        params![id, done, total],
    )?;
    Ok(())
}

/// Mark a run finished with its terminal status (+ error message when failed).
pub fn finish_job_run(pool: &Pool, id: &str, status: &str, finished_ms: i64, error: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE job_runs SET status=?2, finished_at=?3, error=?4 WHERE id=?1",
        params![id, status, finished_ms, error],
    )?;
    Ok(())
}

/// Recent runs for a job, newest first.
pub fn list_job_runs(pool: &Pool, key: &str, limit: usize) -> Result<Vec<JobRun>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {RUN_COLS} FROM job_runs WHERE job_key=?1 ORDER BY started_at DESC LIMIT ?2",
    ))?;
    let rows = stmt.query_map(params![key, limit as i64], row_to_run)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The most recent run for a job, if any.
pub fn last_job_run(pool: &Pool, key: &str) -> Result<Option<JobRun>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {RUN_COLS} FROM job_runs WHERE job_key=?1 ORDER BY started_at DESC LIMIT 1",
    ))?;
    let mut rows = stmt.query_map(params![key], row_to_run)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// Mark every run still flagged `running` as `failed` called once at startup
/// to clean up rows stranded by a crash / kill / OOM mid-run. In-process run
/// state doesn't survive a restart, so such a row would otherwise stay `running`
/// (with a NULL `finished_at`/duration) forever. Returns how many were fixed.
pub fn reconcile_running_runs(pool: &Pool) -> Result<usize> {
    let conn = pool.get()?;
    let now = kroma_primitives::now_ms();
    // Collect the stranded ids first so we can leave an explanatory line in each
    // run's *own* log otherwise the Tâches view shows a run that just stops
    // mid-stream with no reason. This is not a job error: a restart (in dev,
    // cargo-watch rebuilds; in prod, an upgrade/crash) killed it mid-run.
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM job_runs WHERE status='running'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for id in &ids {
        let _ = conn.execute(
            "INSERT INTO job_logs (run_id, ts, level, message) VALUES (?1, ?2, 'error', ?3)",
            params![
                id,
                now,
                "run interrupted by a server restart before it could finish (not a job error); trigger it again to build the remaining work"
            ],
        );
    }
    let n = conn.execute(
        "UPDATE job_runs SET status='failed', finished_at=?1, \
         error='interrupted by server restart' WHERE status='running'",
        params![now],
    )?;
    Ok(n)
}

/// Drop all but the newest `keep` runs for a job, plus any now-orphaned logs.
pub fn prune_job_runs(pool: &Pool, key: &str, keep: usize) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM job_runs WHERE job_key=?1 AND id NOT IN \
         (SELECT id FROM job_runs WHERE job_key=?1 ORDER BY started_at DESC LIMIT ?2)",
        params![key, keep as i64],
    )?;
    conn.execute(
        "DELETE FROM job_logs WHERE run_id NOT IN (SELECT id FROM job_runs)",
        [],
    )?;
    Ok(())
}

// ----- logs -------------------------------------------------------------------

/// Append one log line to a run.
pub fn insert_job_log(pool: &Pool, run_id: &str, ts: i64, level: &str, message: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO job_logs (run_id, ts, level, message) VALUES (?1, ?2, ?3, ?4)",
        params![run_id, ts, level, message],
    )?;
    Ok(())
}

/// The last `limit` log lines of a run, in chronological order.
pub fn list_job_logs(pool: &Pool, run_id: &str, limit: usize) -> Result<Vec<JobLog>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT ts, level, message FROM job_logs WHERE run_id=?1 ORDER BY rowid DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![run_id, limit as i64], |r| {
        Ok(JobLog { ts: r.get(0)?, level: r.get(1)?, message: r.get(2)? })
    })?;
    let mut logs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    logs.reverse(); // DESC fetch → chronological for display
    Ok(logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-jobs-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::init(&path).unwrap()
    }

    #[test]
    fn reconcile_flips_only_running_to_failed() {
        let p = pool();
        insert_job_run(&p, "r1", "cache.cleanup", "manual", 1).unwrap(); // stays running
        insert_job_run(&p, "r2", "cache.cleanup", "manual", 1).unwrap();
        finish_job_run(&p, "r2", "success", 2, None).unwrap(); // already done

        let n = reconcile_running_runs(&p).unwrap();
        assert_eq!(n, 1); // only r1

        let r1 = last_one(&p, "r1");
        assert_eq!(r1.status, "failed");
        assert!(r1.finished_at.is_some());
        assert!(r1.error.unwrap().contains("restart"));
        assert_eq!(last_one(&p, "r2").status, "success"); // untouched
    }

    fn last_one(p: &Pool, id: &str) -> JobRun {
        let conn = p.get().unwrap();
        conn.query_row(&format!("SELECT {RUN_COLS} FROM job_runs WHERE id=?1"), params![id], row_to_run).unwrap()
    }
}
