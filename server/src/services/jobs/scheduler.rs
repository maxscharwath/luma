//! The cron tick loop and due-job selection split out of [`super`] (the job
//! manager) to keep that file focused and to give the scheduler's boundary
//! arithmetic its own home + tests. These are `impl JobManager` methods in a
//! sibling file; same-module privacy lets them touch the manager's private state.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use time::OffsetDateTime;
use tracing::info;

use super::{now_local, Cron, JobManager, TriggerError};
use crate::model::JobId;
use crate::state::SharedState;

/// How often the scheduler wakes to fire due jobs. Any cron time that falls in
/// the `(previous tick, now]` window triggers, so this only bounds latency, not
/// correctness a minute-granularity schedule needs a tick below 60s.
const TICK: StdDuration = StdDuration::from_secs(30);

impl JobManager {
    /// Spawn the cron tick loop. Fires any schedule whose time falls in the
    /// `(last tick, now]` window so a server that was down does **not**
    /// retroactively run missed jobs, and the tick rate only bounds latency.
    pub fn spawn_scheduler(self: Arc<Self>, state: SharedState) {
        tokio::spawn(async move {
            let mut last = now_local(&state);
            let mut ticker = tokio::time::interval(TICK);
            // The immediate first tick establishes the baseline; nothing fires
            // until a scheduled time elapses after it.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let now = now_local(&state);
                let due = self.due_jobs(last, now);
                for id in due {
                    match self.trigger(state.clone(), id, "schedule") {
                        Ok(_) => {}
                        Err(TriggerError::AlreadyRunning) => {
                            info!(job = id.key(), "skipped scheduled run; previous run still active")
                        }
                        Err(TriggerError::Unknown) => {}
                    }
                }
                last = now;
            }
        });
    }

    /// Keys whose schedule fires within `(last, now]`. A fire-time exactly equal
    /// to `now` is included here and excluded from the next window (which starts
    /// strictly after `last == now` via [`Cron::next_after`]), so a boundary job
    /// fires exactly once.
    fn due_jobs(&self, last: OffsetDateTime, now: OffsetDateTime) -> Vec<JobId> {
        let map = self.schedules.read().unwrap();
        map.iter()
            .filter(|(_, st)| st.enabled)
            .filter_map(|(id, st)| {
                let expr = st.schedule.as_deref()?;
                let cron = Cron::parse(expr).ok()?;
                match cron.next_after(last) {
                    Some(fire) if fire <= now => Some(*id),
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Builtin, JobManager};
    use crate::model::{Category, JobId};
    use time::macros::datetime;
    use time::OffsetDateTime;

    /// A manager with one job ([`JobId::CacheCleanup`]) registered on `schedule`.
    fn with_job(schedule: Option<&'static str>) -> JobManager {
        let mut jm = JobManager::new();
        jm.register(&Builtin {
            id: JobId::CacheCleanup,
            category: Category::Maintenance,
            schedule,
            triggers: &[],
            run: |_| Ok(()),
        });
        jm
    }

    fn due(jm: &JobManager, last: OffsetDateTime, now: OffsetDateTime) -> Vec<JobId> {
        jm.due_jobs(last, now)
    }

    #[test]
    fn fires_once_across_the_boundary() {
        let jm = with_job(Some("0 4 * * *"));
        // 04:00 falls in (03:59:50, 04:00:10] → fires.
        assert_eq!(due(&jm, datetime!(2026-06-29 03:59:50 UTC), datetime!(2026-06-29 04:00:10 UTC)), [JobId::CacheCleanup]);
        // The immediately-following window must NOT re-fire it.
        assert!(due(&jm, datetime!(2026-06-29 04:00:10 UTC), datetime!(2026-06-29 04:00:40 UTC)).is_empty());
    }

    #[test]
    fn fire_exactly_at_now_is_included_once() {
        let jm = with_job(Some("0 4 * * *"));
        // A window ENDING exactly at the fire time includes it…
        assert_eq!(due(&jm, datetime!(2026-06-29 03:59:50 UTC), datetime!(2026-06-29 04:00:00 UTC)), [JobId::CacheCleanup]);
        // …and the next window STARTING at it excludes it (no double-fire).
        assert!(due(&jm, datetime!(2026-06-29 04:00:00 UTC), datetime!(2026-06-29 04:00:10 UTC)).is_empty());
    }

    #[test]
    fn manual_only_and_disabled_never_fire() {
        // No schedule (manual-only) → never due.
        let jm = with_job(None);
        assert!(due(&jm, datetime!(2026-06-29 00:00:00 UTC), datetime!(2026-06-30 00:00:00 UTC)).is_empty());

        // Disabled job → never due even inside its window.
        let jm = with_job(Some("0 4 * * *"));
        jm.schedules.write().unwrap().get_mut(&JobId::CacheCleanup).unwrap().enabled = false;
        assert!(due(&jm, datetime!(2026-06-29 03:59:50 UTC), datetime!(2026-06-29 04:00:10 UTC)).is_empty());
    }
}
