//! Downloads (grab) ledger persistence: one row per release sent to a torrent
//! engine, from `queued` through `imported`. The live speed/ETA never touches
//! the DB (it rides the WS event bus); rows carry the durable facts.

use super::*;

/// A stored download row.
#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub id: String,
    pub client_id: String,
    /// The engine's identifier (info-hash hex).
    pub client_ref: String,
    pub request_id: Option<String>,
    /// `movie` | `episode` | `season`.
    pub kind: String,
    pub tmdb_id: u64,
    /// Display / import title (denormalized so a manual grab imports without a
    /// request). `None` = fall back to parsing the release title.
    pub title: Option<String>,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episodes: Option<Vec<u32>>,
    pub release_title: String,
    pub indexer_id: Option<String>,
    pub info_hash: Option<String>,
    pub magnet_or_url: String,
    pub size_bytes: Option<u64>,
    pub score: Option<i32>,
    pub score_breakdown: Option<String>,
    pub status: String,
    pub progress: f64,
    pub save_path: Option<String>,
    /// Library files written by the import (persisted for the record / future
    /// "reveal in library"; not surfaced in a view yet).
    #[allow(dead_code)]
    pub imported_paths: Option<Vec<String>>,
    pub error: Option<String>,
    pub grabbed_at: i64,
    pub completed_at: Option<i64>,
    pub imported_at: Option<i64>,
    /// The tracker's human-viewable torrent page (Sonarr/Radarr's info link).
    pub details_url: Option<String>,
    /// Selected torrent file indices for a partial grab (`None` = whole torrent).
    /// Persisted so the background add ([`crate::services::downloads`]) keeps the
    /// selection even though it runs after the request returned.
    pub only_files: Option<Vec<usize>>,
}

const DL_COLS: &str = "id, client_id, client_ref, request_id, kind, tmdb_id, title, year, \
    season, episodes, release_title, indexer_id, info_hash, magnet_or_url, size_bytes, score, \
    score_breakdown, status, progress, save_path, imported_paths, error, grabbed_at, \
    completed_at, imported_at, details_url, only_files";

fn row_to_download(r: &Row) -> rusqlite::Result<DownloadRow> {
    let episodes: Option<String> = r.get(9)?;
    let imported: Option<String> = r.get(20)?;
    Ok(DownloadRow {
        id: r.get(0)?,
        client_id: r.get(1)?,
        client_ref: r.get(2)?,
        request_id: r.get(3)?,
        kind: r.get(4)?,
        tmdb_id: r.get::<_, i64>(5)? as u64,
        title: r.get(6)?,
        year: r.get(7)?,
        season: r.get(8)?,
        episodes: episodes.and_then(|j| serde_json::from_str(&j).ok()),
        release_title: r.get(10)?,
        indexer_id: r.get(11)?,
        info_hash: r.get(12)?,
        magnet_or_url: r.get(13)?,
        size_bytes: r.get::<_, Option<i64>>(14)?.map(|v| v as u64),
        score: r.get(15)?,
        score_breakdown: r.get(16)?,
        status: r.get(17)?,
        progress: r.get(18)?,
        save_path: r.get(19)?,
        imported_paths: imported.and_then(|j| serde_json::from_str(&j).ok()),
        error: r.get(21)?,
        grabbed_at: r.get(22)?,
        completed_at: r.get(23)?,
        imported_at: r.get(24)?,
        details_url: r.get(25)?,
        only_files: r.get::<_, Option<String>>(26)?.and_then(|j| serde_json::from_str(&j).ok()),
    })
}

pub fn insert_download(pool: &Pool, d: &DownloadRow) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO downloads (id, client_id, client_ref, request_id, kind, tmdb_id, title, year, \
            season, episodes, release_title, indexer_id, info_hash, magnet_or_url, size_bytes, \
            score, score_breakdown, status, progress, save_path, grabbed_at, details_url, only_files) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        params![
            d.id,
            d.client_id,
            d.client_ref,
            d.request_id,
            d.kind,
            d.tmdb_id as i64,
            d.title,
            d.year,
            d.season,
            d.episodes.as_ref().map(|e| serde_json::to_string(e).unwrap_or_default()),
            d.release_title,
            d.indexer_id,
            d.info_hash,
            d.magnet_or_url,
            d.size_bytes.map(|v| v as i64),
            d.score,
            d.score_breakdown,
            d.status,
            d.progress,
            d.save_path,
            d.grabbed_at,
            d.details_url,
            d.only_files.as_ref().map(|f| serde_json::to_string(f).unwrap_or_default())
        ],
    )?;
    Ok(())
}

/// Every download newest-first (the admin queue shows queue + history in one).
pub fn list_downloads(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<DownloadRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DL_COLS} FROM downloads ORDER BY grabbed_at DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![limit as i64], row_to_download)?;
    rows.collect()
}

pub fn get_download(conn: &Connection, id: &str) -> rusqlite::Result<Option<DownloadRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {DL_COLS} FROM downloads WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_download)?;
    rows.next().transpose()
}

/// Rows the monitor polls: everything not terminal.
pub fn active_downloads(conn: &Connection) -> rusqlite::Result<Vec<DownloadRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DL_COLS} FROM downloads \
         WHERE status IN ('queued', 'downloading', 'seeding', 'paused') ORDER BY grabbed_at"
    ))?;
    let rows = stmt.query_map([], row_to_download)?;
    rows.collect()
}

/// An existing non-terminal download of the same torrent (same magnet/URL), so
/// a re-grab doesn't create a duplicate. `failed`/`removed` rows don't count -
/// those are retryable.
pub fn active_download_by_url(conn: &Connection, url: &str) -> rusqlite::Result<Option<DownloadRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DL_COLS} FROM downloads \
         WHERE magnet_or_url = ?1 AND status NOT IN ('failed', 'removed') LIMIT 1"
    ))?;
    let mut rows = stmt.query_map(params![url], row_to_download)?;
    rows.next().transpose()
}

/// Another non-terminal download already running this exact torrent (same
/// engine ref / info-hash) - catches the same content grabbed from a different
/// URL, which the URL check can't see. Excludes the row being activated.
pub fn other_active_download_with_ref(
    conn: &Connection,
    exclude_id: &str,
    client_ref: &str,
) -> rusqlite::Result<Option<DownloadRow>> {
    if client_ref.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {DL_COLS} FROM downloads \
         WHERE client_ref = ?1 AND id != ?2 AND status NOT IN ('failed', 'removed') LIMIT 1"
    ))?;
    let mut rows = stmt.query_map(params![client_ref, exclude_id], row_to_download)?;
    rows.next().transpose()
}

/// Completed rows awaiting import.
pub fn completed_downloads(conn: &Connection) -> rusqlite::Result<Vec<DownloadRow>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {DL_COLS} FROM downloads WHERE status = 'completed'"))?;
    let rows = stmt.query_map([], row_to_download)?;
    rows.collect()
}

/// One request's live acquisition phase, derived from its download rows.
pub struct ActiveDownload {
    pub request_id: String,
    /// A completed grab is being imported (vs still downloading).
    pub importing: bool,
    /// Mean progress (0..1) across this request's live download rows.
    pub progress: f64,
}

/// Requests with a live grab, for deriving the transient `downloading` /
/// `importing` status + progress in list views straight from the relationship
/// (no denormalized status to go stale when a torrent fails or is deleted).
pub fn requests_with_active_downloads(conn: &Connection) -> rusqlite::Result<Vec<ActiveDownload>> {
    let mut stmt = conn.prepare(
        "SELECT request_id, MAX(status = 'completed'), AVG(progress) FROM downloads \
         WHERE request_id IS NOT NULL AND status IN ('queued', 'downloading', 'seeding', 'completed', 'paused') \
         GROUP BY request_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ActiveDownload {
            request_id: r.get::<_, String>(0)?,
            importing: r.get::<_, i64>(1)? != 0,
            progress: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
        })
    })?;
    rows.collect()
}

/// Monitor tick write: progress + status (+ save_path once known).
pub fn update_download_progress(
    pool: &Pool,
    id: &str,
    status: &str,
    progress: f64,
    save_path: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE downloads SET status = ?2, progress = ?3, \
            save_path = COALESCE(?4, save_path), error = ?5 WHERE id = ?1",
        params![id, status, progress, save_path, error],
    )?;
    Ok(())
}

pub fn mark_download_completed(pool: &Pool, id: &str, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE downloads SET status = 'completed', progress = 1.0, completed_at = ?2 WHERE id = ?1",
        params![id, now_ms],
    )?;
    Ok(())
}

pub fn mark_download_imported(pool: &Pool, id: &str, paths: &[String], now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE downloads SET status = 'imported', imported_paths = ?2, imported_at = ?3, \
            error = NULL WHERE id = ?1",
        params![id, serde_json::to_string(paths).unwrap_or_default(), now_ms],
    )?;
    Ok(())
}

/// Pin a known TMDB id to the logical item id an import will produce, so
/// enrichment adopts it instead of guessing from the filename.
pub fn set_tmdb_hint(pool: &Pool, logical_id: &str, tmdb_id: u64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO acq_tmdb (logical_id, tmdb_id) VALUES (?1, ?2) \
         ON CONFLICT(logical_id) DO UPDATE SET tmdb_id = excluded.tmdb_id",
        params![logical_id, tmdb_id as i64],
    )?;
    Ok(())
}

/// The pinned TMDB id for a logical item id, if any.
pub fn tmdb_hint(conn: &Connection, logical_id: &str) -> rusqlite::Result<Option<u64>> {
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT tmdb_id FROM acq_tmdb WHERE logical_id = ?1", params![logical_id], |r| {
        r.get::<_, i64>(0).map(|v| v as u64)
    })
    .optional()
}

/// Reset a failed/removed row back to `queued` (clearing the engine ref, error
/// and progress) so a background re-add can attempt it again.
pub fn reset_download_for_retry(pool: &Pool, id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE downloads SET status = 'queued', client_ref = '', error = NULL, progress = 0, \
            completed_at = NULL, imported_at = NULL, imported_paths = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Attach the engine's torrent ref once a background add resolves, and move the
/// row from `queued` to `downloading`.
pub fn activate_download(pool: &Pool, id: &str, client_ref: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE downloads SET client_ref = ?2, status = 'downloading', error = NULL WHERE id = ?1",
        params![id, client_ref],
    )?;
    Ok(())
}

/// Attach the engine ref WITHOUT changing status (a torrent that was added but
/// the row is already `paused`).
pub fn set_download_ref(pool: &Pool, id: &str, client_ref: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("UPDATE downloads SET client_ref = ?2 WHERE id = ?1", params![id, client_ref])?;
    Ok(())
}

pub fn set_download_status(pool: &Pool, id: &str, status: &str, error: Option<&str>) -> Result<bool> {
    let conn = pool.get()?;
    let n = conn.execute(
        "UPDATE downloads SET status = ?2, error = COALESCE(?3, error) WHERE id = ?1",
        params![id, status, error],
    )?;
    Ok(n > 0)
}

pub fn delete_download_row(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    Ok(conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])? > 0)
}
