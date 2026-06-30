//! The API read models for the job console (`list` / `detail` / `info_for`)
//! split out of [`super`] to keep the manager file focused. `impl JobManager` in
//! a sibling file; same-module privacy lets it read the manager's private state.

use time::OffsetDateTime;

use super::{now_local, Cron, JobManager, RUNS_KEPT};
use crate::db;
use crate::model::{JobDetail, JobId, JobInfo};
use crate::state::SharedState;

impl JobManager {
    /// All jobs as wire-ready [`JobInfo`], in registration order.
    pub fn list(&self, state: &SharedState) -> Vec<JobInfo> {
        let now = now_local(state);
        let pool = state.db.clone();
        self.order.iter().filter_map(|&id| self.info_for(&pool, now, id)).collect()
    }

    /// One job plus its recent run history.
    pub fn detail(&self, state: &SharedState, id: JobId) -> Option<JobDetail> {
        let now = now_local(state);
        let pool = state.db.clone();
        let info = self.info_for(&pool, now, id)?;
        let runs = db::list_job_runs(&pool, id.key(), RUNS_KEPT).unwrap_or_default();
        Some(JobDetail { info, runs })
    }

    /// Build the wire [`JobInfo`] for one job: static metadata + effective
    /// schedule + next fire time + live run progress.
    fn info_for(&self, pool: &db::Pool, now: OffsetDateTime, id: JobId) -> Option<JobInfo> {
        let registered = self.jobs.get(&id)?;
        let st = self.schedules.read().unwrap().get(&id).cloned()?;

        // Next scheduled fire, if enabled and on a (valid) schedule.
        let next_run_at = if st.enabled {
            st.schedule
                .as_deref()
                .and_then(|e| Cron::parse(e).ok())
                .and_then(|c| c.next_after(now))
                .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
        } else {
            None
        };

        let running = self.running.read().unwrap().get(&id).cloned();
        let (progress_done, progress_total) = match &running {
            Some(h) => {
                let (d, t) = h.progress();
                (Some(d), Some(t))
            }
            None => (None, None),
        };

        Some(JobInfo {
            key: id,
            name: format!("jobs.{}.name", id.key()),
            description: format!("jobs.{}.desc", id.key()),
            category: registered.category,
            schedule: st.schedule.clone(),
            default_schedule: registered.default_schedule.map(str::to_string),
            customized: st.customized,
            enabled: st.enabled,
            running: running.is_some(),
            run_id: running.as_ref().map(|h| h.run_id.clone()),
            progress_done,
            progress_total,
            next_run_at,
            last_run: db::last_job_run(pool, id.key()).ok().flatten(),
        })
    }
}
