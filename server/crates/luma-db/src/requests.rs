//! Requests + wanted-ledger persistence, and the tmdbId availability lookups
//! (which ride the `idx_items_tmdb` / `idx_shows_tmdb` expression indexes the
//! json_extract expressions here must stay byte-identical to the DDL).

use rusqlite::OptionalExtension;

use super::*;
use luma_domain::{EpisodeRef, MediaRequest, RequestKind, RequestStatus};

/// Columns of the request list SELECT (requester username joined in). `r.episodes`
/// trails the original set so existing positional indices never shift.
const REQUEST_COLS: &str = "r.id, r.kind, r.tmdb_id, r.title, r.year, r.poster_url, r.seasons, \
    r.status, r.requested_by, u.username, r.reviewed_by, r.note, r.created_at, r.updated_at, \
    r.episodes";

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

pub fn wanted_for_request(conn: &Connection, request_id: &str) -> rusqlite::Result<Vec<WantedRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WANTED_COLS} FROM wanted WHERE request_id = ?1 ORDER BY season, episode"
    ))?;
    let rows = stmt.query_map(params![request_id], row_to_wanted)?;
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
        let path = std::env::temp_dir().join(format!("luma-req-{}-{n}.db", std::process::id()));
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
}
