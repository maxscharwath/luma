//! Playback progress: per-user saved positions and the "continue watching" join.

use super::*;

use kroma_domain::{ContinueItem, Kind, ProgressEntry};

/// Upsert one item's playback position for a user.
pub fn upsert_progress(
    pool: &Pool,
    user_id: &str,
    item_id: &str,
    position_ms: i64,
    duration_ms: Option<i64>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO progress (user_id,item_id,position_ms,duration_ms,updated_at) \
         VALUES (?1,?2,?3,?4,?5) \
         ON CONFLICT(user_id,item_id) DO UPDATE SET \
            position_ms=excluded.position_ms, duration_ms=excluded.duration_ms, \
            updated_at=excluded.updated_at",
        params![user_id, item_id, position_ms, duration_ms, now_or_blank()],
    )?;
    Ok(())
}

/// One item's saved progress for a user, if any.
pub fn get_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<Option<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 AND item_id = ?2",
    )?;
    let mut rows = stmt.query_map(params![user_id, item_id], row_to_progress)?;
    match rows.next() {
        Some(p) => Ok(Some(p?)),
        None => Ok(None),
    }
}

/// Every saved progress row for a user (newest first).
pub fn list_progress(pool: &Pool, user_id: &str) -> Result<Vec<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map(params![user_id], row_to_progress)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Remove a saved position (e.g. finished, or "remove from Continue Watching").
pub fn delete_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM progress WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// "Continue watching": resumable items (started, not yet ~finished), newest
/// first, each carried as a full [`MediaItem`] so clients render normal cards.
pub fn continue_watching(pool: &Pool, user_id: &str) -> Result<Vec<ContinueItem>> {
    let conn = pool.get()?;
    // 1) The resumable item ids + their progress. The JOIN drops any orphan
    //    progress row whose item no longer exists.
    let mut stmt = conn.prepare(
        "SELECT p.item_id,p.position_ms,p.duration_ms,p.updated_at \
         FROM progress p JOIN items i ON i.id = p.item_id \
         WHERE p.user_id = ?1 AND p.position_ms > 15000 \
           AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
         ORDER BY p.updated_at DESC LIMIT 30",
    )?;
    let rows: Vec<(String, i64, Option<i64>, String)> = stmt
        .query_map(params![user_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // 2) Hydrate all ids in one batched pass (files + markers included).
    let ids: Vec<&str> = rows.iter().map(|(id, _, _, _)| id.as_str()).collect();
    let items = items_by_ids_ordered(&conn, &ids)?;
    let mut by_id: std::collections::HashMap<String, MediaItem> =
        items.into_iter().map(|i| (i.id.clone(), i)).collect();

    // 3) Episodes carry no poster of their own, so a Continue tile would fall
    //    back to a placeholder. Borrow the parent show's artwork (keeping any
    //    episode-specific still as the backdrop) one query for all shows.
    let show_ids: Vec<String> = by_id
        .values()
        .filter(|i| i.kind == Kind::Episode)
        .filter_map(|i| i.show_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut show_meta_by_id: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for chunk in show_ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt =
            conn.prepare(&format!("SELECT id, metadata FROM shows WHERE id IN ({ph})"))?;
        let metas = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in metas {
            let (id, json) = row?;
            show_meta_by_id.insert(id, json);
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    for (item_id, position_ms, duration_ms, updated_at) in rows {
        let Some(mut item) = by_id.remove(&item_id) else { continue };
        if item.kind == Kind::Episode {
            let json = item
                .show_id
                .as_ref()
                .and_then(|sid| show_meta_by_id.get(sid).cloned())
                .flatten();
            if let Some(mut show_meta) = parse_metadata(json) {
                if let Some(still) = item.metadata.as_ref().and_then(|m| m.backdrop_url.clone()) {
                    show_meta.backdrop_url = Some(still);
                }
                item.metadata = Some(show_meta);
            }
        }
        out.push(ContinueItem { item, position_ms, duration_ms, updated_at });
    }
    Ok(out)
}

// ----- watched (explicit "seen" marker, independent of resume position) -------

/// Mark an item as watched for a user, and drop any resume position so it leaves
/// "Continue watching". Idempotent (re-marking just refreshes `watched_at`).
pub fn mark_watched(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO watched (user_id,item_id,watched_at) VALUES (?1,?2,?3) \
         ON CONFLICT(user_id,item_id) DO UPDATE SET watched_at=excluded.watched_at",
        params![user_id, item_id, now_or_blank()],
    )?;
    conn.execute(
        "DELETE FROM progress WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// Clear an item's watched flag for a user. Idempotent.
pub fn unmark_watched(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM watched WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// Every item id the user has marked (or finished) as watched clients hydrate a
/// set once and badge cards from it.
pub fn list_watched(pool: &Pool, user_id: &str) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT item_id FROM watched WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ----- my list ("Ma liste" user bookmarks, synced across clients) -----------

/// Add a title to the user's list. Idempotent (re-adding refreshes `added_at`).
pub fn add_to_list(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO my_list (user_id,item_id,added_at) VALUES (?1,?2,?3) \
         ON CONFLICT(user_id,item_id) DO UPDATE SET added_at=excluded.added_at",
        params![user_id, item_id, now_or_blank()],
    )?;
    Ok(())
}

/// Remove a title from the user's list. Idempotent.
pub fn remove_from_list(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM my_list WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// Every item id in the user's list, most-recently-added first.
pub fn list_my_list(pool: &Pool, user_id: &str) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT item_id FROM my_list WHERE user_id = ?1 ORDER BY added_at DESC")?;
    let rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Per-user progress through each show, as a percent 0–100 (only shows with >0).
/// `(watched episodes + the in-progress episode's fraction) / total episodes`
/// a Plex-style series completion bar for show cards.
pub fn show_progress(pool: &Pool, user_id: &str) -> Result<std::collections::HashMap<String, u8>> {
    use std::collections::HashMap;
    let conn = pool.get()?;

    // Total episodes per show.
    let mut totals: HashMap<String, i64> = HashMap::new();
    {
        let mut s = conn.prepare(
            "SELECT show_id, COUNT(*) FROM items \
             WHERE kind = 'episode' AND show_id IS NOT NULL GROUP BY show_id",
        )?;
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (show, n) = row?;
            totals.insert(show, n);
        }
    }

    // Watched episodes per show.
    let mut watched: HashMap<String, i64> = HashMap::new();
    {
        let mut s = conn.prepare(
            "SELECT i.show_id, COUNT(*) FROM watched w JOIN items i ON i.id = w.item_id \
             WHERE w.user_id = ?1 AND i.kind = 'episode' AND i.show_id IS NOT NULL GROUP BY i.show_id",
        )?;
        for row in s.query_map(params![user_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (show, n) = row?;
            watched.insert(show, n);
        }
    }

    // Most-recent in-progress episode's fraction per show (watched + in-progress are
    // disjoint: mark_watched deletes the progress row).
    let mut frac: HashMap<String, f64> = HashMap::new();
    {
        let mut s = conn.prepare(
            "SELECT i.show_id, p.position_ms, p.duration_ms FROM progress p JOIN items i ON i.id = p.item_id \
             WHERE p.user_id = ?1 AND i.kind = 'episode' AND i.show_id IS NOT NULL AND p.position_ms > 15000 \
               AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
             ORDER BY p.updated_at DESC",
        )?;
        for row in s.query_map(params![user_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?))
        })? {
            let (show, pos, dur) = row?;
            // ORDER BY updated_at DESC → keep the first (most recent) per show.
            frac.entry(show).or_insert_with(|| match dur {
                Some(d) if d > 0 => (pos as f64 / d as f64).clamp(0.0, 1.0),
                _ => 0.0,
            });
        }
    }

    let mut out = HashMap::new();
    for (show, total) in totals {
        if total <= 0 {
            continue;
        }
        let w = *watched.get(&show).unwrap_or(&0) as f64;
        let f = *frac.get(&show).unwrap_or(&0.0);
        let pct = ((w + f) / total as f64 * 100.0).round().clamp(0.0, 100.0) as u8;
        if pct > 0 {
            out.insert(show, pct);
        }
    }
    Ok(out)
}

/// Series-completion percent (0–100) for a single show, or `None` if no progress
/// (lighter than [`show_progress`] for a one-show detail page).
pub fn show_progress_one(pool: &Pool, user_id: &str, show_id: &str) -> Result<Option<u8>> {
    let conn = pool.get()?;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM items WHERE kind = 'episode' AND show_id = ?1",
        params![show_id],
        |r| r.get(0),
    )?;
    if total <= 0 {
        return Ok(None);
    }
    let watched: i64 = conn.query_row(
        "SELECT COUNT(*) FROM watched w JOIN items i ON i.id = w.item_id \
         WHERE w.user_id = ?1 AND i.kind = 'episode' AND i.show_id = ?2",
        params![user_id, show_id],
        |r| r.get(0),
    )?;
    let frac = {
        let mut s = conn.prepare(
            "SELECT p.position_ms, p.duration_ms FROM progress p JOIN items i ON i.id = p.item_id \
             WHERE p.user_id = ?1 AND i.show_id = ?2 AND i.kind = 'episode' AND p.position_ms > 15000 \
               AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
             ORDER BY p.updated_at DESC LIMIT 1",
        )?;
        let row = s
            .query_map(params![user_id, show_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
            })?
            .next()
            .transpose()?;
        match row {
            Some((pos, Some(d))) if d > 0 => (pos as f64 / d as f64).clamp(0.0, 1.0),
            _ => 0.0,
        }
    };
    let pct = ((watched as f64 + frac) / total as f64 * 100.0).round().clamp(0.0, 100.0) as u8;
    Ok(if pct > 0 { Some(pct) } else { None })
}

/// Map a row of `item_id,position_ms,duration_ms,updated_at` to a [`ProgressEntry`].
fn row_to_progress(r: &Row) -> rusqlite::Result<ProgressEntry> {
    Ok(ProgressEntry {
        item_id: r.get(0)?,
        position_ms: r.get(1)?,
        duration_ms: r.get(2)?,
        updated_at: r.get(3)?,
    })
}

// ----- continue a series / next episode ---------------------------------------

/// The episode to play to CONTINUE a show, for a user: the most-recent in-progress
/// episode (resume), else the first unwatched episode in order, else the first.
/// Returns the hydrated episode plus whether it has a saved resume position.
pub fn up_next_episode(
    pool: &Pool,
    user_id: &str,
    show_id: &str,
) -> Result<Option<(MediaItem, bool)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {ITEM_COLS} FROM items \
         WHERE show_id = ?1 AND kind = 'episode' ORDER BY season, episode"
    ))?;
    let episodes: Vec<MediaItem> = stmt
        .query_map(params![show_id], row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if episodes.is_empty() {
        return Ok(None);
    }

    // 1) Resume: the most-recently-updated in-progress episode of this show.
    let mut rs = conn.prepare(
        "SELECT p.item_id FROM progress p JOIN items i ON i.id = p.item_id \
         WHERE p.user_id = ?1 AND i.show_id = ?2 AND p.position_ms > 15000 \
           AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
         ORDER BY p.updated_at DESC LIMIT 1",
    )?;
    let resume_id = rs
        .query_map(params![user_id, show_id], |r| r.get::<_, String>(0))?
        .next()
        .transpose()?;
    drop(rs);

    let (mut chosen, resume) = if let Some(id) = resume_id {
        match episodes.iter().find(|e| e.id == id).cloned() {
            Some(e) => (e, true),
            None => (episodes[0].clone(), false),
        }
    } else {
        // 2) The episode AFTER the last (highest, by season/episode) watched one
        //    so finishing E2 continues at E3 even if an earlier episode is unwatched
        //    (Plex/Netflix "on deck"). Caught up / nothing watched → the first.
        let mut ws = conn.prepare(
            "SELECT w.item_id FROM watched w JOIN items i ON i.id = w.item_id \
             WHERE w.user_id = ?1 AND i.show_id = ?2",
        )?;
        let seen: std::collections::HashSet<String> = ws
            .query_map(params![user_id, show_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(ws);
        // `episodes` is ordered by (season, episode); the next after the last seen.
        let next = match episodes.iter().rposition(|e| seen.contains(&e.id)) {
            Some(i) => episodes.get(i + 1).or_else(|| episodes.first()).cloned(),
            None => episodes.first().cloned(),
        };
        (next.unwrap_or_else(|| episodes[0].clone()), false)
    };

    attach_files(&conn, &mut chosen)?;
    Ok(Some((chosen, resume)))
}

/// The next episode after `item_id` in its show, by `(season, episode)` order.
/// `None` for a movie / loose video / the last episode.
pub fn next_episode(pool: &Pool, item_id: &str) -> Result<Option<MediaItem>> {
    let conn = pool.get()?;
    let coords: Option<(Option<String>, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT show_id, season, episode FROM items WHERE id = ?1",
            params![item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((Some(show_id), Some(season), Some(episode))) = coords else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT {ITEM_COLS} FROM items \
         WHERE show_id = ?1 AND kind = 'episode' \
           AND (season > ?2 OR (season = ?2 AND episode > ?3)) \
         ORDER BY season, episode LIMIT 1"
    ))?;
    let next = stmt
        .query_map(params![show_id, season, episode], row_to_item)?
        .next()
        .transpose()?;
    drop(stmt);
    let Some(mut item) = next else { return Ok(None) };
    attach_files(&conn, &mut item)?;
    Ok(Some(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kroma_domain::Permission;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// Fresh DB with one user and one movie item `m1` (so `progress` which has an
    /// items FK can be seeded).
    fn pool_with_user() -> (Pool, String) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-watched-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = crate::init(&path).unwrap();
        let user = crate::create_user(&pool, "w@e.com", "w", "hash", &[Permission::Playback]).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO libraries (id,name,kind,path,added_at) VALUES ('lib','L','movie','/x','t')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (id,kind,title,container,library,added_at) \
             VALUES ('m1','movie','Dune','mkv','lib','t')",
            [],
        )
        .unwrap();
        (pool, user.id)
    }

    #[test]
    fn mark_unmark_round_trips_and_clears_progress() {
        let (pool, uid) = pool_with_user();
        assert!(list_watched(&pool, &uid).unwrap().is_empty());

        // A resume position that mark_watched should wipe.
        upsert_progress(&pool, &uid, "m1", 60_000, Some(120_000)).unwrap();
        mark_watched(&pool, &uid, "m1").unwrap();
        assert_eq!(list_watched(&pool, &uid).unwrap(), vec!["m1".to_string()]);
        assert!(get_progress(&pool, &uid, "m1").unwrap().is_none(), "marking watched clears resume");

        // Idempotent: marking again keeps a single row.
        mark_watched(&pool, &uid, "m1").unwrap();
        assert_eq!(list_watched(&pool, &uid).unwrap().len(), 1);

        // Shows (ids not in `items`) can be marked too the column has no items FK.
        mark_watched(&pool, &uid, "show-7").unwrap();
        let mut ids = list_watched(&pool, &uid).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["m1".to_string(), "show-7".to_string()]);

        unmark_watched(&pool, &uid, "m1").unwrap();
        assert_eq!(list_watched(&pool, &uid).unwrap(), vec!["show-7".to_string()]);
    }

    #[test]
    fn my_list_add_remove_round_trips() {
        let (pool, uid) = pool_with_user();
        assert!(list_my_list(&pool, &uid).unwrap().is_empty());

        add_to_list(&pool, &uid, "m1").unwrap();
        add_to_list(&pool, &uid, "show-7").unwrap(); // show ids allowed (no items FK)
        add_to_list(&pool, &uid, "m1").unwrap(); // idempotent
        let mut ids = list_my_list(&pool, &uid).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["m1".to_string(), "show-7".to_string()]);

        remove_from_list(&pool, &uid, "m1").unwrap();
        assert_eq!(list_my_list(&pool, &uid).unwrap(), vec!["show-7".to_string()]);
    }

    // Guards the `cast` reserved-keyword quoting in season_meta SQL.
    #[test]
    fn season_cast_round_trips() {
        use kroma_domain::CastMember;
        let (pool, _uid) = pool_with_user();
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT INTO shows (id,library,title,added_at) VALUES ('s1','lib','Show','t')",
                [],
            )
            .unwrap();
        }
        assert!(crate::seasons_with_cast(&pool, "s1").unwrap().is_empty());
        let cast = vec![CastMember {
            name: "Alice".into(),
            character: Some("Lead".into()),
            profile_url: None,
        }];
        crate::set_season_cast(&pool, "s1", 1, &cast).unwrap();
        crate::set_season_cast(&pool, "s1", 1, &cast).unwrap(); // idempotent upsert
        assert!(crate::seasons_with_cast(&pool, "s1").unwrap().contains(&1));
        let casts = crate::season_casts(&pool, "s1").unwrap();
        assert_eq!(casts.get(&1).map(|c| c.len()), Some(1));
        assert_eq!(casts[&1][0].name, "Alice");
    }

    #[test]
    fn up_next_and_next_episode() {
        let (pool, uid) = pool_with_user();
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT INTO shows (id,library,title,added_at) VALUES ('s1','lib','Show','t')",
                [],
            )
            .unwrap();
            for (id, s, e) in [("e1", 1, 1), ("e2", 1, 2), ("e3", 1, 3)] {
                conn.execute(
                    "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,added_at) \
                     VALUES (?1,'episode','Ep','mkv','lib','s1',?2,?3,'t')",
                    params![id, s, e],
                )
                .unwrap();
            }
        }

        // Fresh: nothing watched / in progress → first episode, not a resume.
        let (item, resume) = up_next_episode(&pool, &uid, "s1").unwrap().unwrap();
        assert_eq!(item.id, "e1");
        assert!(!resume);

        // e1 watched → continue after the last watched = e2.
        mark_watched(&pool, &uid, "e1").unwrap();
        let (item, resume) = up_next_episode(&pool, &uid, "s1").unwrap().unwrap();
        assert_eq!(item.id, "e2");
        assert!(!resume);

        // Only e2 watched (e1 NOT) → still continue AFTER the highest watched = e3,
        // not the first unwatched (e1). This is the on-deck behaviour.
        unmark_watched(&pool, &uid, "e1").unwrap();
        mark_watched(&pool, &uid, "e2").unwrap();
        let (item, resume) = up_next_episode(&pool, &uid, "s1").unwrap().unwrap();
        assert_eq!(item.id, "e3");
        assert!(!resume);

        // e2 in progress → resume e2 (takes priority over on-deck).
        upsert_progress(&pool, &uid, "e2", 60_000, Some(600_000)).unwrap();
        let (item, resume) = up_next_episode(&pool, &uid, "s1").unwrap().unwrap();
        assert_eq!(item.id, "e2");
        assert!(resume);

        // Sequence: next after e2 is e3; e3 is last; movies have no next.
        assert_eq!(next_episode(&pool, "e2").unwrap().map(|i| i.id), Some("e3".into()));
        assert!(next_episode(&pool, "e3").unwrap().is_none());
        assert!(next_episode(&pool, "m1").unwrap().is_none());
    }
}
