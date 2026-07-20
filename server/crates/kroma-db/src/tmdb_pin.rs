//! Operator-chosen TMDB ids (`tmdb_pin`): the "this match is wrong, use *this*
//! title instead" override.
//!
//! Enrichment consults a pin before any title guess and fetches the id directly,
//! so a correction is authoritative: it survives re-scans, the nightly metadata
//! pass, and a metadata reset. Clearing the pin restores automatic matching.
//!
//! Distinct from the acquisition hint in [`super::downloads`], which is keyed by
//! the logical id an import is *about to* produce, i.e. before the subject exists.

use std::collections::HashMap;

use super::*;

/// Pin `tmdb_id` to one catalog subject (`kind` is `metadata_core::ITEM` or
/// `SHOW`). Upsert: re-pinning replaces the previous choice.
pub fn set(pool: &Pool, kind: &str, id: &str, tmdb_id: u64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO tmdb_pin (subject_kind,subject_id,tmdb_id,updated_at) VALUES (?1,?2,?3,?4) \
         ON CONFLICT(subject_kind,subject_id) DO UPDATE SET \
             tmdb_id=excluded.tmdb_id, updated_at=excluded.updated_at",
        params![kind, id, tmdb_id as i64, kroma_primitives::now_ms()],
    )?;
    Ok(())
}

/// Drop one subject's pin, restoring automatic matching.
pub fn clear(pool: &Pool, kind: &str, id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM tmdb_pin WHERE subject_kind=?1 AND subject_id=?2",
        params![kind, id],
    )?;
    Ok(())
}

/// One subject's pinned id, if an operator set one.
pub fn get(conn: &Connection, kind: &str, id: &str) -> Result<Option<u64>> {
    use rusqlite::OptionalExtension;
    let found = conn
        .query_row(
            "SELECT tmdb_id FROM tmdb_pin WHERE subject_kind=?1 AND subject_id=?2",
            params![kind, id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;
    Ok(found.map(|v| v as u64))
}

/// Every pin for one subject kind, keyed by subject id. Used by the metadata
/// stage's enumeration so a changed pin re-signs the element and re-queues it.
pub fn all_for_kind(pool: &Pool, kind: &str) -> Result<HashMap<String, u64>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT subject_id, tmdb_id FROM tmdb_pin WHERE subject_kind=?1")?;
    let rows = stmt.query_map(params![kind], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, tmdb) = row?;
        out.insert(id, tmdb);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_core::{ITEM, SHOW};
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kroma-tmdb-pin-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::schema::init(&dir.join("test.db")).unwrap()
    }

    #[test]
    fn set_then_get_round_trips() {
        let pool = pool();
        set(&pool, ITEM, "movie-1", 603).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(get(&conn, ITEM, "movie-1").unwrap(), Some(603));
    }

    #[test]
    fn a_pin_is_scoped_to_its_subject_kind() {
        let pool = pool();
        set(&pool, ITEM, "same-id", 1).unwrap();
        set(&pool, SHOW, "same-id", 2).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(get(&conn, ITEM, "same-id").unwrap(), Some(1));
        assert_eq!(get(&conn, SHOW, "same-id").unwrap(), Some(2));
    }

    #[test]
    fn re_pinning_replaces_the_previous_choice() {
        let pool = pool();
        set(&pool, ITEM, "movie-1", 603).unwrap();
        set(&pool, ITEM, "movie-1", 604).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(get(&conn, ITEM, "movie-1").unwrap(), Some(604));
    }

    #[test]
    fn clear_restores_automatic_matching() {
        let pool = pool();
        set(&pool, ITEM, "movie-1", 603).unwrap();
        clear(&pool, ITEM, "movie-1").unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(get(&conn, ITEM, "movie-1").unwrap(), None);
    }

    #[test]
    fn clearing_an_unpinned_subject_is_a_no_op() {
        let pool = pool();
        clear(&pool, ITEM, "never-pinned").unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(get(&conn, ITEM, "never-pinned").unwrap(), None);
    }

    #[test]
    fn all_for_kind_returns_only_that_kind() {
        let pool = pool();
        set(&pool, ITEM, "a", 1).unwrap();
        set(&pool, ITEM, "b", 2).unwrap();
        set(&pool, SHOW, "c", 3).unwrap();
        let items = all_for_kind(&pool, ITEM).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items.get("a"), Some(&1));
        assert_eq!(items.get("b"), Some(&2));
    }
}
