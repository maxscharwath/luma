//! Problem-report persistence: insert (any user), list newest-first (optionally
//! scoped to one reporter), triage-status transitions and delete. The list SELECT
//! joins `users` for the reporter's display name (the admin queue shows it).

use super::*;
use kroma_domain::{Report, ReportCategory, ReportStatus, ReportSubjectKind};

/// Columns of the report list SELECT (reporter username joined in). Positional
/// order must match [`row_to_report`].
const REPORT_COLS: &str = "r.id, r.subject_kind, r.subject_id, r.subject_title, r.category, \
    r.message, r.status, r.reported_by, u.username, r.resolved_by, r.resolved_at, \
    r.created_at, r.updated_at";

fn row_to_report(r: &Row) -> rusqlite::Result<Report> {
    let subject_kind: String = r.get(1)?;
    let category: String = r.get(4)?;
    let status: String = r.get(6)?;
    Ok(Report {
        id: r.get(0)?,
        subject_kind: ReportSubjectKind::parse(&subject_kind).unwrap_or(ReportSubjectKind::Movie),
        subject_id: r.get(2)?,
        subject_title: r.get(3)?,
        category: ReportCategory::parse(&category).unwrap_or(ReportCategory::Other),
        message: r.get(5)?,
        status: ReportStatus::parse(&status).unwrap_or(ReportStatus::Open),
        reported_by: r.get(7)?,
        reported_by_name: r.get(8)?,
        resolved_by: r.get(9)?,
        resolved_at: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

/// A report to insert (id minted by the caller; timestamps stamped here).
pub struct NewReport {
    pub id: String,
    pub subject_kind: ReportSubjectKind,
    pub subject_id: String,
    pub subject_title: String,
    pub category: ReportCategory,
    pub message: Option<String>,
    pub reported_by: Option<String>,
}

pub fn insert_report(pool: &Pool, report: &NewReport, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO reports \
         (id, subject_kind, subject_id, subject_title, category, message, status, reported_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, ?8, ?8)",
        params![
            report.id,
            report.subject_kind.as_str(),
            report.subject_id,
            report.subject_title,
            report.category.as_str(),
            report.message,
            report.reported_by,
            now_ms
        ],
    )?;
    Ok(())
}

/// All reports newest-first, optionally scoped to one reporter (the user-facing
/// "my reports" list).
pub fn list_reports(conn: &Connection, only_user: Option<&str>) -> rusqlite::Result<Vec<Report>> {
    let base =
        format!("SELECT {REPORT_COLS} FROM reports r LEFT JOIN users u ON u.id = r.reported_by");
    match only_user {
        Some(uid) => {
            let mut stmt =
                conn.prepare(&format!("{base} WHERE r.reported_by = ?1 ORDER BY r.created_at DESC"))?;
            let rows = stmt.query_map(params![uid], row_to_report)?;
            rows.collect()
        }
        None => {
            let mut stmt = conn.prepare(&format!("{base} ORDER BY r.created_at DESC"))?;
            let rows = stmt.query_map([], row_to_report)?;
            rows.collect()
        }
    }
}

pub fn get_report(conn: &Connection, id: &str) -> rusqlite::Result<Option<Report>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REPORT_COLS} FROM reports r LEFT JOIN users u ON u.id = r.reported_by WHERE r.id = ?1"
    ))?;
    let mut rows = stmt.query_map(params![id], row_to_report)?;
    rows.next().transpose()
}

/// Transition a report's triage status. Moving to a terminal state records the
/// acting admin + timestamp; reopening (`Open`) clears both, so the resolver
/// fields always reflect the current state. Returns false when the id is absent.
pub fn set_report_status(
    pool: &Pool,
    id: &str,
    status: ReportStatus,
    actor: Option<&str>,
    now_ms: i64,
) -> Result<bool> {
    let conn = pool.get()?;
    let (resolved_by, resolved_at) = match status {
        ReportStatus::Open => (None, None),
        _ => (actor, Some(now_ms)),
    };
    let n = conn.execute(
        "UPDATE reports SET status = ?2, resolved_by = ?3, resolved_at = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, status.as_str(), resolved_by, resolved_at, now_ms],
    )?;
    Ok(n > 0)
}

/// Delete a report. Returns false when absent.
pub fn delete_report(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    Ok(conn.execute("DELETE FROM reports WHERE id = ?1", params![id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-report-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::init(&path).unwrap()
    }

    fn new_report(id: &str, kind: ReportSubjectKind, category: ReportCategory) -> NewReport {
        NewReport {
            id: id.into(),
            subject_kind: kind,
            subject_id: format!("{id}-subj"),
            subject_title: "Title".into(),
            category,
            message: Some("broken".into()),
            reported_by: None,
        }
    }

    #[test]
    fn insert_list_and_get_roundtrip() {
        let p = pool();
        insert_report(&p, &new_report("r1", ReportSubjectKind::Movie, ReportCategory::Audio), 1000)
            .unwrap();
        insert_report(&p, &new_report("r2", ReportSubjectKind::Show, ReportCategory::Metadata), 2000)
            .unwrap();

        let conn = p.get().unwrap();
        let all = list_reports(&conn, None).unwrap();
        assert_eq!(all.len(), 2);
        // Newest-first.
        assert_eq!(all[0].id, "r2");
        assert_eq!(all[0].category, ReportCategory::Metadata);
        assert_eq!(all[0].status, ReportStatus::Open);

        let one = get_report(&conn, "r1").unwrap().unwrap();
        assert_eq!(one.subject_kind, ReportSubjectKind::Movie);
        assert_eq!(one.message.as_deref(), Some("broken"));
        assert!(get_report(&conn, "ghost").unwrap().is_none());
    }

    #[test]
    fn status_transitions_set_and_clear_resolver_fields() {
        let p = pool();
        // resolved_by FKs users, so the acting admin must be a real account.
        let admin = crate::create_user(&p, "admin@test.dev", "admin", "h", &[]).unwrap().id;
        insert_report(&p, &new_report("r1", ReportSubjectKind::Movie, ReportCategory::Video), 1000)
            .unwrap();

        // Resolve: records actor + timestamp.
        assert!(set_report_status(&p, "r1", ReportStatus::Resolved, Some(&admin), 5000).unwrap());
        let conn = p.get().unwrap();
        let r = get_report(&conn, "r1").unwrap().unwrap();
        assert_eq!(r.status, ReportStatus::Resolved);
        assert_eq!(r.resolved_by.as_deref(), Some(admin.as_str()));
        assert_eq!(r.resolved_at, Some(5000));
        drop(conn);

        // Reopen: clears the resolver fields.
        assert!(set_report_status(&p, "r1", ReportStatus::Open, None, 6000).unwrap());
        let conn = p.get().unwrap();
        let r = get_report(&conn, "r1").unwrap().unwrap();
        assert_eq!(r.status, ReportStatus::Open);
        assert_eq!(r.resolved_by, None);
        assert_eq!(r.resolved_at, None);
        drop(conn);

        // Unknown id is a no-op.
        assert!(!set_report_status(&p, "ghost", ReportStatus::Dismissed, Some(&admin), 7000).unwrap());
    }

    #[test]
    fn list_scopes_to_one_reporter_and_delete_removes() {
        let p = pool();
        // A real user for the scoped reporter (reported_by FKs users).
        let alice = crate::create_user(&p, "a@test.dev", "alice", "h", &[]).unwrap().id;
        let mut ra = new_report("ra", ReportSubjectKind::Movie, ReportCategory::Other);
        ra.reported_by = Some(alice.clone());
        insert_report(&p, &ra, 1000).unwrap();
        insert_report(&p, &new_report("rb", ReportSubjectKind::Movie, ReportCategory::Other), 2000)
            .unwrap();

        let conn = p.get().unwrap();
        let mine = list_reports(&conn, Some(&alice)).unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, "ra");
        assert_eq!(mine[0].reported_by_name.as_deref(), Some("alice"));
        drop(conn);

        assert!(delete_report(&p, "ra").unwrap());
        assert!(!delete_report(&p, "ra").unwrap());
        let conn = p.get().unwrap();
        assert_eq!(list_reports(&conn, None).unwrap().len(), 1);
    }
}
