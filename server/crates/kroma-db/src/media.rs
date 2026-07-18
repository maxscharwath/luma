//! Catalog reads: counts, movie/item/show listings and single-item/show fetches.

use super::*;

use kroma_domain::{Season, Show, ShowDetail};

/// `SELECT … FROM shows s` with the season/episode-count correlated subqueries;
/// callers append their own `WHERE`/`ORDER BY` and map rows with
/// [`row_to_show_counted`].
const SHOWS_COUNTED_SELECT: &str = "SELECT s.id,s.title,s.year,s.library,s.added_at,\
    (SELECT COUNT(DISTINCT i.season) FROM items i WHERE i.show_id=s.id),\
    (SELECT COUNT(*) FROM items i WHERE i.show_id=s.id),\
    s.metadata \
 FROM shows s";

/// Map a `shows` row selected via [`SHOWS_COUNTED_SELECT`]
/// (`id,title,year,library,added_at,season_count,episode_count,metadata`) into a
/// [`Show`]; its representative `video` is filled in afterwards.
fn row_to_show_counted(r: &Row) -> rusqlite::Result<Show> {
    Ok(Show {
        id: r.get(0)?,
        title: r.get(1)?,
        year: r.get(2)?,
        library: r.get(3)?,
        added_at: r.get(4)?,
        season_count: r.get::<_, i64>(5)? as u32,
        episode_count: r.get::<_, i64>(6)? as u32,
        video: None,
        metadata: parse_metadata(r.get(7)?),
        progress: None,
    })
}

/// Map a `shows` row selected as `id,title,year,library,added_at,metadata` (no
/// count subqueries) into a [`Show`] with zeroed season/episode counts (the
/// caller fills real counts in later when it needs them).
fn row_to_show_bare(r: &Row) -> rusqlite::Result<Show> {
    Ok(Show {
        id: r.get(0)?,
        title: r.get(1)?,
        year: r.get(2)?,
        library: r.get(3)?,
        added_at: r.get(4)?,
        season_count: 0,
        episode_count: 0,
        video: None,
        metadata: parse_metadata(r.get(5)?),
        progress: None,
    })
}

/// Map five consecutive stream columns starting at `base`
/// (`v_codec,v_width,v_height,v_hdr,v_bit_depth`) into a [`VideoStream`].
fn row_to_video_at(r: &Row, base: usize) -> rusqlite::Result<VideoStream> {
    Ok(VideoStream {
        codec: r.get::<_, String>(base)?,
        width: r.get(base + 1)?,
        height: r.get(base + 2)?,
        hdr: r.get::<_, Option<i64>>(base + 3)?.unwrap_or(0) != 0,
        bit_depth: r.get(base + 4)?,
    })
}

/// (libraries, items, shows) counts for `/api/health`.
pub fn counts(pool: &Pool) -> Result<(usize, usize, usize)> {
    let conn = pool.get()?;
    let libs: i64 = conn.query_row("SELECT COUNT(*) FROM libraries", [], |r| r.get(0))?;
    let items: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
    let shows: i64 = conn.query_row("SELECT COUNT(*) FROM shows", [], |r| r.get(0))?;
    Ok((libs as usize, items as usize, shows as usize))
}

/// Movies (and loose videos) everything that isn't an episode.
pub fn list_movies(pool: &Pool, library: Option<&str>) -> Result<Vec<MediaItem>> {
    query_items(
        pool,
        &format!("SELECT {ITEM_COLS} FROM items WHERE kind != 'episode'"),
        library,
        "ORDER BY title COLLATE NOCASE",
    )
}

/// All playable items (movies + episodes) backwards-compatible `/api/items`.
pub fn list_items(pool: &Pool, library: Option<&str>) -> Result<Vec<MediaItem>> {
    query_items(
        pool,
        &format!("SELECT {ITEM_COLS} FROM items"),
        library,
        "ORDER BY title COLLATE NOCASE",
    )
}

pub fn get_item(pool: &Pool, id: &str) -> Result<Option<MediaItem>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_item)?;
    match rows.next() {
        Some(item) => {
            let mut item = item?;
            attach_files(&conn, &mut item)?;
            Ok(Some(item))
        }
        None => Ok(None),
    }
}

pub fn list_shows(pool: &Pool, library: Option<&str>) -> Result<Vec<Show>> {
    let conn = pool.get()?;
    let (where_sql, want_lib) = match library {
        Some(_) => ("WHERE s.library = ?1", true),
        None => ("", false),
    };
    let sql = format!("{SHOWS_COUNTED_SELECT} {where_sql} ORDER BY s.title COLLATE NOCASE");
    let mut stmt = conn.prepare(&sql)?;

    let mut shows: Vec<Show> = if want_lib {
        stmt.query_map(params![library.unwrap()], row_to_show_counted)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], row_to_show_counted)?.collect::<rusqlite::Result<Vec<_>>>()?
    };

    apply_representative_videos(&conn, &mut shows)?;
    Ok(shows)
}

/// Lightweight catalogue snapshot for the search index: `(items, shows)` with
/// only the fields the index reads (title, show/episode title, metadata) and
/// none of the per-row file / representative-video lookups [`list_movies`] /
/// [`list_shows`] do so a full reindex is just two table scans.
pub fn index_snapshot(pool: &Pool) -> Result<(Vec<MediaItem>, Vec<Show>)> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items"))?;
    let items: Vec<MediaItem> =
        stmt.query_map([], row_to_item)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare("SELECT id,title,year,library,added_at,metadata FROM shows")?;
    let shows: Vec<Show> = stmt
        .query_map([], row_to_show_bare)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((items, shows))
}

/// Fetch full items for a set of ids (search-result hydration). Order is
/// unspecified the caller re-orders by relevance.
pub fn get_items_by_ids(pool: &Pool, ids: &[String]) -> Result<Vec<MediaItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = pool.get()?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT {ITEM_COLS} FROM items WHERE id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let mut items: Vec<MediaItem> = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    attach_files_batch(&conn, &mut items)?;
    Ok(items)
}

/// Fetch full shows (with season/episode counts + representative video) for a set
/// of ids. Order is unspecified the caller re-orders by relevance.
pub fn get_shows_by_ids(pool: &Pool, ids: &[String]) -> Result<Vec<Show>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = pool.get()?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("{SHOWS_COUNTED_SELECT} WHERE s.id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let mut shows: Vec<Show> = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), row_to_show_counted)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    apply_representative_videos(&conn, &mut shows)?;
    Ok(shows)
}

/// Ids of every movie + show crediting `name` in its cast OR key crew, matched
/// case-insensitively over the metadata JSON. Returns `(movie_ids, show_ids)`;
/// episodes are excluded (they inherit a show's credits). Powers `GET /api/people`
/// "everything this actor/director appears in or worked on".
pub fn titles_by_person(pool: &Pool, name: &str) -> Result<(Vec<String>, Vec<String>)> {
    let name = name.trim();
    if name.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let conn = pool.get()?;
    let movie_ids =
        person_ids(&conn, "SELECT id FROM items WHERE kind != 'episode' AND metadata IS NOT NULL AND (", name)?;
    let show_ids = person_ids(&conn, "SELECT id FROM shows WHERE metadata IS NOT NULL AND (", name)?;
    Ok((movie_ids, show_ids))
}

/// Run the shared "credited as `name`" EXISTS predicate (cast OR crew) appended to
/// a table-specific `prefix`, returning the matching ids.
fn person_ids(conn: &rusqlite::Connection, prefix: &str, name: &str) -> Result<Vec<String>> {
    let sql = format!(
        "{prefix} \
         EXISTS (SELECT 1 FROM json_each(metadata,'$.cast') c WHERE json_extract(c.value,'$.name') = ?1 COLLATE NOCASE) OR \
         EXISTS (SELECT 1 FROM json_each(metadata,'$.crew') c WHERE json_extract(c.value,'$.name') = ?1 COLLATE NOCASE))"
    );
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt
        .query_map(params![name], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// Cheap title lookup for show poster rendering.
pub fn show_title(pool: &Pool, id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    Ok(conn
        .query_row("SELECT title FROM shows WHERE id = ?1", params![id], |r| r.get(0))
        .ok())
}

pub fn get_show(pool: &Pool, id: &str) -> Result<Option<ShowDetail>> {
    let conn = pool.get()?;
    let show = conn
        .query_row(
            "SELECT id,title,year,library,added_at,metadata FROM shows WHERE id = ?1",
            params![id],
            row_to_show_bare,
        )
        .ok();

    let Some(mut show) = show else { return Ok(None) };

    let mut stmt = conn.prepare(&format!(
        "SELECT {ITEM_COLS} FROM items WHERE show_id = ?1 \
         ORDER BY season, episode",
    ))?;
    let mut episodes: Vec<MediaItem> = stmt
        .query_map(params![id], row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    attach_files_batch(&conn, &mut episodes)?;

    // Group into seasons.
    let mut seasons: Vec<Season> = Vec::new();
    for ep in episodes.iter().cloned() {
        let n = ep.season.unwrap_or(0);
        match seasons.iter_mut().find(|s| s.number == n) {
            Some(s) => s.episodes.push(ep),
            None => seasons.push(Season { number: n, episodes: vec![ep], cast: Vec::new() }),
        }
    }
    seasons.sort_by_key(|s| s.number);

    // Attach per-season cast (TMDB season credits), resolved during enrichment.
    let mut casts = season_casts(pool, id)?;
    for s in &mut seasons {
        if let Some(cast) = casts.remove(&s.number) {
            s.cast = cast;
        }
    }

    show.episode_count = episodes.len() as u32;
    show.season_count = seasons.len() as u32;
    show.video = representative_video(&conn, id)?;

    Ok(Some(ShowDetail { show, seasons }))
}

/// [`representative_video`] over a whole listing in one query per id-chunk:
/// rows arrive widest-first, so the first row seen per show wins exactly the
/// per-show `ORDER BY v_width DESC LIMIT 1` the single-show query does.
fn apply_representative_videos(conn: &rusqlite::Connection, shows: &mut [Show]) -> Result<()> {
    if shows.is_empty() {
        return Ok(());
    }
    use std::collections::HashMap;
    let ids: Vec<&str> = shows.iter().map(|s| s.id.as_str()).collect();
    let mut best: HashMap<String, VideoStream> = HashMap::new();
    for chunk in ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT i.show_id,f.v_codec,f.v_width,f.v_height,f.v_hdr,f.v_bit_depth \
             FROM files f JOIN items i ON f.item_id = i.id \
             WHERE i.show_id IN ({ph}) AND f.probed = 1 AND f.v_codec IS NOT NULL \
             ORDER BY f.v_width DESC NULLS LAST",
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
            Ok((r.get::<_, String>(0)?, row_to_video_at(r, 1)?))
        })?;
        for row in rows {
            let (show_id, video) = row?;
            best.entry(show_id).or_insert(video);
        }
    }
    for s in shows.iter_mut() {
        s.video = best.remove(&s.id);
    }
    Ok(())
}

/// Pick a representative video stream for a show the highest-resolution probed
/// file across all of the show's episodes.
fn representative_video(conn: &rusqlite::Connection, show_id: &str) -> Result<Option<VideoStream>> {
    let mut stmt = conn.prepare(
        "SELECT f.v_codec,f.v_width,f.v_height,f.v_hdr,f.v_bit_depth \
         FROM files f JOIN items i ON f.item_id = i.id \
         WHERE i.show_id = ?1 AND f.probed = 1 AND f.v_codec IS NOT NULL \
         ORDER BY f.v_width DESC NULLS LAST LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![show_id], |r| row_to_video_at(r, 0))?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

fn query_items(pool: &Pool, base: &str, library: Option<&str>, tail: &str) -> Result<Vec<MediaItem>> {
    let conn = pool.get()?;
    let mut items: Vec<MediaItem> = match library {
        Some(lib) => {
            let sql = format!("{base} {} {tail}", if base.contains("WHERE") { "AND library = ?1" } else { "WHERE library = ?1" });
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![lib], row_to_item)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            let sql = format!("{base} {tail}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], row_to_item)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    attach_files_batch(&conn, &mut items)?;
    Ok(items)
}
