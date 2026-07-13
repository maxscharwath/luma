//! The Downloads module's own persistence: the `download_clients` + `downloads`
//! ledger tables (schema + typed rows + queries), relocated out of the core
//! `luma-db` crate so the module owns its vertical end to end. [`MIGRATIONS`] is
//! registered by the module's `ServerModule::migrations` and applied at DB init,
//! right after the core schema (so `downloads.request_id` can FK the core
//! `requests` table).
//!
//! This module doubles as a thin facade: it re-exports the core `luma-db` surface
//! (catalog, requests, settings, tmdb hints) and the `indexers` rows that moved to
//! the Indexers module crate, so a single `crate::db::...` path resolves every
//! query the module makes.

use anyhow::Result;
use rusqlite::{params, Connection, Row};

// The core persistence surface (catalog, requests, settings, acq tmdb hints, ...)
// stays in luma-db; re-exported so `crate::db::get_request` etc. keep resolving.
pub use luma_module_sdk::db::*;
// The `indexers` table + queries moved into the Indexers module crate; the
// acquisition + queue paths reach them through this same facade.
// The indexers table is owned by the indexer module; the queue view + acquisition
// reach it through luma_module_sdk::ports::IndexerDbPort, not a re-export here.

/// Schema for the download tables this module owns, applied after the core schema
/// at DB init. `IF NOT EXISTS` DDL only, so it runs harmlessly on every boot.
/// Copied verbatim out of the old core schema so existing databases keep working.
pub const MIGRATIONS: &str = "
    -- Download clients (torrent engines). The embedded rqbit engine is seeded
    -- as a row (id='embedded', kind='rqbit') at boot when compiled in, so
    -- dispatch and the admin UI treat every engine uniformly; url/username/
    -- password apply to the external kinds only.
    CREATE TABLE IF NOT EXISTS download_clients (
        id         TEXT PRIMARY KEY,
        kind       TEXT NOT NULL,
        name       TEXT NOT NULL,
        url        TEXT NOT NULL DEFAULT '',
        username   TEXT NOT NULL DEFAULT '',
        password   TEXT NOT NULL DEFAULT '',
        enabled    INTEGER NOT NULL DEFAULT 1,
        priority   INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    );

    -- One row per grab: a release sent to a download client. `client_id` has no
    -- FK so history survives a deleted client config. `score_breakdown` keeps
    -- the decision engine's explanation; `episodes` is the JSON list of episode
    -- numbers a season pack covers; `imported_paths` the library files written.
    CREATE TABLE IF NOT EXISTS downloads (
        id              TEXT PRIMARY KEY,
        client_id       TEXT NOT NULL,
        client_ref      TEXT NOT NULL,
        request_id      TEXT REFERENCES requests(id) ON DELETE SET NULL,
        kind            TEXT NOT NULL,
        tmdb_id         INTEGER NOT NULL,
        title           TEXT,
        year            INTEGER,
        season          INTEGER,
        episodes        TEXT,
        release_title   TEXT NOT NULL,
        indexer_id      TEXT,
        info_hash       TEXT,
        magnet_or_url   TEXT NOT NULL,
        size_bytes      INTEGER,
        score           INTEGER,
        score_breakdown TEXT,
        status          TEXT NOT NULL DEFAULT 'queued',
        progress        REAL NOT NULL DEFAULT 0,
        save_path       TEXT,
        imported_paths  TEXT,
        error           TEXT,
        grabbed_at      INTEGER NOT NULL,
        completed_at    INTEGER,
        imported_at     INTEGER,
        details_url     TEXT,
        only_files      TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status, grabbed_at DESC);
    CREATE INDEX IF NOT EXISTS idx_downloads_req    ON downloads(request_id);
";

// ----- download clients -----------------------------------------------------------

/// The seeded embedded-engine row id (created at boot when compiled in).
pub const EMBEDDED_CLIENT_ID: &str = "embedded";

/// A stored download-client row (full, including the secret; internal only).
#[derive(Debug, Clone)]
pub struct DownloadClientRow {
    pub id: String,
    /// `rqbit` | `transmission` | `qbittorrent`.
    pub kind: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: i64,
}

const CLIENT_COLS: &str = "id, kind, name, url, username, password, enabled, priority, created_at";

fn row_to_client(r: &Row) -> rusqlite::Result<DownloadClientRow> {
    Ok(DownloadClientRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        url: r.get(3)?,
        username: r.get(4)?,
        password: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        priority: r.get(7)?,
        created_at: r.get(8)?,
    })
}

pub fn list_download_clients(conn: &Connection) -> rusqlite::Result<Vec<DownloadClientRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CLIENT_COLS} FROM download_clients ORDER BY priority DESC, created_at"
    ))?;
    let rows = stmt.query_map([], row_to_client)?;
    rows.collect()
}

pub fn get_download_client(conn: &Connection, id: &str) -> rusqlite::Result<Option<DownloadClientRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {CLIENT_COLS} FROM download_clients WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_client)?;
    rows.next().transpose()
}

/// The engine a new grab goes to: first enabled by priority.
pub fn preferred_download_client(conn: &Connection) -> rusqlite::Result<Option<DownloadClientRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CLIENT_COLS} FROM download_clients WHERE enabled = 1 \
         ORDER BY priority DESC, created_at LIMIT 1"
    ))?;
    let mut rows = stmt.query_map([], row_to_client)?;
    rows.next().transpose()
}

pub fn insert_download_client(pool: &Pool, row: &DownloadClientRow) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR IGNORE INTO download_clients (id, kind, name, url, username, password, enabled, priority, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![row.id, row.kind, row.name, row.url, row.username, row.password, row.enabled as i64, row.priority, row.created_at],
    )?;
    Ok(())
}

/// Partial update; `password = None` keeps the stored secret.
#[allow(clippy::too_many_arguments)]
pub fn update_download_client(
    pool: &Pool,
    id: &str,
    name: Option<&str>,
    url: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    enabled: Option<bool>,
    priority: Option<i32>,
) -> Result<bool> {
    let conn = pool.get()?;
    let n = conn.execute(
        "UPDATE download_clients SET \
            name = COALESCE(?2, name), \
            url = COALESCE(?3, url), \
            username = COALESCE(?4, username), \
            password = COALESCE(?5, password), \
            enabled = COALESCE(?6, enabled), \
            priority = COALESCE(?7, priority) \
         WHERE id = ?1",
        params![id, name, url, username, password, enabled.map(|e| e as i64), priority],
    )?;
    Ok(n > 0)
}

pub fn delete_download_client(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    Ok(conn.execute("DELETE FROM download_clients WHERE id = ?1", params![id])? > 0)
}

// ----- downloads (grab ledger) ----------------------------------------------------

// DownloadRow moved to luma_module_sdk::ports; re-exported for this crate.
pub use luma_module_sdk::ports::DownloadRow;

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
