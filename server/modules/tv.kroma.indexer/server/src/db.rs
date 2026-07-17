//! The Indexers module's own persistence: the `indexers` table (schema + typed
//! row + queries), relocated out of the core `kroma-db` crate so the module owns
//! its vertical end to end. [`MIGRATIONS`] is registered by the module's
//! `ServerModule::migrations` and applied at DB init, right after the core schema.
//!
//! Secrets (`api_key`, per-indexer passwords in `settings`) never leave this
//! layer as part of a view; the routes map rows to `IndexerView` with only a
//! has-secret flag.

use anyhow::Result;
use rusqlite::{params, Connection, Row};

use kroma_module_sdk::db::Pool;

/// The `indexers` table schema, applied after the core schema at DB init.
/// `IF NOT EXISTS` DDL only, so it runs harmlessly on every boot. Copied verbatim
/// out of the old core schema so existing databases keep working unchanged.
pub const MIGRATIONS: &str = "
    -- Torznab indexers (Jackett / Prowlarr endpoints). `categories` is a comma
    -- list; `priority` is a flat score tiebreak in the decision engine.
    -- `kind` is 'torznab' (external Jackett/Prowlarr endpoint; url+api_key) or
    -- 'builtin' (native Cardigann engine: `definition_id` names the definition,
    -- `settings` is a JSON map of the admin-entered per-indexer config incl.
    -- credentials, and `url` is the chosen base link).
    CREATE TABLE IF NOT EXISTS indexers (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        url           TEXT NOT NULL,
        api_key       TEXT NOT NULL DEFAULT '',
        categories    TEXT NOT NULL DEFAULT '2000,5000',
        enabled       INTEGER NOT NULL DEFAULT 1,
        priority      INTEGER NOT NULL DEFAULT 0,
        kind          TEXT NOT NULL DEFAULT 'torznab',
        definition_id TEXT,
        settings      TEXT NOT NULL DEFAULT '{}',
        last_ok_at    INTEGER,
        last_error    TEXT,
        created_at    INTEGER NOT NULL
    );
";

// The stored indexer row is the shared IndexerRow contract now, so the downloads
// queue view + acquisition name it without depending on this crate.
pub use kroma_module_sdk::ports::IndexerRow;

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
