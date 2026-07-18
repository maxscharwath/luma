//! The Downloads module's own persistence: the `download_clients` + `downloads`
//! ledger tables (schema + typed rows + queries), relocated out of the core
//! `kroma-db` crate so the module owns its vertical end to end. [`MIGRATIONS`] is
//! registered by the module's `ServerModule::migrations` and applied at DB init,
//! right after the core schema (so `downloads.request_id` can FK the core
//! `requests` table).
//!
//! This module doubles as a thin facade: it re-exports the core `kroma-db` surface
//! (catalog, requests, settings, tmdb hints) and the `indexers` rows that moved to
//! the Indexers module crate, so a single `crate::db::...` path resolves every
//! query the module makes.

use anyhow::Result;
use rusqlite::{params, Connection, Row};

// The core persistence surface (catalog, requests, settings, acq tmdb hints, ...)
// stays in kroma-db; re-exported so `crate::db::get_request` etc. keep resolving.
pub use kroma_module_sdk::db::*;
// The `indexers` table + queries moved into the Indexers module crate; the
// acquisition + queue paths reach them through this same facade.
// The indexers table is owned by the indexer module; the queue view + acquisition
// reach it through kroma_module_sdk::ports::IndexerDbPort, not a re-export here.

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

// DownloadRow moved to kroma_module_sdk::ports; re-exported for this crate.
pub use kroma_module_sdk::ports::DownloadRow;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A fresh temp DB with the core schema (via `init`, so the `requests` table
    /// the downloads FK points at exists) plus this module's own tables applied.
    fn test_db() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("kroma-torrents-test-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = init(&path).unwrap();
        {
            let conn = pool.get().unwrap();
            apply_migrations(&conn, MIGRATIONS).unwrap();
        }
        pool
    }

    fn client(id: &str, priority: i32, enabled: bool, created_at: i64) -> DownloadClientRow {
        DownloadClientRow {
            id: id.into(),
            kind: "rqbit".into(),
            name: format!("Client {id}"),
            url: "http://host".into(),
            username: "user".into(),
            password: "secret".into(),
            enabled,
            priority,
            created_at,
        }
    }

    fn download(id: &str, status: &str, grabbed_at: i64) -> DownloadRow {
        DownloadRow {
            id: id.into(),
            client_id: "embedded".into(),
            client_ref: String::new(),
            request_id: None,
            kind: "movie".into(),
            tmdb_id: 42,
            title: Some("Dune".into()),
            year: Some(2021),
            season: None,
            episodes: None,
            release_title: format!("Rel.{id}.mkv"),
            indexer_id: None,
            info_hash: None,
            magnet_or_url: format!("magnet:?xt=urn:btih:{id}"),
            size_bytes: Some(1024),
            score: Some(5),
            score_breakdown: None,
            status: status.into(),
            progress: 0.0,
            save_path: None,
            imported_paths: None,
            error: None,
            grabbed_at,
            completed_at: None,
            imported_at: None,
            details_url: None,
            only_files: None,
        }
    }

    /// Seed a bare `requests` row so a download's `request_id` FK is satisfiable.
    fn seed_request(pool: &Pool, id: &str) {
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO requests (id,kind,tmdb_id,title,status,created_at,updated_at) \
                 VALUES (?1,'movie',1,'T','pending',0,0)",
                params![id],
            )
            .unwrap();
    }

    #[test]
    fn download_clients_crud_and_ordering() {
        let pool = test_db();
        {
            let conn = pool.get().unwrap();
            // Empty DB: nothing to list / find / prefer.
            assert!(list_download_clients(&conn).unwrap().is_empty());
            assert!(get_download_client(&conn, "c1").unwrap().is_none());
            assert!(preferred_download_client(&conn).unwrap().is_none());
        }

        insert_download_client(&pool, &client("c1", 10, true, 100)).unwrap();
        insert_download_client(&pool, &client("c2", 20, true, 200)).unwrap();
        insert_download_client(&pool, &client("c3", 20, false, 150)).unwrap();

        {
            let conn = pool.get().unwrap();
            // ORDER BY priority DESC, created_at ASC: c3 (20,150), c2 (20,200), c1 (10).
            let ids: Vec<String> =
                list_download_clients(&conn).unwrap().into_iter().map(|c| c.id).collect();
            assert_eq!(ids, vec!["c3".to_string(), "c2".to_string(), "c1".to_string()]);

            let c2 = get_download_client(&conn, "c2").unwrap().unwrap();
            assert_eq!(c2.name, "Client c2");
            assert_eq!(c2.password, "secret");
            assert!(get_download_client(&conn, "missing").unwrap().is_none());

            // Preferred = first ENABLED by priority (disabled c3 is skipped).
            assert_eq!(preferred_download_client(&conn).unwrap().unwrap().id, "c2");
        }

        // INSERT OR IGNORE: re-inserting an existing id keeps the original row.
        insert_download_client(&pool, &client("c1", 99, false, 999)).unwrap();
        {
            let conn = pool.get().unwrap();
            let c1 = get_download_client(&conn, "c1").unwrap().unwrap();
            assert_eq!(c1.priority, 10);
            assert_eq!(c1.name, "Client c1");
        }

        // Partial update: name/enabled/priority change; password None keeps the secret.
        assert!(update_download_client(
            &pool,
            "c1",
            Some("Renamed"),
            None,
            None,
            None,
            Some(false),
            Some(50),
        )
        .unwrap());
        {
            let conn = pool.get().unwrap();
            let c1 = get_download_client(&conn, "c1").unwrap().unwrap();
            assert_eq!(c1.name, "Renamed");
            assert!(!c1.enabled);
            assert_eq!(c1.priority, 50);
            assert_eq!(c1.password, "secret"); // unchanged
            assert_eq!(c1.url, "http://host"); // unchanged
        }
        // Password can be updated when Some is passed.
        assert!(update_download_client(&pool, "c1", None, None, None, Some("newpass"), None, None)
            .unwrap());
        {
            let conn = pool.get().unwrap();
            assert_eq!(get_download_client(&conn, "c1").unwrap().unwrap().password, "newpass");
        }
        // Updating an unknown id affects no rows.
        assert!(!update_download_client(&pool, "missing", Some("x"), None, None, None, None, None)
            .unwrap());

        assert!(delete_download_client(&pool, "c1").unwrap());
        assert!(!delete_download_client(&pool, "c1").unwrap()); // already gone
        {
            let conn = pool.get().unwrap();
            assert!(get_download_client(&conn, "c1").unwrap().is_none());
        }
    }

    #[test]
    fn downloads_insert_get_list_roundtrip() {
        let pool = test_db();

        let mut d1 = download("d1", "queued", 10);
        d1.episodes = Some(vec![1, 2, 3]);
        d1.only_files = Some(vec![0, 2]);
        d1.season = Some(2);
        d1.size_bytes = Some(2048);
        d1.tmdb_id = 99;
        insert_download(&pool, &d1).unwrap();
        insert_download(&pool, &download("d2", "downloading", 20)).unwrap();
        insert_download(&pool, &download("d3", "seeding", 30)).unwrap();

        let conn = pool.get().unwrap();

        // Field round-trip incl. JSON-encoded episodes / only_files.
        let got = get_download(&conn, "d1").unwrap().unwrap();
        assert_eq!(got.episodes, Some(vec![1, 2, 3]));
        assert_eq!(got.only_files, Some(vec![0, 2]));
        assert_eq!(got.season, Some(2));
        assert_eq!(got.size_bytes, Some(2048));
        assert_eq!(got.tmdb_id, 99);
        assert_eq!(got.status, "queued");
        assert_eq!(got.title.as_deref(), Some("Dune"));
        // Columns not written by insert default to NULL.
        assert!(got.imported_paths.is_none());
        assert!(got.completed_at.is_none());

        assert!(get_download(&conn, "missing").unwrap().is_none());

        // Newest-first (grabbed_at DESC), honouring the limit.
        let ids: Vec<String> =
            list_downloads(&conn, 2).unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["d3".to_string(), "d2".to_string()]);
        let ids: Vec<String> =
            list_downloads(&conn, 10).unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["d3".to_string(), "d2".to_string(), "d1".to_string()]);
    }

    #[test]
    fn active_completed_and_dedup_queries() {
        let pool = test_db();
        for (id, status, at) in [
            ("d_q", "queued", 10),
            ("d_d", "downloading", 20),
            ("d_s", "seeding", 30),
            ("d_p", "paused", 40),
            ("d_c", "completed", 50),
            ("d_f", "failed", 60),
            ("d_r", "removed", 70),
            ("d_i", "imported", 80),
        ] {
            insert_download(&pool, &download(id, status, at)).unwrap();
        }
        let conn = pool.get().unwrap();

        // Non-terminal set, ordered by grabbed_at ASC.
        let active: Vec<String> =
            active_downloads(&conn).unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(active, vec!["d_q".to_string(), "d_d".into(), "d_s".into(), "d_p".into()]);

        let completed: Vec<String> =
            completed_downloads(&conn).unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(completed, vec!["d_c".to_string()]);

        // by_url: a live download matches; a failed one and an unknown url do not.
        assert_eq!(
            active_download_by_url(&conn, "magnet:?xt=urn:btih:d_d").unwrap().map(|d| d.id),
            Some("d_d".to_string())
        );
        assert!(active_download_by_url(&conn, "magnet:?xt=urn:btih:d_f").unwrap().is_none());
        assert!(active_download_by_url(&conn, "magnet:?xt=urn:btih:none").unwrap().is_none());
    }

    #[test]
    fn other_active_download_with_ref_dedups_by_engine_ref() {
        let pool = test_db();
        let mut a = download("a", "downloading", 10);
        a.client_ref = "ref-a".into();
        let mut b = download("b", "downloading", 20);
        b.client_ref = "ref-a".into();
        let mut c = download("c", "failed", 30);
        c.client_ref = "ref-b".into();
        insert_download(&pool, &a).unwrap();
        insert_download(&pool, &b).unwrap();
        insert_download(&pool, &c).unwrap();

        let conn = pool.get().unwrap();
        // Another live row shares ref-a; the excluded id is itself.
        assert_eq!(
            other_active_download_with_ref(&conn, "a", "ref-a").unwrap().map(|d| d.id),
            Some("b".to_string())
        );
        // An empty ref never matches.
        assert!(other_active_download_with_ref(&conn, "a", "").unwrap().is_none());
        // A terminal (failed) row is not a live duplicate.
        assert!(other_active_download_with_ref(&conn, "x", "ref-b").unwrap().is_none());
    }

    #[test]
    fn download_lifecycle_mutations() {
        let pool = test_db();
        insert_download(&pool, &download("d1", "queued", 10)).unwrap();

        activate_download(&pool, "d1", "cref").unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "downloading");
            assert_eq!(d.client_ref, "cref");
        }

        update_download_progress(&pool, "d1", "downloading", 0.5, Some("/dl/path"), None).unwrap();
        // Later tick with save_path None must not wipe the known path (COALESCE).
        update_download_progress(&pool, "d1", "seeding", 0.9, None, Some("warn")).unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "seeding");
            assert!((d.progress - 0.9).abs() < 1e-9);
            assert_eq!(d.save_path.as_deref(), Some("/dl/path"));
            assert_eq!(d.error.as_deref(), Some("warn"));
        }

        mark_download_completed(&pool, "d1", 12_345).unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "completed");
            assert!((d.progress - 1.0).abs() < 1e-9);
            assert_eq!(d.completed_at, Some(12_345));
        }

        mark_download_imported(&pool, "d1", &["/lib/a.mkv".to_string()], 67_890).unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "imported");
            assert_eq!(d.imported_paths, Some(vec!["/lib/a.mkv".to_string()]));
            assert_eq!(d.imported_at, Some(67_890));
            assert!(d.error.is_none());
        }

        reset_download_for_retry(&pool, "d1").unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "queued");
            assert_eq!(d.client_ref, "");
            assert!(d.error.is_none());
            assert!((d.progress - 0.0).abs() < 1e-9);
            assert!(d.completed_at.is_none());
            assert!(d.imported_at.is_none());
            assert!(d.imported_paths.is_none());
        }

        // set_download_ref attaches an engine ref without touching status.
        set_download_ref(&pool, "d1", "r2").unwrap();
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.client_ref, "r2");
            assert_eq!(d.status, "queued");
        }

        // set_download_status: sets error, then a None error is a COALESCE keep.
        assert!(set_download_status(&pool, "d1", "paused", Some("pz")).unwrap());
        assert!(set_download_status(&pool, "d1", "downloading", None).unwrap());
        {
            let conn = pool.get().unwrap();
            let d = get_download(&conn, "d1").unwrap().unwrap();
            assert_eq!(d.status, "downloading");
            assert_eq!(d.error.as_deref(), Some("pz"));
        }
        assert!(!set_download_status(&pool, "missing", "x", None).unwrap());

        assert!(delete_download_row(&pool, "d1").unwrap());
        assert!(!delete_download_row(&pool, "d1").unwrap());
        {
            let conn = pool.get().unwrap();
            assert!(get_download(&conn, "d1").unwrap().is_none());
        }
    }

    #[test]
    fn requests_with_active_downloads_rollup() {
        let pool = test_db();
        seed_request(&pool, "req1");
        seed_request(&pool, "req2");

        // req1: one live + one completed (+ a failed row that must be ignored).
        let mut a = download("a", "downloading", 10);
        a.request_id = Some("req1".into());
        a.progress = 0.5;
        let mut b = download("b", "completed", 20);
        b.request_id = Some("req1".into());
        b.progress = 1.0;
        let mut c = download("c", "failed", 30);
        c.request_id = Some("req1".into());
        c.progress = 0.9;
        // req2: only a live row.
        let mut e = download("e", "downloading", 40);
        e.request_id = Some("req2".into());
        e.progress = 0.2;
        // Orphan (no request) never appears.
        let f = download("f", "downloading", 50);
        for d in [a, b, c, e, f] {
            insert_download(&pool, &d).unwrap();
        }

        let conn = pool.get().unwrap();
        let by_req: std::collections::HashMap<String, ActiveDownload> =
            requests_with_active_downloads(&conn)
                .unwrap()
                .into_iter()
                .map(|r| (r.request_id.clone(), r))
                .collect();
        assert_eq!(by_req.len(), 2);

        let r1 = &by_req["req1"];
        assert!(r1.importing); // MAX(status='completed') = 1
        // AVG over live+completed only (the failed row is excluded): (0.5 + 1.0)/2.
        assert!((r1.progress - 0.75).abs() < 1e-9);

        let r2 = &by_req["req2"];
        assert!(!r2.importing);
        assert!((r2.progress - 0.2).abs() < 1e-9);
    }
}
