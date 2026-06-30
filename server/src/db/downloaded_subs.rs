//! Subtitles fetched from an online provider and cached as WebVTT. One row per
//! downloaded track; merged into the item's subtitle list so they show in the
//! player next to embedded tracks.

use super::*;

/// A cached online subtitle. `path` is an absolute WebVTT file under the data dir.
#[derive(Debug, Clone)]
pub struct DownloadedSub {
    pub id: String,
    pub item_id: String,
    pub language: Option<String>,
    pub label: String,
    pub provider: String,
    pub path: String,
}

fn from_row(r: &Row) -> rusqlite::Result<DownloadedSub> {
    Ok(DownloadedSub {
        id: r.get(0)?,
        item_id: r.get(1)?,
        language: r.get(2)?,
        label: r.get(3)?,
        provider: r.get(4)?,
        path: r.get(5)?,
    })
}

const COLS: &str = "id, item_id, language, label, provider, path";

/// Every downloaded subtitle for an item, oldest first.
pub fn downloaded_subs_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<Vec<DownloadedSub>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {COLS} FROM downloaded_subtitles WHERE item_id = ?1 ORDER BY created_at"))?;
    let rows = stmt.query_map(params![item_id], |r| from_row(r))?;
    rows.collect()
}

/// One downloaded subtitle by id (for serving its WebVTT).
pub fn downloaded_sub(conn: &Connection, id: &str) -> rusqlite::Result<Option<DownloadedSub>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM downloaded_subtitles WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], |r| from_row(r))?;
    rows.next().transpose()
}

/// Insert (or replace) a downloaded subtitle record.
pub fn insert_downloaded_sub(pool: &Pool, sub: &DownloadedSub) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO downloaded_subtitles (id, item_id, language, label, provider, path, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
        params![sub.id, sub.item_id, sub.language, sub.label, sub.provider, sub.path],
    )?;
    Ok(())
}
