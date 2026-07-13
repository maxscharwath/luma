//! Ledger read / status queries: per-stage tallies, per-subject status rollups,
//! and the lean row-mappers that back the pipeline elements list. All read-only.

use std::collections::HashMap;

use crate::*;
use luma_domain::{PipelineTaskView, StageStat};

/// Per-stage status tally `(pending, running, done, failed, blocked)`.
pub fn counts(pool: &Pool, stage: &str) -> Result<(i64, i64, i64, i64, i64)> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM pipeline_tasks WHERE stage=?1 GROUP BY status")?;
    let rows = stmt.query_map(params![stage], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut c = [0i64; 5]; // pending, running, done, failed, blocked
    for row in rows {
        let (st, n) = row?;
        match st.as_str() {
            "pending" => c[0] = n,
            "running" => c[1] = n,
            "done" => c[2] = n,
            "failed" => c[3] = n,
            "blocked" => c[4] = n,
            _ => {}
        }
    }
    Ok((c[0], c[1], c[2], c[3], c[4]))
}

/// The `StageStat` for one stage (counts + identity), for the API + WS event.
pub fn stage_stat(pool: &Pool, stage: &str, key: &str, subject_kind: &str) -> Result<StageStat> {
    let (pending, running, done, failed, blocked) = counts(pool, stage)?;
    Ok(StageStat {
        stage: stage.to_string(),
        key: key.to_string(),
        subject_kind: subject_kind.to_string(),
        pending,
        running,
        done,
        failed,
        blocked,
    })
}

/// Every task of one stage as `subject_id -> (status, error)`. Bulk map for the
/// pipeline elements list (overlays the ledger's running/failed/pending states,
/// with the failure message, onto the cheap artifact signals).
pub fn stage_statuses(
    pool: &Pool,
    stage: &str,
) -> Result<HashMap<String, (String, Option<String>)>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT subject_id, status, error FROM pipeline_tasks WHERE stage=?1")?;
    let rows = stmt.query_map(params![stage], |r| {
        Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

/// Lean item row for the elements list: only the columns the view needs, with
/// poster/genre/has-metadata pulled out of the JSON via `json_extract` so we
/// never deserialize the full (heavy) TMDB metadata blob per item.
pub struct RawItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub duration_ms: Option<i64>,
    pub show_id: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub episode_title: Option<String>,
    pub has_meta: bool,
    pub poster: Option<String>,
    pub genre: Option<String>,
}

pub struct RawShow {
    pub id: String,
    pub title: String,
    pub year: Option<i64>,
    pub has_meta: bool,
    pub poster: Option<String>,
    pub genre: Option<String>,
}

/// All items, lean (no full-metadata parse). One query.
pub fn raw_items(pool: &Pool) -> Result<Vec<RawItem>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,kind,title,year,duration_ms,show_id,show_title,season,episode,episode_title,\
           (metadata IS NOT NULL), json_extract(metadata,'$.posterUrl'), json_extract(metadata,'$.genres[0]') \
         FROM items",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(RawItem {
            id: r.get(0)?,
            kind: r.get(1)?,
            title: r.get(2)?,
            year: r.get(3)?,
            duration_ms: r.get(4)?,
            show_id: r.get(5)?,
            show_title: r.get(6)?,
            season: r.get(7)?,
            episode: r.get(8)?,
            episode_title: r.get(9)?,
            has_meta: r.get::<_, i64>(10)? != 0,
            poster: r.get(11)?,
            genre: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// All shows, lean. One query.
pub fn raw_shows(pool: &Pool) -> Result<Vec<RawShow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,title,year,(metadata IS NOT NULL), \
           json_extract(metadata,'$.posterUrl'), json_extract(metadata,'$.genres[0]') FROM shows",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(RawShow {
            id: r.get(0)?,
            title: r.get(1)?,
            year: r.get(2)?,
            has_meta: r.get::<_, i64>(3)? != 0,
            poster: r.get(4)?,
            genre: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The ledger status of one task, or `None` if no task exists for it yet.
pub fn task_status(pool: &Pool, stage: &str, subject_id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT status FROM pipeline_tasks WHERE stage=?1 AND subject_id=?2")?;
    let mut rows = stmt.query_map(params![stage, subject_id], |r| r.get::<_, String>(0))?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// The "worst" ledger status across several subjects of one stage, by severity
/// (`failed` > `running` > `pending` > `done`), or `None` if none has a task.
/// Used to roll a show's per-episode/per-file tasks up to one treatment state.
/// ONE query over the whole subject set (was N+1: a connection+query per id),
/// backed by the `(stage, subject_id)` index.
pub fn worst_status(pool: &Pool, stage: &str, subject_ids: &[String]) -> Result<Option<String>> {
    if subject_ids.is_empty() {
        return Ok(None);
    }
    let rank = |s: &str| match s {
        "failed" => 4,
        "running" => 3,
        "pending" => 2,
        "done" => 1,
        _ => 0,
    };
    let conn = pool.get()?;
    let placeholders = vec!["?"; subject_ids.len()].join(",");
    let sql = format!(
        "SELECT subject_id, status FROM pipeline_tasks \
         WHERE stage=? AND subject_id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut binds: Vec<&str> = Vec::with_capacity(subject_ids.len() + 1);
    binds.push(stage);
    binds.extend(subject_ids.iter().map(String::as_str));
    let rows = stmt.query_map(rusqlite::params_from_iter(binds), |r| r.get::<_, String>(1))?;
    let mut worst: Option<String> = None;
    for st in rows {
        let st = st?;
        if worst.as_deref().is_none_or(|w| rank(&st) > rank(w)) {
            worst = Some(st);
        }
    }
    Ok(worst)
}

/// Titles for a set of item ids, one query (ids without a row are simply absent
/// from the map). Batch resolver for the failed-task drill-down.
pub fn item_titles(pool: &Pool, ids: &[String]) -> Result<HashMap<String, String>> {
    titles_in(pool, "items", ids)
}

/// Titles for a set of show ids, one query. Batch resolver for the failed-task
/// drill-down (metadata/embed subjects are item-kind but their id may be a show).
pub fn show_titles(pool: &Pool, ids: &[String]) -> Result<HashMap<String, String>> {
    titles_in(pool, "shows", ids)
}

/// `id -> title` for the given ids from `table` (a fixed `"items"`/`"shows"`,
/// never user input), in one `IN (...)` query. Empty ids yields an empty map.
fn titles_in(pool: &Pool, table: &str, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let conn = pool.get()?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT id, title FROM {table} WHERE id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

/// Failed tasks for a stage's drill-down (newest failure first). `title` is left
/// as the raw id here; the API layer resolves it against the catalog.
pub fn failed_tasks(pool: &Pool, stage: &str, limit: usize) -> Result<Vec<PipelineTaskView>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT subject_kind, subject_id, status, attempts, error, finished_at \
         FROM pipeline_tasks WHERE stage=?1 AND status='failed' \
         ORDER BY finished_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![stage, limit as i64], |r| {
        let subject_id: String = r.get(1)?;
        Ok(PipelineTaskView {
            stage: stage.to_string(),
            subject_kind: r.get(0)?,
            title: subject_id.clone(),
            subject_id,
            status: r.get(2)?,
            attempts: r.get(3)?,
            error: r.get(4)?,
            finished_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
