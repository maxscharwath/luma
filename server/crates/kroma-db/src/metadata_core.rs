//! Language-invariant resolved metadata (`metadata_core`): one row per catalog
//! subject (a movie/video item OR a show). This is the identity / art / people
//! layer that never changes with the UI language the per-language text lives in
//! [`super::translations`]. Promoting `tmdb_id` to a real column here also makes
//! availability matching a plain indexed seek instead of a `json_extract` scan.
//!
//! Not yet wired into the read/write paths (built additively ahead of the
//! cutover); remove the `dead_code` allowance once enrichment + serving move over.
#![allow(dead_code)]

use std::collections::HashMap;

use super::*;

use kroma_domain::{CastMember, CrewMember};

/// Subject-kind discriminants for `metadata_core` / `translations` rows.
pub const ITEM: &str = "item";
pub const SHOW: &str = "show";

/// The invariant half of a title's metadata. `cast` / `crew` are the people
/// (names + photos); the localized character names live per-language in
/// [`super::translations::TransData::characters`], aligned to `cast` by index.
#[derive(Debug, Clone, Default)]
pub struct MetaCore {
    pub tmdb_id: Option<u64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<u64>,
    pub release_date: Option<String>,
    pub rating: Option<f32>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub logo_url: Option<String>,
    pub cast: Vec<CastMember>,
    pub crew: Vec<CrewMember>,
}

/// Column list for core SELECTs keeps [`row_to_core`] index-stable.
const CORE_COLS: &str =
    "tmdb_id,imdb_id,tvdb_id,release_date,rating,poster_url,backdrop_url,logo_url,cast_json,crew_json";

/// Upsert one subject's invariant core. Character names are stripped from `cast`
/// before storing they are language-variant and belong in `translations`.
pub fn set_core(pool: &Pool, kind: &str, id: &str, core: &MetaCore) -> Result<()> {
    let conn = pool.get()?;
    write_core(&conn, kind, id, core)
}

/// Connection-level upsert (shares one tx/connection with a caller doing a batch).
pub(crate) fn write_core(conn: &Connection, kind: &str, id: &str, core: &MetaCore) -> Result<()> {
    // Store people without their (localized) character it lives in translations.
    let cast: Vec<CastMember> = core
        .cast
        .iter()
        .cloned()
        .map(|c| CastMember { character: None, ..c })
        .collect();
    let cast_json = serde_json::to_string(&cast).unwrap_or_else(|_| "[]".into());
    let crew_json = serde_json::to_string(&core.crew).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO metadata_core \
            (subject_kind,subject_id,tmdb_id,imdb_id,tvdb_id,release_date,rating,\
             poster_url,backdrop_url,logo_url,cast_json,crew_json,updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) \
         ON CONFLICT(subject_kind,subject_id) DO UPDATE SET \
             tmdb_id=excluded.tmdb_id, imdb_id=excluded.imdb_id, tvdb_id=excluded.tvdb_id, \
             release_date=excluded.release_date, rating=excluded.rating, \
             poster_url=excluded.poster_url, backdrop_url=excluded.backdrop_url, \
             logo_url=excluded.logo_url, cast_json=excluded.cast_json, \
             crew_json=excluded.crew_json, updated_at=excluded.updated_at",
        params![
            kind,
            id,
            core.tmdb_id.map(|v| v as i64),
            core.imdb_id,
            core.tvdb_id.map(|v| v as i64),
            core.release_date,
            core.rating,
            core.poster_url,
            core.backdrop_url,
            core.logo_url,
            cast_json,
            crew_json,
            kroma_primitives::now_ms(),
        ],
    )?;
    Ok(())
}

/// One subject's invariant core, or `None` if not yet enriched.
pub fn get_core(pool: &Pool, kind: &str, id: &str) -> Result<Option<MetaCore>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {CORE_COLS} FROM metadata_core WHERE subject_kind=?1 AND subject_id=?2"
    ))?;
    let mut rows = stmt.query_map(params![kind, id], row_to_core)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// Invariant cores for a batch of ids (one query per id-chunk), keyed by id.
pub fn get_cores(conn: &Connection, kind: &str, ids: &[&str]) -> Result<HashMap<String, MetaCore>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT subject_id,{CORE_COLS} FROM metadata_core \
             WHERE subject_kind=? AND subject_id IN ({ph})"
        ))?;
        let params_iter = std::iter::once(kind).chain(chunk.iter().copied());
        let rows = stmt.query_map(rusqlite::params_from_iter(params_iter), |r| {
            // subject_id is col 0; core cols shift by one.
            Ok((r.get::<_, String>(0)?, row_to_core_offset(r, 1)?))
        })?;
        for row in rows {
            let (id, core) = row?;
            out.insert(id, core);
        }
    }
    Ok(out)
}

/// Row mapper for a `SELECT CORE_COLS` (cols 0..=9).
fn row_to_core(r: &Row) -> rusqlite::Result<MetaCore> {
    row_to_core_offset(r, 0)
}

/// Row mapper starting at column `base` (so callers can prepend `subject_id`).
fn row_to_core_offset(r: &Row, base: usize) -> rusqlite::Result<MetaCore> {
    let cast_json: String = r.get(base + 8)?;
    let crew_json: String = r.get(base + 9)?;
    Ok(MetaCore {
        tmdb_id: r.get::<_, Option<i64>>(base)?.map(|v| v as u64),
        imdb_id: r.get(base + 1)?,
        tvdb_id: r.get::<_, Option<i64>>(base + 2)?.map(|v| v as u64),
        release_date: r.get(base + 3)?,
        rating: r.get(base + 4)?,
        poster_url: r.get(base + 5)?,
        backdrop_url: r.get(base + 6)?,
        logo_url: r.get(base + 7)?,
        cast: serde_json::from_str(&cast_json).unwrap_or_default(),
        crew: serde_json::from_str(&crew_json).unwrap_or_default(),
    })
}

/// Delete one subject's core (paired with a translations wipe on re-language /
/// reprocess). Errors are propagated; a missing row is a no-op.
pub fn delete_core(conn: &Connection, kind: &str, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM metadata_core WHERE subject_kind=?1 AND subject_id=?2",
        params![kind, id],
    )?;
    Ok(())
}
