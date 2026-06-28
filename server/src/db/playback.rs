//! Playback progress: per-user saved positions and the "continue watching" join.

use super::*;

use crate::model::{ContinueItem, ProgressEntry};

/// Upsert one item's playback position for a user.
pub fn upsert_progress(
    pool: &Pool,
    user_id: &str,
    item_id: &str,
    position_ms: i64,
    duration_ms: Option<i64>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO progress (user_id,item_id,position_ms,duration_ms,updated_at) \
         VALUES (?1,?2,?3,?4,?5) \
         ON CONFLICT(user_id,item_id) DO UPDATE SET \
            position_ms=excluded.position_ms, duration_ms=excluded.duration_ms, \
            updated_at=excluded.updated_at",
        params![user_id, item_id, position_ms, duration_ms, now_or_blank()],
    )?;
    Ok(())
}

/// One item's saved progress for a user, if any.
pub fn get_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<Option<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 AND item_id = ?2",
    )?;
    let mut rows = stmt.query_map(params![user_id, item_id], row_to_progress)?;
    match rows.next() {
        Some(p) => Ok(Some(p?)),
        None => Ok(None),
    }
}

/// Every saved progress row for a user (newest first).
pub fn list_progress(pool: &Pool, user_id: &str) -> Result<Vec<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map(params![user_id], row_to_progress)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Remove a saved position (e.g. finished, or "remove from Continue Watching").
pub fn delete_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM progress WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// "Continue watching": resumable items (started, not yet ~finished), newest
/// first, each carried as a full [`MediaItem`] so clients render normal cards.
pub fn continue_watching(pool: &Pool, user_id: &str) -> Result<Vec<ContinueItem>> {
    let conn = pool.get()?;
    // 1) The resumable item ids + their progress. The JOIN drops any orphan
    //    progress row whose item no longer exists.
    let mut stmt = conn.prepare(
        "SELECT p.item_id,p.position_ms,p.duration_ms,p.updated_at \
         FROM progress p JOIN items i ON i.id = p.item_id \
         WHERE p.user_id = ?1 AND p.position_ms > 15000 \
           AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
         ORDER BY p.updated_at DESC LIMIT 30",
    )?;
    let rows: Vec<(String, i64, Option<i64>, String)> = stmt
        .query_map(params![user_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // 2) Hydrate each into a full item (with files) on the same connection.
    let mut item_stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"))?;
    let mut out = Vec::with_capacity(rows.len());
    for (item_id, position_ms, duration_ms, updated_at) in rows {
        let mut it = item_stmt.query_map(params![item_id], row_to_item)?;
        if let Some(item) = it.next() {
            let mut item = item?;
            attach_files(&conn, &mut item)?;
            out.push(ContinueItem { item, position_ms, duration_ms, updated_at });
        }
    }
    Ok(out)
}

/// Map a row of `item_id,position_ms,duration_ms,updated_at` to a [`ProgressEntry`].
fn row_to_progress(r: &Row) -> rusqlite::Result<ProgressEntry> {
    Ok(ProgressEntry {
        item_id: r.get(0)?,
        position_ms: r.get(1)?,
        duration_ms: r.get(2)?,
        updated_at: r.get(3)?,
    })
}
