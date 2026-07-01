//! Catalog queries that back the home-screen section generator: trending (recency
//! -weighted play counts), recently-added, the user's last play, batch hydration
//! by id, and the embedding-cache staleness stamp.

use super::*;

/// Top `n` item ids by recency-weighted play count over the last 30 days a
/// half-life decay so a burst last week outranks a stale all-time favourite.
/// 604800 s = 1-week half-life; 2592000 s = 30-day window.
pub fn trending_ids(pool: &Pool, n: usize) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id, \
                SUM(1.0 / POW(2.0, (strftime('%s','now') - ended_at) / 604800.0)) AS score \
         FROM play_history \
         WHERE item_id IS NOT NULL AND ended_at > strftime('%s','now') - 2592000 \
         GROUP BY item_id ORDER BY score DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Most-recently-added movie ids (episodes excluded rows are movie/show level).
pub fn recently_added_ids(pool: &Pool, n: usize) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id FROM items WHERE kind != 'episode' ORDER BY added_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The user's most recently finished item id (for "Because you watched …").
pub fn last_played(pool: &Pool, user_id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id FROM play_history \
         WHERE user_id = ?1 AND item_id IS NOT NULL \
         ORDER BY ended_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
    Ok(rows.next().transpose()?)
}

/// Of `candidates`, the subset sharing ≥1 genre with `seed` a coherence guard
/// for the single-seed "Because you watched" row. The lexical embedder is weakly
/// discriminative item↔item (the whole catalog clusters in a narrow cosine band,
/// so a Van Gogh drama's nearest neighbour can be a horror film); requiring a
/// shared genre keeps the row honest. `None` when `seed` has no genres nothing
/// to guard on, so the caller keeps the unfiltered list.
pub fn genre_coherent_ids(pool: &Pool, seed: &str, candidates: &[String]) -> Result<Option<std::collections::HashSet<String>>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let conn = pool.get()?;
    // Seed + candidates can both be movies *or* shows (recommendation rows mix
    // them), so look in both tables querying `items` alone would silently drop
    // every show id from the keep-set and defeat the movie/show mixing.
    let mut gstmt = conn.prepare(
        "SELECT g.value FROM items i, json_each(i.metadata,'$.genres') g WHERE i.id = ?1 \
         UNION SELECT g.value FROM shows s, json_each(s.metadata,'$.genres') g WHERE s.id = ?1",
    )?;
    let seed_genres: Vec<String> =
        gstmt.query_map(params![seed], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if seed_genres.is_empty() {
        return Ok(None);
    }
    let cand_ph = vec!["?"; candidates.len()].join(",");
    let genre_ph = vec!["?"; seed_genres.len()].join(",");
    let sql = format!(
        "SELECT DISTINCT i.id FROM items i, json_each(i.metadata,'$.genres') g \
         WHERE i.id IN ({cand_ph}) AND g.value IN ({genre_ph}) \
         UNION SELECT DISTINCT s.id FROM shows s, json_each(s.metadata,'$.genres') g \
         WHERE s.id IN ({cand_ph}) AND g.value IN ({genre_ph})"
    );
    let mut stmt = conn.prepare(&sql)?;
    // Placeholders appear twice (items arm + shows arm), so bind the args twice.
    let args = candidates
        .iter()
        .chain(seed_genres.iter())
        .chain(candidates.iter())
        .chain(seed_genres.iter());
    let kept = stmt
        .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
    Ok(Some(kept))
}

/// Drop the genre-incoherent neighbours from a ranked `(id, score)` list (order
/// preserved) via [`genre_coherent_ids`]. A no-op when the seed has no genres or
/// the query errors used by the "Because you watched" home row and the
/// detail-page "More like this" rail.
pub fn genre_guard(pool: &Pool, seed: &str, ranked: Vec<(String, f32)>) -> Vec<(String, f32)> {
    let ids: Vec<String> = ranked.iter().map(|(id, _)| id.clone()).collect();
    match genre_coherent_ids(pool, seed, &ids) {
        Ok(Some(keep)) => ranked.into_iter().filter(|(id, _)| keep.contains(id)).collect(),
        _ => ranked,
    }
}

/// Hydrate item ids into full [`MediaItem`]s, preserving the given order and
/// silently dropping ids without a backing `items` row (e.g. show vectors).
pub fn items_by_ids(pool: &Pool, ids: &[&str]) -> Result<Vec<MediaItem>> {
    let conn = pool.get()?;
    Ok(items_by_ids_ordered(&conn, ids)?)
}

/// Hydrate ranked ids into [`SectionItem`]s, preserving order: each id resolves
/// to a movie (an `items` row) or a show (a `shows` row); unknown ids drop. This
/// is what lets recommendation rows mix films and séries both are embedded and
/// ranked, but a show id has no `items` row, so [`items_by_ids`] alone drops it.
pub fn entities_by_ids(pool: &Pool, ids: &[&str]) -> Result<Vec<crate::model::SectionItem>> {
    use crate::model::{SectionItem, Show};
    use std::collections::HashMap;

    let mut item_map: HashMap<String, MediaItem> =
        items_by_ids(pool, ids)?.into_iter().map(|i| (i.id.clone(), i)).collect();
    let owned: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    let mut show_map: HashMap<String, Show> =
        get_shows_by_ids(pool, &owned)?.into_iter().map(|s| (s.id.clone(), s)).collect();

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(item) = item_map.remove(*id) {
            out.push(SectionItem::Movie { item });
        } else if let Some(show) = show_map.remove(*id) {
            out.push(SectionItem::Show { show });
        }
    }
    Ok(out)
}

/// `MAX(updated_at)` over `item_vectors` a cheap change-stamp the in-memory
/// [`crate::services::sections::VectorCache`] polls to know when to reload (it
/// changes on every re-embed, so it also catches a backend/dimension switch).
pub fn vectors_max_updated_at(pool: &Pool) -> Result<Option<String>> {
    let conn = pool.get()?;
    let stamp: Option<String> =
        conn.query_row("SELECT MAX(updated_at) FROM item_vectors", [], |r| r.get(0))?;
    Ok(stamp)
}
