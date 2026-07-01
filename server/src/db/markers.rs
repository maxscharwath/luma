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

/// Item ids that have at least one stored marker. Bulk signal for the pipeline
/// elements list.
pub fn item_ids_with_markers(pool: &Pool) -> Result<std::collections::HashSet<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT DISTINCT item_id FROM markers")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Whether an item has any stored marker (for the per-element treatments view;
/// note a legitimately intro-less episode has none, so combine with the ledger).
pub fn has_markers(pool: &Pool, item_id: &str) -> Result<bool> {
    let conn = pool.get()?;
    let n: i64 =
        conn.query_row("SELECT COUNT(*) FROM markers WHERE item_id=?1", params![item_id], |r| {
            r.get(0)
        })?;
    Ok(n > 0)
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

/// [`markers_for_item`] over many items in one query per id-chunk, keyed by
/// item id (ids absent from the result simply have no markers).
pub(crate) fn markers_for_items(
    conn: &Connection,
    item_ids: &[&str],
) -> rusqlite::Result<std::collections::HashMap<String, Vec<Marker>>> {
    let mut out: std::collections::HashMap<String, Vec<Marker>> = std::collections::HashMap::new();
    for chunk in item_ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT item_id, kind, start_ms, end_ms FROM markers \
             WHERE item_id IN ({ph}) ORDER BY start_ms",
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (item_id, kind, start, end) = row?;
            if let Some(kind) = kind_from_str(&kind) {
                out.entry(item_id).or_default().push(Marker {
                    kind,
                    start_ms: start.max(0) as u64,
                    end_ms: end.max(0) as u64,
                });
            }
        }
    }
    Ok(out)
}

/// Upsert one segment marker (`(item_id, kind)` is unique). `source` records
/// provenance (`chapters` | `fingerprint` | `manual`).
///
/// Writes respect a provenance precedence (`manual` > `fingerprint` > `chapters`):
/// a write only overwrites an existing marker when its source ranks at least as
/// high. This keeps a re-probe (cheap embedded `chapters`) from clobbering a more
/// accurate `fingerprint` marker, while fingerprint/manual still refresh freely.
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
           source = excluded.source, updated_at = excluded.updated_at \
         WHERE (CASE excluded.source WHEN 'manual' THEN 3 WHEN 'fingerprint' THEN 2 WHEN 'chapters' THEN 1 ELSE 0 END) \
            >= (CASE markers.source WHEN 'manual' THEN 3 WHEN 'fingerprint' THEN 2 WHEN 'chapters' THEN 1 ELSE 0 END)",
        params![item_id, kind_str(kind), start_ms as i64, end_ms as i64, source],
    )?;
    Ok(())
}
