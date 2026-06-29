//! Catalog queries that back the home-screen section generator: trending (recency
//! -weighted play counts), recently-added, the user's last play, batch hydration
//! by id, and the embedding-cache staleness stamp.

use super::*;

/// Top `n` item ids by recency-weighted play count over the last 30 days — a
/// half-life decay so a burst last week outranks a stale all-time favourite.
/// 604800 s = 1-week half-life; 2592000 s = 30-day window.
pub fn trending_ids(pool: &Pool, n: usize) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id, \
                SUM(1.0 / POW(2.0, (strftime('%s','now') - ended_at) / 604800.0)) AS score \
         FROM play_history \
         WHERE item_id IS NOT NULL AND ended_at > strftime('%s','now') - 2592000 \
         GROUP BY item_id ORDER BY score DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Most-recently-added movie ids (episodes excluded — rows are movie/show level).
pub fn recently_added_ids(pool: &Pool, n: usize) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id FROM items WHERE kind != 'episode' ORDER BY added_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The user's most recently finished item id (for "Because you watched …").
pub fn last_played(pool: &Pool, user_id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id FROM play_history \
         WHERE user_id = ?1 AND item_id IS NOT NULL \
         ORDER BY ended_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
    Ok(rows.next().transpose()?)
}

/// Hydrate item ids into full [`MediaItem`]s, preserving the given order and
/// silently dropping ids without a backing `items` row (e.g. show vectors).
pub fn items_by_ids(pool: &Pool, ids: &[&str]) -> Result<Vec<MediaItem>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let mut rows = stmt.query_map(params![id], row_to_item)?;
        if let Some(item) = rows.next() {
            let mut item = item?;
            attach_files(&conn, &mut item)?;
            out.push(item);
        }
    }
    Ok(out)
}

/// `MAX(updated_at)` over `item_vectors` — a cheap change-stamp the in-memory
/// [`crate::services::sections::VectorCache`] polls to know when to reload (it
/// changes on every re-embed, so it also catches a backend/dimension switch).
pub fn vectors_max_updated_at(pool: &Pool) -> Result<Option<String>> {
    let conn = pool.get()?;
    let stamp: Option<String> =
        conn.query_row("SELECT MAX(updated_at) FROM item_vectors", [], |r| r.get(0))?;
    Ok(stamp)
}
