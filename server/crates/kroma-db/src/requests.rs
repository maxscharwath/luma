//! Requests + wanted-ledger persistence, and the tmdbId availability lookups
//! (which ride the `idx_items_tmdb` / `idx_shows_tmdb` expression indexes the
//! json_extract expressions here must stay byte-identical to the DDL).

use rusqlite::OptionalExtension;

use super::*;
use kroma_domain::{CalendarEntry, EpisodeRef, MediaRequest, RequestKind, RequestStatus};

/// Columns of the request list SELECT (requester username joined in). `r.episodes`
/// and the Phase 2 airing columns trail the original set so existing positional
/// indices never shift.
const REQUEST_COLS: &str = "r.id, r.kind, r.tmdb_id, r.title, r.year, r.poster_url, r.seasons, \
    r.status, r.requested_by, u.username, r.reviewed_by, r.note, r.created_at, r.updated_at, \
    r.episodes, r.air_status, r.next_air_date, r.last_refresh_at";

fn row_to_request(r: &Row) -> rusqlite::Result<MediaRequest> {
    let kind: String = r.get(1)?;
    let seasons_json: Option<String> = r.get(6)?;
    let status: String = r.get(7)?;
    let episodes_json: Option<String> = r.get(14)?;
    Ok(MediaRequest {
        id: r.get(0)?,
        kind: RequestKind::parse(&kind).unwrap_or(RequestKind::Movie),
        tmdb_id: r.get::<_, i64>(2)? as u64,
        title: r.get(3)?,
        year: r.get(4)?,
        poster_url: r.get(5)?,
        seasons: seasons_json.and_then(|j| serde_json::from_str(&j).ok()),
        episodes: episodes_json.and_then(|j| serde_json::from_str(&j).ok()),
        status: RequestStatus::parse(&status).unwrap_or(RequestStatus::Pending),
        requested_by: r.get(8)?,
        requested_by_name: r.get(9)?,
        reviewed_by: r.get(10)?,
        note: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        progress: None,
        air_status: r.get(15)?,
        next_air_date: r.get(16)?,
        last_refresh_at: r.get(17)?,
    })
}

/// A request to insert (id minted by the caller; timestamps stamped here).
pub struct NewRequest {
    pub id: String,
    pub kind: RequestKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub seasons: Option<Vec<u32>>,
    pub episodes: Option<Vec<EpisodeRef>>,
    pub status: RequestStatus,
    pub requested_by: Option<String>,
}

pub fn insert_request(pool: &Pool, req: &NewRequest, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    let seasons = req.seasons.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default());
    let episodes = req.episodes.as_ref().map(|e| serde_json::to_string(e).unwrap_or_default());
    conn.execute(
        "INSERT INTO requests (id, kind, tmdb_id, title, year, poster_url, seasons, status, requested_by, episodes, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![
            req.id,
            req.kind.as_str(),
            req.tmdb_id as i64,
            req.title,
            req.year,
            req.poster_url,
            seasons,
            req.status.as_str(),
            req.requested_by,
            episodes,
            now_ms
        ],
    )?;
    Ok(())
}

/// All requests newest-first, optionally scoped to one requester.
pub fn list_requests(conn: &Connection, only_user: Option<&str>) -> rusqlite::Result<Vec<MediaRequest>> {
    let base = format!(
        "SELECT {REQUEST_COLS} FROM requests r LEFT JOIN users u ON u.id = r.requested_by"
    );
    match only_user {
        Some(uid) => {
            let mut stmt =
                conn.prepare(&format!("{base} WHERE r.requested_by = ?1 ORDER BY r.created_at DESC"))?;
            let rows = stmt.query_map(params![uid], row_to_request)?;
            rows.collect()
        }
        None => {
            let mut stmt = conn.prepare(&format!("{base} ORDER BY r.created_at DESC"))?;
            let rows = stmt.query_map([], row_to_request)?;
            rows.collect()
        }
    }
}

pub fn get_request(conn: &Connection, id: &str) -> rusqlite::Result<Option<MediaRequest>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REQUEST_COLS} FROM requests r LEFT JOIN users u ON u.id = r.requested_by WHERE r.id = ?1"
    ))?;
    let mut rows = stmt.query_map(params![id], row_to_request)?;
    rows.next().transpose()
}

/// The open (mergeable) request for a title, if any: a second ask for the same
/// TMDB id folds into it instead of duplicating the queue. Denied/failed and
/// fully-available requests are not merge targets (a fresh ask reopens those
/// as a new row).
pub fn find_open_request(
    conn: &Connection,
    kind: RequestKind,
    tmdb_id: u64,
) -> rusqlite::Result<Option<MediaRequest>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REQUEST_COLS} FROM requests r LEFT JOIN users u ON u.id = r.requested_by \
         WHERE r.kind = ?1 AND r.tmdb_id = ?2 \
           AND r.status IN ('pending', 'approved', 'partially_available') \
         ORDER BY r.created_at DESC LIMIT 1"
    ))?;
    let mut rows = stmt.query_map(params![kind.as_str(), tmdb_id as i64], row_to_request)?;
    rows.next().transpose()
}

/// The newest request row (any status) for a title, for discover flagging.
pub fn latest_request_for(
    conn: &Connection,
    kind: RequestKind,
    tmdb_id: u64,
) -> rusqlite::Result<Option<(String, RequestStatus)>> {
    conn.query_row(
        "SELECT id, status FROM requests WHERE kind = ?1 AND tmdb_id = ?2 \
         ORDER BY created_at DESC LIMIT 1",
        params![kind.as_str(), tmdb_id as i64],
        |r| {
            let status: String = r.get(1)?;
            Ok((r.get(0)?, RequestStatus::parse(&status).unwrap_or(RequestStatus::Pending)))
        },
    )
    .optional()
}

pub fn set_request_status(
    pool: &Pool,
    id: &str,
    status: RequestStatus,
    reviewed_by: Option<&str>,
    note: Option<&str>,
    now_ms: i64,
) -> Result<bool> {
    let conn = pool.get()?;
    let n = conn.execute(
        "UPDATE requests SET status = ?2, \
         reviewed_by = COALESCE(?3, reviewed_by), note = COALESCE(?4, note), updated_at = ?5 \
         WHERE id = ?1",
        params![id, status.as_str(), reviewed_by, note, now_ms],
    )?;
    Ok(n > 0)
}

/// Replace a request's season subset (merge of a second ask; `None` = whole show).
pub fn set_request_seasons(pool: &Pool, id: &str, seasons: Option<&[u32]>, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    let json = seasons.map(|s| serde_json::to_string(s).unwrap_or_default());
    conn.execute(
        "UPDATE requests SET seasons = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, json, now_ms],
    )?;
    Ok(())
}

/// Replace a request's individual-episode subset (merge of a second ask;
/// `None` = no per-episode ask).
pub fn set_request_episodes(
    pool: &Pool,
    id: &str,
    episodes: Option<&[EpisodeRef]>,
    now_ms: i64,
) -> Result<()> {
    let conn = pool.get()?;
    let json = episodes.map(|e| serde_json::to_string(e).unwrap_or_default());
    conn.execute(
        "UPDATE requests SET episodes = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, json, now_ms],
    )?;
    Ok(())
}

/// Store the TMDB airing signals from a refresh pass + stamp `last_refresh_at`
/// (throttle key). `air_status` / `next_air_date` are set outright (not
/// COALESCE'd): an ended show clearing its `next_air_date` back to NULL is a
/// meaningful update. Does not touch `updated_at` (a background metadata sync,
/// not a user-facing lifecycle change).
pub fn set_request_air(
    pool: &Pool,
    id: &str,
    air_status: Option<&str>,
    next_air_date: Option<&str>,
    refreshed_at: i64,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE requests SET air_status = ?2, next_air_date = ?3, last_refresh_at = ?4 WHERE id = ?1",
        params![id, air_status, next_air_date, refreshed_at],
    )?;
    Ok(())
}

/// Delete a request (cascades its wanted rows). Returns false when absent.
pub fn delete_request(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    Ok(conn.execute("DELETE FROM requests WHERE id = ?1", params![id])? > 0)
}

// ----- wanted ledger ------------------------------------------------------------

/// One wanted unit: a movie, or one episode of a requested show season.
#[derive(Debug, Clone)]
pub struct WantedRow {
    pub id: String,
    pub request_id: String,
    /// `"movie"` | `"episode"`.
    pub kind: String,
    /// The movie's TMDB id, or the SHOW's TMDB id for episodes.
    pub tmdb_id: u64,
    pub imdb_id: Option<String>,
    /// Search title (movie title or show title).
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// `YYYY-MM-DD`; unaired episodes are skipped by search until the date passes.
    pub air_date: Option<String>,
    /// `"wanted"` | `"grabbed"` | `"available"`.
    pub status: String,
    pub last_search_at: Option<i64>,
}

fn row_to_wanted(r: &Row) -> rusqlite::Result<WantedRow> {
    Ok(WantedRow {
        id: r.get(0)?,
        request_id: r.get(1)?,
        kind: r.get(2)?,
        tmdb_id: r.get::<_, i64>(3)? as u64,
        imdb_id: r.get(4)?,
        title: r.get(5)?,
        year: r.get(6)?,
        season: r.get(7)?,
        episode: r.get(8)?,
        air_date: r.get(9)?,
        status: r.get(10)?,
        last_search_at: r.get(11)?,
    })
}

const WANTED_COLS: &str =
    "id, request_id, kind, tmdb_id, imdb_id, title, year, season, episode, air_date, status, last_search_at";

/// Replace a request's wanted rows (approval re-materializes from scratch, so a
/// re-approve after a season merge stays consistent). One transaction.
pub fn replace_wanted(pool: &Pool, request_id: &str, rows: &[WantedRow], now_ms: i64) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM wanted WHERE request_id = ?1", params![request_id])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO wanted (id, request_id, kind, tmdb_id, imdb_id, title, year, season, episode, air_date, status, last_search_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for w in rows {
            stmt.execute(params![
                w.id,
                w.request_id,
                w.kind,
                w.tmdb_id as i64,
                w.imdb_id,
                w.title,
                w.year,
                w.season,
                w.episode,
                w.air_date,
                w.status,
                w.last_search_at,
                now_ms
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Additively insert wanted rows WITHOUT clearing the request's existing set
/// (the refresh pass: newly-aired episodes join the ledger without disturbing
/// grabbed/available rows). `INSERT OR IGNORE` so a row whose deterministic id
/// already exists is a no-op. One transaction.
pub fn insert_wanted(pool: &Pool, rows: &[WantedRow], now_ms: i64) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO wanted (id, request_id, kind, tmdb_id, imdb_id, title, year, season, episode, air_date, status, last_search_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for w in rows {
            stmt.execute(params![
                w.id,
                w.request_id,
                w.kind,
                w.tmdb_id as i64,
                w.imdb_id,
                w.title,
                w.year,
                w.season,
                w.episode,
                w.air_date,
                w.status,
                w.last_search_at,
                now_ms
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Fill in an `air_date` TMDB now knows for an existing wanted row that lacked
/// one. Only updates rows whose `air_date IS NULL` so a known date is never
/// overwritten; never changes `status`, so a grabbed/available row is untouched.
pub fn set_wanted_air_date(pool: &Pool, id: &str, air_date: &str, now_ms: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE wanted SET air_date = ?2, updated_at = ?3 WHERE id = ?1 AND air_date IS NULL",
        params![id, air_date, now_ms],
    )?;
    Ok(())
}

pub fn wanted_for_request(conn: &Connection, request_id: &str) -> rusqlite::Result<Vec<WantedRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WANTED_COLS} FROM wanted WHERE request_id = ?1 ORDER BY season, episode"
    ))?;
    let rows = stmt.query_map(params![request_id], row_to_wanted)?;
    rows.collect()
}

/// Upcoming "coming soon" calendar entries: future-dated wanted rows (a movie's
/// availability date or a show episode's air date) not yet on disk, joined with
/// their request's display fields, ascending by date. `requester` limits to one
/// user's requests (the user-facing page); `None` spans every request (a manager
/// view). Bounded by `limit`.
pub fn upcoming_calendar(
    conn: &Connection,
    today: &str,
    requester: Option<&str>,
    limit: usize,
) -> rusqlite::Result<Vec<CalendarEntry>> {
    let mut stmt = conn.prepare(
        "SELECT w.request_id, w.tmdb_id, r.kind, w.title, w.year, r.poster_url, \
                w.season, w.episode, w.air_date, w.status \
         FROM wanted w JOIN requests r ON r.id = w.request_id \
         WHERE w.air_date IS NOT NULL AND w.air_date > ?1 \
           AND w.status IN ('wanted', 'grabbed') \
           AND r.status NOT IN ('denied', 'failed') \
           AND (?2 IS NULL OR r.requested_by = ?2) \
         ORDER BY w.air_date ASC, r.title ASC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![today, requester, limit as i64], |r| {
        let kind: String = r.get(2)?;
        Ok(CalendarEntry {
            request_id: Some(r.get(0)?),
            tmdb_id: r.get::<_, i64>(1)? as u64,
            kind: RequestKind::parse(&kind).unwrap_or(RequestKind::Movie),
            title: r.get(3)?,
            year: r.get(4)?,
            poster_url: r.get(5)?,
            season: r.get(6)?,
            episode: r.get(7)?,
            air_date: r.get(8)?,
            status: r.get(9)?,
        })
    })?;
    rows.collect()
}

/// The "missing / wanted" list: aired-or-released wanted rows (a movie past its
/// availability date, or a show episode past its air date, plus undated rows)
/// that are still `wanted` (not grabbed / available), joined with their request's
/// display fields. This is the inverse of [`upcoming_calendar`] (past-due instead
/// of future). `requester` limits to one user's requests; `None` spans all.
/// Sorted by title then season/episode. Bounded by `limit`.
pub fn missing_items(
    conn: &Connection,
    today: &str,
    requester: Option<&str>,
    limit: usize,
) -> rusqlite::Result<Vec<CalendarEntry>> {
    let mut stmt = conn.prepare(
        "SELECT w.request_id, w.tmdb_id, r.kind, w.title, w.year, r.poster_url, \
                w.season, w.episode, w.air_date, w.status \
         FROM wanted w JOIN requests r ON r.id = w.request_id \
         WHERE w.status = 'wanted' AND (w.air_date IS NULL OR w.air_date <= ?1) \
           AND r.status NOT IN ('denied', 'failed') \
           AND (?2 IS NULL OR r.requested_by = ?2) \
         ORDER BY r.title ASC, w.season ASC, w.episode ASC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![today, requester, limit as i64], |r| {
        let kind: String = r.get(2)?;
        Ok(CalendarEntry {
            request_id: Some(r.get(0)?),
            tmdb_id: r.get::<_, i64>(1)? as u64,
            kind: RequestKind::parse(&kind).unwrap_or(RequestKind::Movie),
            title: r.get(3)?,
            year: r.get(4)?,
            poster_url: r.get(5)?,
            season: r.get(6)?,
            episode: r.get(7)?,
            air_date: r.get(8)?,
            status: r.get(9)?,
        })
    })?;
    rows.collect()
}

/// Replace the library-scan "gaps" for one show (Sonarr-style missing episodes:
/// aired TMDB episodes not on disk), computed by the `library.missing` job. One
/// transaction: clear the show's rows then insert the current set (empty = the
/// show is complete, so its rows are simply cleared). `rows` = (season, episode,
/// air_date). Title + poster are denormalized for the missing view.
pub fn replace_show_gaps(
    pool: &Pool,
    show_id: &str,
    tmdb_id: u64,
    title: &str,
    poster_url: Option<&str>,
    rows: &[(u32, u32, Option<String>)],
    now_ms: i64,
) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM library_gaps WHERE show_id = ?1", params![show_id])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO library_gaps (show_id, tmdb_id, title, poster_url, season, episode, air_date, detected_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (season, episode, air_date) in rows {
            stmt.execute(params![
                show_id,
                tmdb_id as i64,
                title,
                poster_url,
                season,
                episode,
                air_date,
                now_ms
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// The library-scan "missing" rows (aired episodes of library shows not on disk),
/// as [`CalendarEntry`] with `request_id = None` (they are not requests yet). The
/// missing view unions these with [`missing_items`]; the client turns a
/// no-request row into a request when the user asks to watch it. Excludes shows
/// that already have an open request for the same tmdb id (that request's ledger
/// already tracks them, avoiding a duplicate line). Sorted by title.
pub fn library_gaps_list(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<CalendarEntry>> {
    let mut stmt = conn.prepare(
        "SELECT g.tmdb_id, g.title, g.poster_url, g.season, g.episode, g.air_date \
         FROM library_gaps g \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM requests r \
             WHERE r.tmdb_id = g.tmdb_id AND r.kind = 'show' \
               AND r.status NOT IN ('denied', 'failed') \
         ) \
         ORDER BY g.title ASC, g.season ASC, g.episode ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(CalendarEntry {
            request_id: None,
            tmdb_id: r.get::<_, i64>(0)? as u64,
            kind: RequestKind::Show,
            title: r.get(1)?,
            year: None,
            poster_url: r.get(2)?,
            season: r.get(3)?,
            episode: r.get(4)?,
            air_date: r.get(5)?,
            status: "missing".into(),
        })
    })?;
    rows.collect()
}

/// Wanted rows ready for an automatic search pass: still wanted, aired (or
/// undated), least-recently-searched first, capped.
pub fn wanted_searchable(conn: &Connection, today: &str, limit: usize) -> rusqlite::Result<Vec<WantedRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WANTED_COLS} FROM wanted \
         WHERE status = 'wanted' AND (air_date IS NULL OR air_date <= ?1) \
         ORDER BY last_search_at IS NOT NULL, last_search_at, season, episode LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![today, limit as i64], row_to_wanted)?;
    rows.collect()
}

/// Chunked `UPDATE wanted SET {set_sql} WHERE id IN (...)` over `ids`. `lead`
/// are the SET-clause params (`?1`..) that precede the id placeholders.
fn update_wanted_chunked(
    pool: &Pool,
    ids: &[String],
    set_sql: &str,
    lead: &[&dyn rusqlite::ToSql],
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let conn = pool.get()?;
    for chunk in ids.chunks(IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = lead.to_vec();
        for id in chunk {
            params_vec.push(id);
        }
        conn.execute(
            &format!("UPDATE wanted SET {set_sql} WHERE id IN ({ph})"),
            params_vec.as_slice(),
        )?;
    }
    Ok(())
}

pub fn set_wanted_status(pool: &Pool, ids: &[String], status: &str, now_ms: i64) -> Result<()> {
    update_wanted_chunked(pool, ids, "status = ?1, updated_at = ?2", &[&status, &now_ms])
}

pub fn stamp_wanted_searched(pool: &Pool, ids: &[String], now_ms: i64) -> Result<()> {
    update_wanted_chunked(pool, ids, "last_search_at = ?1, updated_at = ?1", &[&now_ms])
}

// ----- availability lookups (metadata_core.tmdb_id, indexed) ---------------------

/// The library movie item carrying this TMDB id, if any. `video` items count:
/// enrichment resolves both against TMDB's movie namespace. Seeks the real
/// `metadata_core.tmdb_id` column, joined back to `items` so a stale core row for
/// a since-deleted item never matches.
pub fn movie_item_by_tmdb(conn: &Connection, tmdb_id: u64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT c.subject_id FROM metadata_core c JOIN items i ON i.id = c.subject_id \
         WHERE c.subject_kind = 'item' AND c.tmdb_id = ?1 AND i.kind IN ('movie', 'video') LIMIT 1",
        params![tmdb_id as i64],
        |r| r.get(0),
    )
    .optional()
}

/// The library show carrying this TMDB id, if any.
pub fn show_by_tmdb(conn: &Connection, tmdb_id: u64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT c.subject_id FROM metadata_core c JOIN shows s ON s.id = c.subject_id \
         WHERE c.subject_kind = 'show' AND c.tmdb_id = ?1 LIMIT 1",
        params![tmdb_id as i64],
        |r| r.get(0),
    )
    .optional()
}

/// Every (season, episode) pair present on disk for a show.
pub fn episodes_present(conn: &Connection, show_id: &str) -> rusqlite::Result<Vec<(u32, u32)>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT season, episode FROM items \
         WHERE show_id = ?1 AND season IS NOT NULL AND episode IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![show_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn pool() -> Pool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-req-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::init(&path).unwrap()
    }

    fn seed_library(conn: &Connection) {
        conn.execute(
            "INSERT INTO libraries (id, name, kind, path, added_at) VALUES ('lib1','Films','movies','/x','now')",
            [],
        )
        .unwrap();
    }

    fn insert_movie_item(conn: &Connection, id: &str, tmdb: u64) {
        conn.execute(
            "INSERT INTO items (id, kind, title, container, library, added_at) \
             VALUES (?1, 'movie', 'T', 'mkv', 'lib1', 'now')",
            params![id],
        )
        .unwrap();
        // Availability now seeks metadata_core.tmdb_id (a real indexed column).
        conn.execute(
            "INSERT INTO metadata_core (subject_kind, subject_id, tmdb_id, updated_at) \
             VALUES ('item', ?1, ?2, 0)",
            params![id, tmdb as i64],
        )
        .unwrap();
    }

    fn new_req(id: &str, kind: RequestKind, tmdb: u64, seasons: Option<Vec<u32>>) -> NewRequest {
        NewRequest {
            id: id.into(),
            kind,
            tmdb_id: tmdb,
            title: "T".into(),
            year: Some(2020),
            poster_url: None,
            seasons,
            episodes: None,
            status: RequestStatus::Pending,
            requested_by: None,
        }
    }

    #[test]
    fn availability_lookup_matches_metadata_tmdb_id() {
        let p = pool();
        let conn = p.get().unwrap();
        seed_library(&conn);
        insert_movie_item(&conn, "m1", 603);
        assert_eq!(movie_item_by_tmdb(&conn, 603).unwrap().as_deref(), Some("m1"));
        assert_eq!(movie_item_by_tmdb(&conn, 604).unwrap(), None);
        // Items without a metadata_core row never match (no tmdb_id to seek).
        conn.execute(
            "INSERT INTO items (id, kind, title, container, library, added_at) \
             VALUES ('m2','movie','U','mkv','lib1','now')",
            [],
        )
        .unwrap();
        assert_eq!(movie_item_by_tmdb(&conn, 0).unwrap(), None);
    }

    #[test]
    fn request_roundtrip_merge_and_cascade() {
        let p = pool();
        insert_request(&p, &new_req("r1", RequestKind::Show, 1396, Some(vec![1])), 1000).unwrap();

        let conn = p.get().unwrap();
        let open = find_open_request(&conn, RequestKind::Show, 1396).unwrap().unwrap();
        assert_eq!(open.id, "r1");
        assert_eq!(open.seasons.as_deref(), Some(&[1u32][..]));
        assert_eq!(open.status, RequestStatus::Pending);
        drop(conn);

        // Widen the season subset (the duplicate-merge path).
        set_request_seasons(&p, "r1", Some(&[1, 2]), 2000).unwrap();
        let conn = p.get().unwrap();
        assert_eq!(
            get_request(&conn, "r1").unwrap().unwrap().seasons.as_deref(),
            Some(&[1u32, 2][..])
        );
        drop(conn);

        // Individual episodes persist and read back alongside the seasons.
        set_request_episodes(&p, "r1", Some(&[EpisodeRef { season: 3, episode: 5 }]), 2100).unwrap();
        let conn = p.get().unwrap();
        assert_eq!(
            get_request(&conn, "r1").unwrap().unwrap().episodes.as_deref(),
            Some(&[EpisodeRef { season: 3, episode: 5 }][..])
        );
        drop(conn);

        // Denied requests are not merge targets.
        set_request_status(&p, "r1", RequestStatus::Denied, Some("boss"), Some("non"), 3000).unwrap();
        let conn = p.get().unwrap();
        assert!(find_open_request(&conn, RequestKind::Show, 1396).unwrap().is_none());
        let denied = get_request(&conn, "r1").unwrap().unwrap();
        assert_eq!(denied.status, RequestStatus::Denied);
        assert_eq!(denied.note.as_deref(), Some("non"));
        drop(conn);

        // Wanted rows ride the request row's lifetime (FK cascade).
        let rows = vec![WantedRow {
            id: "w1".into(),
            request_id: "r1".into(),
            kind: "episode".into(),
            tmdb_id: 1396,
            imdb_id: None,
            title: "T".into(),
            year: None,
            season: Some(1),
            episode: Some(1),
            air_date: Some("2020-01-01".into()),
            status: "wanted".into(),
            last_search_at: None,
        }];
        replace_wanted(&p, "r1", &rows, 4000).unwrap();
        let conn = p.get().unwrap();
        assert_eq!(wanted_for_request(&conn, "r1").unwrap().len(), 1);
        drop(conn);
        assert!(delete_request(&p, "r1").unwrap());
        let conn = p.get().unwrap();
        assert!(wanted_for_request(&conn, "r1").unwrap().is_empty());
    }

    #[test]
    fn set_request_air_roundtrips_and_last_refresh_is_internal() {
        let p = pool();
        insert_request(&p, &new_req("r1", RequestKind::Show, 1396, None), 1000).unwrap();
        set_request_air(&p, "r1", Some("Returning Series"), Some("2026-01-17"), 5000).unwrap();
        let conn = p.get().unwrap();
        let req = get_request(&conn, "r1").unwrap().unwrap();
        assert_eq!(req.air_status.as_deref(), Some("Returning Series"));
        assert_eq!(req.next_air_date.as_deref(), Some("2026-01-17"));
        assert_eq!(req.last_refresh_at, Some(5000));
        // updated_at is NOT bumped by a background refresh.
        assert_eq!(req.updated_at, 1000);
        drop(conn);
        // Ended shows clear next_air_date back to NULL (set outright, not COALESCE).
        set_request_air(&p, "r1", Some("Ended"), None, 6000).unwrap();
        let conn = p.get().unwrap();
        let req = get_request(&conn, "r1").unwrap().unwrap();
        assert_eq!(req.air_status.as_deref(), Some("Ended"));
        assert_eq!(req.next_air_date, None);
        // Wire stays clean: last_refresh_at is #[serde(skip)].
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("lastRefreshAt").is_none());
        assert_eq!(json.get("airStatus").and_then(|v| v.as_str()), Some("Ended"));
    }

    #[test]
    fn insert_wanted_is_additive_and_never_disturbs_grabbed() {
        let p = pool();
        insert_request(&p, &new_req("r1", RequestKind::Show, 1396, None), 1000).unwrap();
        let mk = |id: &str, episode: u32, air: Option<&str>, status: &str| WantedRow {
            id: id.into(),
            request_id: "r1".into(),
            kind: "episode".into(),
            tmdb_id: 1396,
            imdb_id: None,
            title: "T".into(),
            year: None,
            season: Some(1),
            episode: Some(episode),
            air_date: air.map(str::to_string),
            status: status.into(),
            last_search_at: None,
        };
        // Seed one grabbed (dated) + one wanted (undated) row.
        replace_wanted(&p, "r1", &[mk("w-e1", 1, Some("2020-01-01"), "grabbed"), mk("w-e2", 2, None, "wanted")], 1000)
            .unwrap();
        // Refresh: a brand-new aired episode + a would-be duplicate of e1.
        insert_wanted(&p, &[mk("w-e3", 3, Some("2020-01-03"), "wanted"), mk("w-e1", 1, Some("2020-01-01"), "wanted")], 2000)
            .unwrap();
        // Fill the missing air_date on the existing wanted row e2.
        set_wanted_air_date(&p, "w-e2", "2020-01-02", 2000).unwrap();
        // The grabbed row must not be overwritten by set_wanted_air_date.
        set_wanted_air_date(&p, "w-e1", "2999-01-01", 2000).unwrap();

        let conn = p.get().unwrap();
        let rows = wanted_for_request(&conn, "r1").unwrap();
        assert_eq!(rows.len(), 3, "e3 added; the e1 duplicate was ignored");
        let by_ep = |ep: u32| rows.iter().find(|w| w.episode == Some(ep)).unwrap();
        // e1 stayed grabbed with its original date (INSERT OR IGNORE + air-date guard).
        assert_eq!(by_ep(1).status, "grabbed");
        assert_eq!(by_ep(1).air_date.as_deref(), Some("2020-01-01"));
        // e2 gained the newly-known air date.
        assert_eq!(by_ep(2).air_date.as_deref(), Some("2020-01-02"));
        // e3 is the freshly-added row.
        assert_eq!(by_ep(3).status, "wanted");
    }

    #[test]
    fn wanted_searchable_gates_on_air_date_and_status() {
        let p = pool();
        insert_request(&p, &new_req("r1", RequestKind::Show, 1396, None), 1000).unwrap();
        let mk = |id: &str, episode: u32, air: Option<&str>, status: &str, searched: Option<i64>| WantedRow {
            id: id.into(),
            request_id: "r1".into(),
            kind: "episode".into(),
            tmdb_id: 1396,
            imdb_id: None,
            title: "T".into(),
            year: None,
            season: Some(1),
            episode: Some(episode),
            air_date: air.map(str::to_string),
            status: status.into(),
            last_search_at: searched,
        };
        let rows = vec![
            mk("w-aired", 1, Some("2020-01-01"), "wanted", Some(500)),
            mk("w-unaired", 2, Some("2999-01-01"), "wanted", None),
            mk("w-grabbed", 3, Some("2020-01-01"), "grabbed", None),
            mk("w-undated", 4, None, "wanted", None),
        ];
        replace_wanted(&p, "r1", &rows, 1000).unwrap();

        let conn = p.get().unwrap();
        let due = wanted_searchable(&conn, "2026-07-05", 10).unwrap();
        let ids: Vec<&str> = due.iter().map(|w| w.id.as_str()).collect();
        // Unaired + already-grabbed excluded; never-searched sorts first.
        assert_eq!(ids, vec!["w-undated", "w-aired"]);
        drop(conn);

        set_wanted_status(&p, &["w-aired".to_string()], "available", 2000).unwrap();
        stamp_wanted_searched(&p, &["w-undated".to_string()], 3000).unwrap();
        let conn = p.get().unwrap();
        let due = wanted_searchable(&conn, "2026-07-05", 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "w-undated");
        assert_eq!(due[0].last_search_at, Some(3000));
    }

    #[test]
    fn wanted_searchable_gates_unreleased_movies() {
        // Phase 4: a movie's wanted row carries its release date as air_date
        // (season/episode NULL). An unreleased movie is monitored (excluded from
        // search) until its date passes; a released / dateless one is searchable.
        let p = pool();
        insert_request(&p, &new_req("m1", RequestKind::Movie, 603, None), 1000).unwrap();
        let mk = |id: &str, air: Option<&str>| WantedRow {
            id: id.into(),
            request_id: "m1".into(),
            kind: "movie".into(),
            tmdb_id: 603,
            imdb_id: None,
            title: "M".into(),
            year: None,
            season: None,
            episode: None,
            air_date: air.map(str::to_string),
            status: "wanted".into(),
            last_search_at: None,
        };
        // One request can only hold its own rows; use three requests so each
        // movie row is independent.
        insert_request(&p, &new_req("m2", RequestKind::Movie, 604, None), 1000).unwrap();
        insert_request(&p, &new_req("m3", RequestKind::Movie, 605, None), 1000).unwrap();
        replace_wanted(&p, "m1", &[mk("m-future", Some("2999-01-01"))], 1000).unwrap();
        replace_wanted(&p, "m2", &[WantedRow { request_id: "m2".into(), ..mk("m-out", Some("2020-01-01")) }], 1000).unwrap();
        replace_wanted(&p, "m3", &[WantedRow { request_id: "m3".into(), ..mk("m-nodate", None) }], 1000).unwrap();

        let conn = p.get().unwrap();
        let due = wanted_searchable(&conn, "2026-07-05", 10).unwrap();
        let mut ids: Vec<&str> = due.iter().map(|w| w.id.as_str()).collect();
        ids.sort_unstable();
        // The unreleased movie is held back; the out / undated ones are searchable.
        assert_eq!(ids, vec!["m-nodate", "m-out"]);
    }

    #[test]
    fn missing_items_lists_aired_open_rows_only() {
        // The missing list = aired/released rows still `wanted`. Unaired (future),
        // grabbed and available rows are excluded; undated aired rows are included.
        let p = pool();
        insert_request(&p, &new_req("r1", RequestKind::Show, 1396, None), 1000).unwrap();
        let mk = |id: &str, episode: u32, air: Option<&str>, status: &str| WantedRow {
            id: id.into(),
            request_id: "r1".into(),
            kind: "episode".into(),
            tmdb_id: 1396,
            imdb_id: None,
            title: "T".into(),
            year: None,
            season: Some(1),
            episode: Some(episode),
            air_date: air.map(str::to_string),
            status: status.into(),
            last_search_at: None,
        };
        replace_wanted(
            &p,
            "r1",
            &[
                mk("w-aired", 1, Some("2020-01-01"), "wanted"),
                mk("w-future", 2, Some("2999-01-01"), "wanted"),
                mk("w-grabbed", 3, Some("2020-01-01"), "grabbed"),
                mk("w-available", 4, Some("2020-01-01"), "available"),
                mk("w-undated", 5, None, "wanted"),
            ],
            1000,
        )
        .unwrap();

        let conn = p.get().unwrap();
        let missing = missing_items(&conn, "2026-07-05", None, 50).unwrap();
        let mut eps: Vec<u32> = missing.iter().filter_map(|e| e.episode).collect();
        eps.sort_unstable();
        // Only the aired-and-still-wanted rows (ep 1 aired, ep 5 undated).
        assert_eq!(eps, vec![1, 5]);
        // A different requester's scope excludes them.
        assert!(missing_items(&conn, "2026-07-05", Some("someone-else"), 50).unwrap().is_empty());
    }
}
