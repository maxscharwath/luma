//! Torznab indexer + download-client config persistence. Secrets (api_key /
//! password) never leave this layer as part of a view; the API maps rows to
//! `IndexerView` with only a has-secret flag.

use super::*;

/// A stored indexer row (full, including the secret; internal only).
#[derive(Debug, Clone)]
pub struct IndexerRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub categories: Vec<u32>,
    pub enabled: bool,
    pub priority: i32,
    /// `torznab` (external Jackett/Prowlarr) or `builtin` (native Cardigann).
    pub kind: String,
    /// The Cardigann definition id (file stem) for `builtin` rows.
    pub definition_id: Option<String>,
    /// JSON map of per-indexer settings (credentials + toggles) for `builtin`.
    pub settings: String,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

const INDEXER_COLS: &str = "id, name, url, api_key, categories, enabled, priority, \
    kind, definition_id, settings, last_ok_at, last_error, created_at";

fn row_to_indexer(r: &Row) -> rusqlite::Result<IndexerRow> {
    let cats: String = r.get(4)?;
    Ok(IndexerRow {
        id: r.get(0)?,
        name: r.get(1)?,
        url: r.get(2)?,
        api_key: r.get(3)?,
        categories: cats
            .split(',')
            .filter_map(|c| c.trim().parse().ok())
            .collect(),
        enabled: r.get::<_, i64>(5)? != 0,
        priority: r.get(6)?,
        kind: r.get(7)?,
        definition_id: r.get(8)?,
        settings: r.get(9)?,
        last_ok_at: r.get(10)?,
        last_error: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn list_indexers(conn: &Connection) -> rusqlite::Result<Vec<IndexerRow>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {INDEXER_COLS} FROM indexers ORDER BY created_at"))?;
    let rows = stmt.query_map([], row_to_indexer)?;
    rows.collect()
}

pub fn enabled_indexers(conn: &Connection) -> rusqlite::Result<Vec<IndexerRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {INDEXER_COLS} FROM indexers WHERE enabled = 1 ORDER BY priority DESC, created_at"
    ))?;
    let rows = stmt.query_map([], row_to_indexer)?;
    rows.collect()
}

pub fn get_indexer(conn: &Connection, id: &str) -> rusqlite::Result<Option<IndexerRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {INDEXER_COLS} FROM indexers WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_indexer)?;
    rows.next().transpose()
}

pub fn insert_indexer(pool: &Pool, row: &IndexerRow) -> Result<()> {
    let conn = pool.get()?;
    let cats = row.categories.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    conn.execute(
        "INSERT INTO indexers \
            (id, name, url, api_key, categories, enabled, priority, kind, definition_id, settings, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            row.id, row.name, row.url, row.api_key, cats, row.enabled as i64, row.priority,
            row.kind, row.definition_id, row.settings, row.created_at
        ],
    )?;
    Ok(())
}

/// Partial update; `api_key = None` keeps the stored secret.
#[allow(clippy::too_many_arguments)]
pub fn update_indexer(
    pool: &Pool,
    id: &str,
    name: Option<&str>,
    url: Option<&str>,
    api_key: Option<&str>,
    categories: Option<&[u32]>,
    enabled: Option<bool>,
    priority: Option<i32>,
    settings: Option<&str>,
) -> Result<bool> {
    let conn = pool.get()?;
    let cats = categories.map(|c| c.iter().map(u32::to_string).collect::<Vec<_>>().join(","));
    let n = conn.execute(
        "UPDATE indexers SET \
            name = COALESCE(?2, name), \
            url = COALESCE(?3, url), \
            api_key = COALESCE(?4, api_key), \
            categories = COALESCE(?5, categories), \
            enabled = COALESCE(?6, enabled), \
            priority = COALESCE(?7, priority), \
            settings = COALESCE(?8, settings) \
         WHERE id = ?1",
        params![id, name, url, api_key, cats, enabled.map(|e| e as i64), priority, settings],
    )?;
    Ok(n > 0)
}

pub fn delete_indexer(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    Ok(conn.execute("DELETE FROM indexers WHERE id = ?1", params![id])? > 0)
}

/// Record a test / search outcome on the row (drives the admin card's
/// last-test line).
pub fn note_indexer_result(pool: &Pool, id: &str, ok: bool, error: Option<&str>, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    if ok {
        conn.execute(
            "UPDATE indexers SET last_ok_at = ?2, last_error = NULL WHERE id = ?1",
            params![id, now_ms],
        )?;
    } else {
        conn.execute("UPDATE indexers SET last_error = ?2 WHERE id = ?1", params![id, error])?;
    }
    Ok(())
}

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
