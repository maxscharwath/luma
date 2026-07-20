//! Catalog queries that back the home-screen section generator: trending (recency
//! -weighted play counts), recently-added, the user's last play, batch hydration
//! by id, and the embedding-cache staleness stamp.

use super::*;

/// Top `n` *entity* ids by recency-weighted play count over the last 30 days a
/// half-life decay so a burst last week outranks a stale all-time favourite.
/// 604800 s = 1-week half-life; 2592000 s = 30-day window.
///
/// An episode play folds into its parent show (`COALESCE(show_id, item_id)`):
/// the home row hydrates these ids through [`entities_by_ids`], which only knows
/// how to render movies and shows an episode id would hydrate as a
/// `SectionItem::Movie` with no poster art (episodes carry none, only the show
/// does) and route to the wrong page. Folding also aggregates every episode of a
/// binged show into one trending entry instead of a row of near-duplicate cards.
///
/// The decay is `1 / 2^weeks` computed with a left shift on *whole* weeks
/// (integer division), not `POW()`: the bundled SQLite is compiled without
/// `SQLITE_ENABLE_MATH_FUNCTIONS`, so `POW` does not exist there and the query
/// used to fail on every call in production. Whole-week steps make the decay a
/// staircase rather than a smooth curve, which the ranking does not care about
/// (the window only spans 5 steps). The shift is clamped to 0..=62: a clock
/// jump could otherwise make it negative (SQLite would shift the other way and
/// divide by zero) or overflow a 64-bit shift.
pub fn trending_ids(pool: &Pool, n: usize) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(i.show_id, ph.item_id) AS ent_id, \
                SUM(1.0 / (1 << MIN(MAX((strftime('%s','now') - ph.ended_at) / 604800, 0), 62))) AS score \
         FROM play_history ph \
         LEFT JOIN items i ON i.id = ph.item_id \
         WHERE ph.item_id IS NOT NULL AND ph.ended_at > strftime('%s','now') - 2592000 \
         GROUP BY ent_id ORDER BY score DESC LIMIT ?1",
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

/// The distinct parent-show ids behind `ids` (only episode rows have one), in
/// [`IN_CHUNK`]-sized batches. A lean projection on purpose: callers that just
/// need "which shows does this history touch?" would otherwise hydrate every id
/// through [`items_by_ids`], paying for the metadata blob plus a files/markers
/// batch to read a single column. `DISTINCT` is per chunk, so a caller folding
/// into a set still gets the dedup it expects.
pub fn show_ids_for(pool: &Pool, ids: &[&str]) -> Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = pool.get()?;
    let mut out = Vec::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT DISTINCT show_id FROM items WHERE show_id IS NOT NULL AND id IN ({ph})"
        ))?;
        let rows =
            stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| r.get::<_, String>(0))?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Hydrate ranked ids into [`SectionItem`]s, preserving order: each id resolves
/// to a movie (an `items` row) or a show (a `shows` row); unknown ids drop. This
/// is what lets recommendation rows mix films and séries both are embedded and
/// ranked, but a show id has no `items` row, so [`items_by_ids`] alone drops it.
pub fn entities_by_ids(pool: &Pool, ids: &[&str]) -> Result<Vec<kroma_domain::SectionItem>> {
    use kroma_domain::{SectionItem, Show};
    use std::collections::HashMap;

    let mut item_map: HashMap<String, MediaItem> =
        items_by_ids(pool, ids)?.into_iter().map(|i| (i.id.clone(), i)).collect();
    let owned: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    let mut show_map: HashMap<String, Show> =
        get_shows_by_ids(pool, &owned)?.into_iter().map(|s| (s.id.clone(), s)).collect();

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(item) = item_map.remove(*id) {
            out.push(SectionItem::Movie { item: Box::new(item) });
        } else if let Some(show) = show_map.remove(*id) {
            out.push(SectionItem::Show { show: Box::new(show) });
        }
    }
    Ok(out)
}

/// `MAX(updated_at)` over `item_vectors` a cheap change-stamp the in-memory
/// `crate::services::sections::VectorCache` polls to know when to reload (it
/// changes on every re-embed, so it also catches a backend/dimension switch).
pub fn vectors_max_updated_at(pool: &Pool) -> Result<Option<String>> {
    let conn = pool.get()?;
    let stamp: Option<String> =
        conn.query_row("SELECT MAX(updated_at) FROM item_vectors", [], |r| r.get(0))?;
    Ok(stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kroma_domain::SectionItem;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn seeded() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-home-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = crate::init(&path).unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO libraries (id,name,kind,path,added_at) VALUES ('lib','L','movies','/x','t')", []).unwrap();
        let movie = |id: &str, added: &str, genres: &str| {
            conn.execute(
                "INSERT INTO items (id,kind,title,container,library,added_at,metadata) \
                 VALUES (?1,'movie','T','mkv','lib',?2,?3)",
                params![id, added, format!("{{\"tmdbId\":1,\"tmdbUrl\":\"x\",\"genres\":[{genres}]}}")],
            )
            .unwrap();
        };
        movie("seed", "2019", "\"Horror\"");
        movie("c1", "2020", "\"Horror\",\"Thriller\"");
        movie("c2", "2021", "\"Comedy\"");
        // A movie with no metadata (no genres to guard on).
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,added_at) VALUES ('nogen','movie','N','mkv','lib','2022')",
            [],
        )
        .unwrap();
        // An episode (excluded from recently-added).
        conn.execute(
            "INSERT INTO shows (id,library,title,added_at,metadata) VALUES ('sh1','lib','Show','t','{\"tmdbId\":9,\"tmdbUrl\":\"x\",\"genres\":[\"Horror\"]}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
             VALUES ('e1','episode','Ep','mkv','lib','sh1',1,1,'2099')",
            [],
        )
        .unwrap();
        drop(conn);
        pool
    }

    #[test]
    fn recently_added_last_played_and_trending() {
        let p = seeded();
        {
            let conn = p.get().unwrap();
            // Two recent plays of 'c1', one of 'c2' (item_id has no FK here).
            for (id, item) in [("p1", "c1"), ("p2", "c1"), ("p3", "c2")] {
                conn.execute(
                    "INSERT INTO play_history (id,user_id,item_id,kind,title,started_at,ended_at) \
                     VALUES (?1,'u1',?2,'movie','T',0,strftime('%s','now'))",
                    params![id, item],
                )
                .unwrap();
            }
        }
        // recently-added excludes episodes; newest added_at first.
        let recent = recently_added_ids(&p, 10).unwrap();
        assert!(!recent.contains(&"e1".to_string())); // episodes excluded
        // Newest added_at first: nogen (2022) before c2 (2021) before c1 (2020).
        let idx = |id: &str| recent.iter().position(|x| x == id).unwrap();
        assert!(idx("nogen") < idx("c2") && idx("c2") < idx("c1"));

        // (trending_ids has its own ranking test below.)

        // last_played = most recent by ended_at for the user.
        assert!(last_played(&p, "u1").unwrap().is_some());
        assert!(last_played(&p, "nobody").unwrap().is_none());
    }

    #[test]
    fn trending_ids_ranks_by_recency_weighted_plays() {
        let p = seeded();
        let day = 86_400i64;
        {
            let conn = p.get().unwrap();
            let play = |item: &str, ago: i64| {
                let id = format!("ph-{item}-{ago}-{}", SEQ.fetch_add(1, Ordering::Relaxed));
                conn.execute(
                    "INSERT INTO play_history (id,user_id,item_id,kind,title,started_at,ended_at) \
                     VALUES (?1,'u1',?2,'movie','T',0,strftime('%s','now') - ?3)",
                    params![id, item, ago],
                )
                .unwrap();
            };
            // 3 plays yesterday: full weight (week 0) -> 3.0.
            for _ in 0..3 {
                play("c1", day);
            }
            // 3 plays 25 days ago: week 3 -> 3 * 0.125 = 0.375.
            for _ in 0..3 {
                play("c2", 25 * day);
            }
            // 5 plays 21 days ago: week 3 -> 5 * 0.125 = 0.625. More plays than c1,
            // but staler, so the decay must still put it behind.
            for _ in 0..5 {
                play("nogen", 21 * day);
            }
            // Played 10x two months ago: outside the 30-day window entirely.
            for _ in 0..10 {
                play("seed", 60 * day);
            }
        }

        let top = trending_ids(&p, 10).unwrap();
        // Recency beats raw count, and among equal counts the fresher wins.
        assert_eq!(top, vec!["c1".to_string(), "nogen".to_string(), "c2".to_string()]);
        // The two-month-old binge is out of the window, so it never shows.
        assert!(!top.contains(&"seed".to_string()));
        // The limit is honoured.
        assert_eq!(trending_ids(&p, 2).unwrap(), vec!["c1".to_string(), "nogen".to_string()]);
    }

    #[test]
    fn trending_folds_episodes_into_their_show() {
        let p = seeded();
        {
            let conn = p.get().unwrap();
            // A single fresh episode play. 'e1' belongs to show 'sh1'.
            conn.execute(
                "INSERT INTO play_history (id,user_id,item_id,kind,title,started_at,ended_at) \
                 VALUES ('phe','u1','e1','episode','Ep',0,strftime('%s','now'))",
                [],
            )
            .unwrap();
        }
        let top = trending_ids(&p, 10).unwrap();
        // The episode surfaces as its parent show, never as the raw episode id
        // (episodes have no poster art and route to the wrong page).
        assert!(top.contains(&"sh1".to_string()));
        assert!(!top.contains(&"e1".to_string()));
    }

    #[test]
    fn genre_coherence_and_guard() {
        let p = seeded();
        let cands = vec!["c1".to_string(), "c2".to_string()];
        // Only c1 shares Horror with the seed.
        let keep = genre_coherent_ids(&p, "seed", &cands).unwrap().unwrap();
        assert!(keep.contains("c1") && !keep.contains("c2"));

        // Empty candidate list, and a seed without genres, both yield None (no guard).
        assert!(genre_coherent_ids(&p, "seed", &[]).unwrap().is_none());
        assert!(genre_coherent_ids(&p, "nogen", &cands).unwrap().is_none());

        // genre_guard drops the incoherent neighbour, preserving order.
        let ranked = vec![("c1".to_string(), 0.9f32), ("c2".to_string(), 0.5f32)];
        assert_eq!(genre_guard(&p, "seed", ranked), vec![("c1".to_string(), 0.9f32)]);
        // A genreless seed is a no-op (keeps everything).
        let ranked = vec![("c1".to_string(), 0.9f32), ("c2".to_string(), 0.5f32)];
        assert_eq!(genre_guard(&p, "nogen", ranked).len(), 2);
    }

    #[test]
    fn hydration_and_vectors_stamp() {
        let p = seeded();
        // items_by_ids drops unknown ids (and show ids, which have no items row).
        let items = items_by_ids(&p, &["c1", "ghost", "sh1"]).unwrap();
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["c1"]);

        // show_ids_for projects episodes to their parent show; movies and unknown
        // ids contribute nothing, and an empty input never hits the DB.
        assert_eq!(show_ids_for(&p, &["e1", "c1", "ghost"]).unwrap(), vec!["sh1".to_string()]);
        assert!(show_ids_for(&p, &[]).unwrap().is_empty());

        // entities_by_ids mixes movies + shows, order preserved, unknowns dropped.
        let ents = entities_by_ids(&p, &["c1", "sh1", "ghost"]).unwrap();
        assert_eq!(ents.len(), 2);
        assert!(matches!(&ents[0], SectionItem::Movie { item } if item.id == "c1"));
        assert!(matches!(&ents[1], SectionItem::Show { show } if show.id == "sh1"));

        // Vector staleness stamp: None until a vector exists.
        assert!(vectors_max_updated_at(&p).unwrap().is_none());
        p.get()
            .unwrap()
            .execute("INSERT INTO item_vectors (id,dim,vec,updated_at) VALUES ('c1',2,x'0000','2026-01-01')", [])
            .unwrap();
        assert_eq!(vectors_max_updated_at(&p).unwrap().as_deref(), Some("2026-01-01"));
    }
}
