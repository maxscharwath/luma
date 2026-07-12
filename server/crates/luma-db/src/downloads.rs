//! Acquisition TMDB hint (`acq_tmdb`): pin a known TMDB id to the logical item
//! id an import will produce, so metadata enrichment adopts the real id instead
//! of re-guessing it from the filename.
//!
//! The `downloads` / `download_clients` ledger tables and their typed queries now
//! live in the dev.luma.torrents module crate (`luma_torrent::db`); only this
//! core hint table stays here, because `tmdb_hint` is read by the core enrichment
//! service (which depends on no module crate).

use super::*;

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
