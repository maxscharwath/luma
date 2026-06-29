//! Catalog writes: metadata attachment, ffprobe results and the scan diff-sync.

use std::collections::HashMap;

use super::*;

use crate::model::{Library, LibraryKind, Metadata, Show};

/// Attach resolved TMDB metadata to one item (used by the enrichment pass).
pub fn set_item_metadata(pool: &Pool, id: &str, meta: &Metadata) -> Result<()> {
    let conn = pool.get()?;
    let json = serde_json::to_string(meta)?;
    conn.execute("UPDATE items SET metadata = ?2 WHERE id = ?1", params![id, json])?;
    Ok(())
}

/// Attach resolved TMDB metadata to one show (used by the enrichment pass).
pub fn set_show_metadata(pool: &Pool, id: &str, meta: &Metadata) -> Result<()> {
    let conn = pool.get()?;
    let json = serde_json::to_string(meta)?;
    conn.execute("UPDATE shows SET metadata = ?2 WHERE id = ?1", params![id, json])?;
    Ok(())
}

/// (file_id, abs_path, owning item_id) for every file awaiting an ffprobe pass.
/// Drives the phase-2 background probing.
pub fn unprobed_files(pool: &Pool) -> Result<Vec<(String, String, String)>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT id, abs_path, item_id FROM files WHERE probed = 0")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Whether a given item already has at least one probed file (used to decide
/// whether a probe is the *first* one for an item → emit an ItemUpdated).
pub fn item_has_probed_file(pool: &Pool, item_id: &str) -> Result<bool> {
    let conn = pool.get()?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE item_id = ?1 AND probed = 1",
        params![item_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Persist the probe result for one file (sets stream columns + `probed=1`),
/// then recompute the owning item's representative columns.
pub fn set_file_probe(
    pool: &Pool,
    file_id: &str,
    duration_ms: Option<u64>,
    video: Option<&VideoStream>,
    audio: Option<&AudioStream>,
    audio_tracks: &[AudioStream],
    subtitles: &[SubtitleTrack],
) -> Result<()> {
    let conn = pool.get()?;
    let subs = serde_json::to_string(subtitles).unwrap_or_else(|_| "[]".into());
    let a_tracks = serde_json::to_string(audio_tracks).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE files SET probed=1, duration_ms=?2, \
            v_codec=?3, v_width=?4, v_height=?5, v_hdr=?6, v_bit_depth=?7, \
            a_codec=?8, a_channels=?9, a_language=?10, subtitles=?11, audio_tracks=?12 \
         WHERE id = ?1",
        params![
            file_id,
            duration_ms.map(|d| d as i64),
            video.map(|v| v.codec.clone()),
            video.and_then(|v| v.width),
            video.and_then(|v| v.height),
            video.map(|v| v.hdr as i64),
            video.and_then(|v| v.bit_depth),
            audio.map(|a| a.codec.clone()),
            audio.and_then(|a| a.channels),
            audio.and_then(|a| a.language.clone()),
            subs,
            a_tracks,
        ],
    )?;

    // Recompute the owning item's representative columns.
    let item_id: Option<String> = conn
        .query_row("SELECT item_id FROM files WHERE id = ?1", params![file_id], |r| r.get(0))
        .ok();
    if let Some(item_id) = item_id {
        recompute_item_representative(&conn, &item_id)?;
    }
    Ok(())
}

/// Recompute one item's representative (top-level) columns from its
/// highest-resolution probed file: container/duration/video/audio/subtitles and
/// the representative `abs_path`/`rel_path`.
fn recompute_item_representative(conn: &Connection, item_id: &str) -> Result<()> {
    // Best probed file for this item = highest v_width (then any probed).
    let best: Option<(String, String, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT abs_path, container, rel_path, duration_ms FROM files \
             WHERE item_id = ?1 AND probed = 1 \
             ORDER BY v_width DESC NULLS LAST, id LIMIT 1",
            params![item_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .ok();

    if let Some((abs, container, rel, duration)) = best {
        conn.execute(
            "UPDATE items SET container=?2, abs_path=?3, rel_path=?4, duration_ms=?5 WHERE id=?1",
            params![item_id, container, abs, rel, duration],
        )?;
    }
    Ok(())
}

/// Recompute representative columns for every item that has a probed file.
fn recompute_all_representatives(pool: &Pool) -> Result<()> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT item_id FROM files WHERE probed = 1",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        recompute_item_representative(&conn, &id)?;
    }
    Ok(())
}

/// Diff-sync the scanned index into the DB in one transaction.
///
/// Unlike a blunt DELETE-all + INSERT, this PRESERVES the expensive bits across
/// rescans:
///   * `items.metadata` / `shows.metadata` (TMDB art) is never overwritten.
///   * A file's probed stream data is kept when its `size`+`mtime` are unchanged;
///     only new or modified files get `probed=0` and will be re-probed.
///
/// `mtimes` maps file id → unix-seconds mtime collected during the scan (see
/// [`crate::services::scan::take_mtimes`]). `items` carry their `files[]`.
pub fn sync_all(
    pool: &Pool,
    libraries: &[Library],
    shows: &[Show],
    items: &[MediaItem],
    mtimes: &HashMap<String, Option<i64>>,
) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;

    // 1) Libraries — UPSERT by id. We must NOT `DELETE FROM libraries` wholesale:
    //    `items`/`files` cascade-delete from libraries, which would wipe all the
    //    precious probed data and metadata we're trying to preserve. Instead
    //    upsert each library, then delete only libraries no longer scanned (whose
    //    cascade is the correct behaviour — their items are gone too).
    {
        let mut lib_stmt = tx.prepare(
            "INSERT INTO libraries (id,name,kind,path,added_at) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, path=excluded.path",
        )?;
        for l in libraries {
            lib_stmt.execute(params![l.id, l.name, library_kind_str(&l.kind), l.path, now_or_blank()])?;
        }
        // Delete libraries that vanished from the scan (cascades their items/files).
        let keep: Vec<String> = libraries.iter().map(|l| l.id.clone()).collect();
        let mut existing: Vec<String> = Vec::new();
        {
            let mut q = tx.prepare("SELECT id FROM libraries")?;
            let rows = q.query_map([], |r| r.get::<_, String>(0))?;
            for r in rows {
                existing.push(r?);
            }
        }
        let mut del = tx.prepare("DELETE FROM libraries WHERE id = ?1")?;
        for id in &existing {
            if !keep.contains(id) {
                del.execute(params![id])?;
            }
        }
    }

    // 2) Shows — upsert without ever touching `metadata`.
    {
        let mut show_stmt = tx.prepare(
            "INSERT INTO shows (id,library,title,year,added_at) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET library=excluded.library, title=excluded.title, \
                 year=COALESCE(excluded.year, shows.year)",
        )?;
        for s in shows {
            show_stmt.execute(params![s.id, s.library, s.title, s.year, s.added_at])?;
        }
    }

    // 3) Items — upsert without ever touching `metadata`.
    {
        let mut item_stmt = tx.prepare(
            "INSERT INTO items \
                (id,kind,title,year,container,library,show_id,show_title,\
                 season,episode,episode_end,episode_title,rel_path,abs_path,added_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) \
             ON CONFLICT(id) DO UPDATE SET \
                 kind=excluded.kind, title=excluded.title, year=excluded.year, \
                 library=excluded.library, show_id=excluded.show_id, \
                 show_title=excluded.show_title, season=excluded.season, \
                 episode=excluded.episode, episode_end=excluded.episode_end, \
                 episode_title=excluded.episode_title",
        )?;
        for i in items {
            // The item's container/rel_path/abs_path mirror its (first) file until
            // probing recomputes the representative; pick the first file as seed.
            let seed = i.files.first();
            let container = seed.map(|f| f.container.clone()).unwrap_or_default();
            let rel_path = seed.and_then(|f| f.rel_path.clone());
            let abs_path = seed.and_then(|f| f.abs_path.clone());
            item_stmt.execute(params![
                i.id,
                kind_str(&i.kind),
                i.title,
                i.year,
                container,
                i.library,
                i.show_id,
                i.show_title,
                i.season,
                i.episode,
                i.episode_end,
                i.episode_title,
                rel_path,
                abs_path,
                i.added_at,
            ])?;
        }
    }

    // 4) Files — diff sync by abs_path. Delete files no longer on disk; upsert
    //    scanned files, resetting `probed=0` only when size/mtime changed.
    {
        // Build the set of abs_paths we just scanned.
        let scanned: std::collections::HashSet<&str> = items
            .iter()
            .flat_map(|i| i.files.iter())
            .filter_map(|f| f.abs_path.as_deref())
            .collect();

        // Delete DB file rows whose abs_path is gone from disk.
        let mut existing: Vec<(String, String)> = Vec::new();
        {
            let mut q = tx.prepare("SELECT id, abs_path FROM files")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for r in rows {
                existing.push(r?);
            }
        }
        {
            let mut del = tx.prepare("DELETE FROM files WHERE id = ?1")?;
            for (id, abs) in &existing {
                if !scanned.contains(abs.as_str()) {
                    del.execute(params![id])?;
                }
            }
        }

        // Existing (size, mtime, probed) keyed by abs_path, to decide reuse.
        let mut prev: HashMap<String, (Option<i64>, Option<i64>, i64)> = HashMap::new();
        {
            let mut q = tx.prepare("SELECT abs_path, size, mtime, probed FROM files")?;
            let rows = q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?;
            for r in rows {
                let (abs, size, mtime, probed) = r?;
                prev.insert(abs, (size, mtime, probed));
            }
        }

        // Upsert each scanned file. When size+mtime match an already-probed row,
        // keep probed=1 and DON'T touch its stream columns. Otherwise reset.
        let mut keep_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition",
        )?;
        let mut reset_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition,probed,\
                 duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,a_codec,a_channels,a_language,subtitles,audio_tracks) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'[]','[]') \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition, probed=0, duration_ms=NULL, v_codec=NULL, v_width=NULL, \
                 v_height=NULL, v_hdr=NULL, v_bit_depth=NULL, a_codec=NULL, a_channels=NULL, \
                 a_language=NULL, subtitles='[]', audio_tracks='[]'",
        )?;
        // Files that arrive already probed (demo/seed content): store their stream
        // data directly as probed=1 so they never enter the phase-2 pass.
        let mut preprobed_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition,probed,\
                 duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,a_codec,a_channels,a_language,subtitles,audio_tracks) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19) \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition, probed=1, duration_ms=excluded.duration_ms, \
                 v_codec=excluded.v_codec, v_width=excluded.v_width, v_height=excluded.v_height, \
                 v_hdr=excluded.v_hdr, v_bit_depth=excluded.v_bit_depth, a_codec=excluded.a_codec, \
                 a_channels=excluded.a_channels, a_language=excluded.a_language, subtitles=excluded.subtitles, \
                 audio_tracks=excluded.audio_tracks",
        )?;

        for i in items {
            for f in &i.files {
                let Some(abs) = f.abs_path.as_deref() else { continue };
                let size = f.size.map(|s| s as i64);
                let mtime = mtimes.get(&f.id).copied().flatten();

                if f.probed {
                    // Pre-probed (demo): store the supplied stream data.
                    let v = f.video.as_ref();
                    let a = f.audio.as_ref();
                    let subs = serde_json::to_string(&f.subtitles).unwrap_or_else(|_| "[]".into());
                    let a_tracks = serde_json::to_string(&f.audio_tracks).unwrap_or_else(|_| "[]".into());
                    preprobed_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                        f.duration_ms.map(|d| d as i64),
                        v.map(|v| v.codec.clone()),
                        v.and_then(|v| v.width),
                        v.and_then(|v| v.height),
                        v.map(|v| v.hdr as i64),
                        v.and_then(|v| v.bit_depth),
                        a.map(|a| a.codec.clone()),
                        a.and_then(|a| a.channels),
                        a.and_then(|a| a.language.clone()),
                        subs,
                        a_tracks,
                    ])?;
                    continue;
                }

                let unchanged_probed = prev.get(abs).is_some_and(|(psize, pmtime, probed)| {
                    *probed == 1 && *psize == size && *pmtime == mtime
                });
                if unchanged_probed {
                    keep_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                    ])?;
                } else {
                    reset_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                    ])?;
                }
            }
        }
    }

    // 5) Prune items/shows that now have zero backing files/episodes.
    tx.execute("DELETE FROM items WHERE id NOT IN (SELECT DISTINCT item_id FROM files)", [])?;
    tx.execute("DELETE FROM shows WHERE id NOT IN (SELECT DISTINCT show_id FROM items WHERE show_id IS NOT NULL)", [])?;

    tx.commit()?;

    // 6) Recompute every item's representative columns from its probed files.
    recompute_all_representatives(pool)?;
    // 7) Drop content embeddings for titles that vanished from the catalog.
    let _ = prune_orphan_vectors(pool);
    Ok(())
}

fn kind_str(k: &Kind) -> &'static str {
    match k {
        Kind::Movie => "movie",
        Kind::Episode => "episode",
        Kind::Video => "video",
    }
}

fn library_kind_str(k: &LibraryKind) -> &'static str {
    match k {
        LibraryKind::Movies => "movies",
        LibraryKind::Shows => "shows",
        LibraryKind::Mixed => "mixed",
    }
}
