use super::*;
use super::ops::{Subject, MAX_ATTEMPTS};
use crate::db::Pool;
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

fn pool() -> Pool {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("luma-pipe-{}-{n}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    crate::db::init(&path).unwrap()
}

/// `(pending, running, done, failed, blocked)` for the test stage.
fn c(p: &Pool) -> (i64, i64, i64, i64, i64) {
    counts(p, "s").unwrap()
}

fn subj(pairs: &[(&str, &str)]) -> Vec<Subject> {
    pairs.iter().map(|(id, sig)| (id.to_string(), sig.to_string())).collect()
}

#[test]
fn reconcile_is_incremental_and_idempotent() {
    let p = pool();
    let s = subj(&[("a", "v1"), ("b", "v1")]);

    // First reconcile: both subjects become pending.
    reconcile(&p, "s", "item", &s, 1).unwrap();
    assert_eq!(c(&p), (2, 0, 0, 0, 0));

    // Claim + succeed both -> done.
    let batch = claim_batch(&p, "s", 10, 2).unwrap();
    assert_eq!(batch.len(), 2);
    let ok: Vec<TaskResult> =
        batch.iter().map(|(id, _)| TaskResult { id: id.clone(), error: None, duration_ms: 1 }).collect();
    finish_batch(&p, "s", &ok, 3).unwrap();
    assert_eq!(c(&p), (0, 0, 2, 0, 0));

    // Re-reconcile with UNCHANGED signatures: nothing re-queued (the whole
    // point the heavy work is skipped on a re-run).
    reconcile(&p, "s", "item", &s, 4).unwrap();
    assert_eq!(c(&p), (0, 0, 2, 0, 0));
    assert!(claim_batch(&p, "s", 10, 5).unwrap().is_empty());

    // Change one signature: only that subject is re-queued.
    reconcile(&p, "s", "item", &subj(&[("a", "v2"), ("b", "v1")]), 6).unwrap();
    assert_eq!(c(&p), (1, 0, 1, 0, 0));
    let re = claim_batch(&p, "s", 10, 7).unwrap();
    assert_eq!(re, vec![("a".to_string(), "v2".to_string())]);

    // A disappeared subject is purged.
    reconcile(&p, "s", "item", &subj(&[("b", "v1")]), 8).unwrap();
    let all: i64 = {
        let conn = p.get().unwrap();
        conn.query_row("SELECT COUNT(*) FROM pipeline_tasks WHERE subject_id='a'", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(all, 0);
}

#[test]
fn failures_retry_up_to_max_then_stick() {
    let p = pool();
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 1).unwrap();

    // Fail it MAX_ATTEMPTS times; each reconcile re-queues it while under the
    // cap, and it stops being retried once the cap is reached.
    for i in 0..MAX_ATTEMPTS {
        let batch = claim_batch(&p, "s", 10, 10 + i).unwrap();
        assert_eq!(batch.len(), 1, "attempt {i} should have a pending task to claim");
        finish_batch(
            &p,
            "s",
            &[TaskResult { id: "a".into(), error: Some("boom".into()), duration_ms: 1 }],
            20 + i,
        )
        .unwrap();
        reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 30 + i).unwrap();
    }
    // Cap reached: failed, and no longer auto-retried.
    assert_eq!(c(&p), (0, 0, 0, 1, 0));
    assert!(claim_batch(&p, "s", 10, 99).unwrap().is_empty());

    // A manual retry clears it back to pending regardless of attempts.
    assert_eq!(retry(&p, "s", Some("a")).unwrap(), 1);
    assert_eq!(c(&p), (1, 0, 0, 0, 0));
}

#[test]
fn manual_retry_jumps_the_queue() {
    let p = pool();
    // A routine pending task (priority 0)...
    reconcile(&p, "s", "item", &subj(&[("routine", "v1")]), 1).unwrap();
    // ...and a failed task sitting in the backlog.
    {
        let conn = p.get().unwrap();
        conn.execute(
            "INSERT INTO pipeline_tasks(stage,subject_kind,subject_id,status,attempts,priority,enqueued_at,updated_at) \
             VALUES ('s','item','boom','failed',1,0,0,0)",
            [],
        )
        .unwrap();
    }
    // Manual retry bumps it above the routine backlog.
    assert_eq!(retry(&p, "s", Some("boom")).unwrap(), 1);
    // The very next claim returns the retried task first.
    let batch = claim_batch(&p, "s", 1, 5).unwrap();
    assert_eq!(batch.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["boom"]);
}

#[test]
fn enqueue_null_sig_is_backfilled_not_rerun() {
    let p = pool();
    // `enqueue` (the reprocess fast-track) inserts with input_sig = NULL.
    enqueue(&p, "s", "item", "a", 100, 1).unwrap();
    let batch = claim_batch(&p, "s", 10, 2).unwrap();
    assert_eq!(batch.len(), 1);
    finish_batch(&p, "s", &[TaskResult { id: "a".into(), error: None, duration_ms: 1 }], 3)
        .unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));

    // Reconcile with the current signature: a NULL old sig is backfilled, NOT
    // treated as changed, so the just-finished task stays done (no re-run).
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 4).unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));
    assert!(claim_batch(&p, "s", 10, 5).unwrap().is_empty());

    // The sig is now stored, so the next reconcile is still a no-op...
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 6).unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));
    // ...while a genuine change still re-queues it.
    reconcile(&p, "s", "item", &subj(&[("a", "v2")]), 7).unwrap();
    assert_eq!(c(&p), (1, 0, 0, 0, 0));
}

#[test]
fn reset_running_recovers_stranded_tasks() {
    let p = pool();
    reconcile(&p, "s", "item", &subj(&[("a", "v1"), ("b", "v1")]), 1).unwrap();
    claim_batch(&p, "s", 10, 2).unwrap(); // both -> running
    assert_eq!(c(&p), (0, 2, 0, 0, 0));
    assert_eq!(reset_running(&p, None).unwrap(), 2);
    assert_eq!(c(&p), (2, 0, 0, 0, 0));
}

#[test]
fn unreadable_signature_never_requeues_or_deletes() {
    let p = pool();
    // Process one subject to `done`.
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 1).unwrap();
    let batch = claim_batch(&p, "s", 10, 2).unwrap();
    let ok: Vec<TaskResult> =
        batch.iter().map(|(id, _)| TaskResult { id: id.clone(), error: None, duration_ms: 1 }).collect();
    finish_batch(&p, "s", &ok, 3).unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));

    // Mount blip: the file is unreadable this pass. The done task must be left
    // alone (not re-queued) and must survive (not purged as "gone").
    reconcile(&p, "s", "item", &subj(&[("a", UNREADABLE_SIG)]), 4).unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));
    assert!(claim_batch(&p, "s", 10, 5).unwrap().is_empty());

    // Mount back with the SAME real signature: still a no-op (stored sig was never
    // overwritten by the sentinel), so no wasteful recompute.
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 6).unwrap();
    assert_eq!(c(&p), (0, 0, 1, 0, 0));
    assert!(claim_batch(&p, "s", 10, 7).unwrap().is_empty());
}

#[test]
fn requeue_stage_rebuilds_done_tasks_after_cache_wipe() {
    let p = pool();
    // Two subjects -> done.
    reconcile(&p, "s", "item", &subj(&[("a", "v1"), ("b", "v1")]), 1).unwrap();
    let batch = claim_batch(&p, "s", 10, 2).unwrap();
    let ok: Vec<TaskResult> =
        batch.iter().map(|(id, _)| TaskResult { id: id.clone(), error: None, duration_ms: 1 }).collect();
    finish_batch(&p, "s", &ok, 3).unwrap();
    assert_eq!(c(&p), (0, 0, 2, 0, 0));

    // Outputs wiped out of band: re-queue the whole stage. Both go back to pending
    // and are claimable again even though their signatures never changed.
    assert_eq!(requeue_stage(&p, "s", 4).unwrap(), 2);
    assert_eq!(c(&p), (2, 0, 0, 0, 0));
    assert_eq!(claim_batch(&p, "s", 10, 5).unwrap().len(), 2);
}

#[test]
fn unreadable_brand_new_subject_is_deferred_not_inserted() {
    let p = pool();
    // A never-seen subject that is unreadable right now creates no task...
    reconcile(&p, "s", "item", &subj(&[("a", UNREADABLE_SIG)]), 1).unwrap();
    assert_eq!(c(&p), (0, 0, 0, 0, 0));
    // ...and is picked up as pending once it becomes readable.
    reconcile(&p, "s", "item", &subj(&[("a", "v1")]), 2).unwrap();
    assert_eq!(c(&p), (1, 0, 0, 0, 0));
}
