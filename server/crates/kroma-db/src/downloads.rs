//! Acquisition TMDB hint (`acq_file_tmdb`): pin a known TMDB id to the exact
//! file path an import wrote, so metadata enrichment adopts the real id instead
//! of re-guessing it from the filename.
//!
//! Keyed by absolute path rather than a recomputed logical item id: the import
//! knows exactly where it placed the file, and the scanner records that same
//! path in `files.abs_path`, so enrichment resolves an item's id by joining
//! `files` to this table and the link can never orphan on a title-parse
//! difference (the failure mode that matched "Scary Movie" to "A Scary Movie").
//!
//! The `downloads` / `download_clients` ledger tables and their typed queries now
//! live in the tv.kroma.torrents module crate (`kroma_torrent::db`); only this
//! core hint table stays here, because the join is read by the core enrichment
//! service (which depends on no module crate).

use std::collections::HashMap;

use super::*;

/// Pin a known TMDB id to the absolute path an import wrote a file to, so
/// enrichment adopts it instead of guessing from the filename. `abs_path` must be
/// the same (canonicalized) path the scanner records in `files.abs_path`.
pub fn set_file_tmdb(pool: &Pool, abs_path: &str, tmdb_id: u64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO acq_file_tmdb (abs_path, tmdb_id) VALUES (?1, ?2) \
         ON CONFLICT(abs_path) DO UPDATE SET tmdb_id = excluded.tmdb_id",
        params![abs_path, tmdb_id as i64],
    )?;
    Ok(())
}

/// The acquisition-known TMDB id for one movie item, resolved through any of its
/// files' paths. `None` when no imported file of the item carries a hint.
pub fn acq_tmdb_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<Option<u64>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT act.tmdb_id FROM acq_file_tmdb act \
         JOIN files f ON f.abs_path = act.abs_path \
         WHERE f.item_id = ?1 LIMIT 1",
        params![item_id],
        |r| r.get::<_, i64>(0).map(|v| v as u64),
    )
    .optional()
}

/// The acquisition-known TMDB id for one show, resolved through any of its
/// episodes' files (an episode download carries the show's TMDB id).
pub fn acq_tmdb_for_show(conn: &Connection, show_id: &str) -> rusqlite::Result<Option<u64>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT act.tmdb_id FROM acq_file_tmdb act \
         JOIN files f ON f.abs_path = act.abs_path \
         JOIN items i ON i.id = f.item_id \
         WHERE i.show_id = ?1 LIMIT 1",
        params![show_id],
        |r| r.get::<_, i64>(0).map(|v| v as u64),
    )
    .optional()
}

/// Every movie item that has an acquisition TMDB hint, keyed by item id. Loaded
/// once per enrichment run instead of a per-item query (the table is empty on a
/// library that never used the built-in acquisition).
pub fn acq_tmdb_by_item(pool: &Pool) -> Result<HashMap<String, u64>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT f.item_id, act.tmdb_id FROM acq_file_tmdb act \
         JOIN files f ON f.abs_path = act.abs_path",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
    })?;
    Ok(rows.flatten().collect())
}

/// Every show that has an acquisition TMDB hint on one of its episode files,
/// keyed by show id (an episode file's download carries the show's TMDB id).
pub fn acq_tmdb_by_show(pool: &Pool) -> Result<HashMap<String, u64>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT i.show_id, act.tmdb_id FROM acq_file_tmdb act \
         JOIN files f ON f.abs_path = act.abs_path \
         JOIN items i ON i.id = f.item_id \
         WHERE i.show_id IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
    })?;
    Ok(rows.flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-dl-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::init(&path).unwrap()
    }

    /// Insert a movie item with one file at `abs_path`, so the hint join has
    /// something to resolve against. `foreign_keys` is ON, so the parent library
    /// must exist first.
    fn seed_movie(pool: &Pool, item_id: &str, abs_path: &str) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO libraries (id, name, kind, path, added_at) \
             VALUES ('lib', 'Lib', 'movies', '/lib', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (id, title, kind, container, library, added_at) \
             VALUES (?1, 'X', 'movie', 'mkv', 'lib', '2026-01-01')",
            params![item_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (id, item_id, abs_path, container) \
             VALUES (?1, ?2, ?3, 'mkv')",
            params![format!("f-{item_id}"), item_id, abs_path],
        )
        .unwrap();
    }

    #[test]
    fn set_and_resolve_item_hint_by_path() {
        let p = pool();
        let path = "/lib/Scary Movie (2026)/Scary Movie (2026).mkv";
        seed_movie(&p, "item-1", path);
        {
            let conn = p.get().unwrap();
            assert!(acq_tmdb_for_item(&conn, "item-1").unwrap().is_none());
        }
        set_file_tmdb(&p, path, 1273221).unwrap();
        {
            let conn = p.get().unwrap();
            assert_eq!(acq_tmdb_for_item(&conn, "item-1").unwrap(), Some(1273221));
        }
        // Upsert replaces the pinned id in place.
        set_file_tmdb(&p, path, 999).unwrap();
        let conn = p.get().unwrap();
        assert_eq!(acq_tmdb_for_item(&conn, "item-1").unwrap(), Some(999));
        assert!(acq_tmdb_for_item(&conn, "unknown-item").unwrap().is_none());
    }

    #[test]
    fn preload_maps_items_by_id() {
        let p = pool();
        seed_movie(&p, "item-a", "/lib/A/A.mkv");
        seed_movie(&p, "item-b", "/lib/B/B.mkv");
        set_file_tmdb(&p, "/lib/A/A.mkv", 111).unwrap();
        // item-b has no hint -> absent from the map.
        let map = acq_tmdb_by_item(&p).unwrap();
        assert_eq!(map.get("item-a"), Some(&111));
        assert_eq!(map.get("item-b"), None);
    }

    #[test]
    fn a_hint_for_a_path_no_file_scanned_yet_resolves_nothing() {
        // The import may write the hint before the scan has indexed the file.
        // Until a `files` row exists at that path, the join yields nothing (and
        // must not error): the next scan lands the file and the id is adopted.
        let p = pool();
        set_file_tmdb(&p, "/lib/Pending/Pending.mkv", 42).unwrap();
        assert!(acq_tmdb_by_item(&p).unwrap().is_empty());
    }
}
