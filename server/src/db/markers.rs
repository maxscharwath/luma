//! Episode segment markers (intro / credits): the data behind the player's
//! "skip intro" button and the credits-triggered "next episode" card. One row per
//! `(item_id, kind)`; bounds in milliseconds. Rows are written by the probe pass
//! (embedded chapters) and the audio-fingerprint job.

use super::*;
use crate::model::{Marker, MarkerKind};

fn kind_str(kind: MarkerKind) -> &'static str {
    match kind {
        MarkerKind::Intro => "intro",
        MarkerKind::Credits => "credits",
    }
}

fn kind_from_str(s: &str) -> Option<MarkerKind> {
    match s {
        "intro" => Some(MarkerKind::Intro),
        "credits" => Some(MarkerKind::Credits),
        _ => None,
    }
}

/// All markers for an item, ordered by start (intro before credits). Unknown
/// kinds are skipped so a future kind can't break older clients.
pub fn markers_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<Vec<Marker>> {
    let mut stmt = conn
        .prepare("SELECT kind, start_ms, end_ms FROM markers WHERE item_id = ?1 ORDER BY start_ms")?;
    let rows = stmt.query_map(params![item_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (kind, start, end) = r?;
        if let Some(kind) = kind_from_str(&kind) {
            out.push(Marker { kind, start_ms: start.max(0) as u64, end_ms: end.max(0) as u64 });
        }
    }
    Ok(out)
}

/// Upsert one segment marker (`(item_id, kind)` is unique). `source` records
/// provenance (`chapters` | `fingerprint` | `manual`).
pub fn set_marker(
    pool: &Pool,
    item_id: &str,
    kind: MarkerKind,
    start_ms: u64,
    end_ms: u64,
    source: &str,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO markers (item_id, kind, start_ms, end_ms, source, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now')) \
         ON CONFLICT(item_id, kind) DO UPDATE SET \
           start_ms = excluded.start_ms, end_ms = excluded.end_ms, \
           source = excluded.source, updated_at = excluded.updated_at",
        params![item_id, kind_str(kind), start_ms as i64, end_ms as i64, source],
    )?;
    Ok(())
}
