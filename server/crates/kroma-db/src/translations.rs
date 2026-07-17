//! The generic per-language translation cache (`translations`).
//!
//! ONE table for every localized string in the app: TMDB text (`source='tmdb'`)
//! and LLM-generated section / suggestion titles + reasons (`source='llm'`).
//! Adding a language is inserting rows never a schema change. Reads resolve with
//! a fallback chain: requested lang -> `en` -> any available.
//!
//! `subject_kind` is `'item'|'show'|'episode'|'season_cast'|'curated'|'suggestion'`;
//! only the [`TransData`] fields relevant to that kind are populated (the rest
//! serialize away). Reads are point / range seeks on the PK `(kind,id,lang)`.
//!
//! Not yet wired into the read/write paths (built additively ahead of the
//! cutover); remove the `dead_code` allowance once serving + generation move over.
#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::*;

/// Provenance tags for the `source` column (cache invalidation + backfill).
pub const TMDB: &str = "tmdb";
pub const LLM: &str = "llm";
pub const MANUAL: &str = "manual";

/// The variant payload stored per `(subject, lang)`. Every field is optional /
/// skip-if-empty so a row only carries what its `subject_kind` needs (a catalog
/// row uses title/overview/…, a curated/suggestion row uses title/reason).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    /// Localized character names, aligned by index to the subject's core `cast`
    /// (or the season cast for `season_cast`). `None` = character unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<Option<String>>,
    /// One-line reason (curated collections + AI suggestions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TransData {
    /// Whether this payload carries anything worth persisting.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.tagline.is_none()
            && self.overview.is_none()
            && self.genres.is_empty()
            && self.characters.is_empty()
            && self.reason.is_none()
    }
}

/// Upsert one `(subject, lang)` translation.
pub fn put(pool: &Pool, kind: &str, id: &str, lang: &str, source: &str, data: &TransData) -> Result<()> {
    let conn = pool.get()?;
    write(&conn, kind, id, lang, source, data)
}

/// Connection-level upsert (shares one tx/connection with a caller doing a batch,
/// e.g. writing every supported language for one title).
pub(crate) fn write(
    conn: &Connection,
    kind: &str,
    id: &str,
    lang: &str,
    source: &str,
    data: &TransData,
) -> Result<()> {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".into());
    conn.execute(
        "INSERT INTO translations (subject_kind,subject_id,lang,source,data,updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6) \
         ON CONFLICT(subject_kind,subject_id,lang) DO UPDATE SET \
             source=excluded.source, data=excluded.data, updated_at=excluded.updated_at",
        params![kind, id, lang, source, json, kroma_primitives::now_ms()],
    )?;
    Ok(())
}

/// Resolve one subject's translation in `lang` (fallback requested -> en -> any).
/// `None` only when the subject has no translations at all.
pub fn resolve_one(pool: &Pool, kind: &str, id: &str, lang: &str) -> Result<Option<TransData>> {
    let conn = pool.get()?;
    let mut raw = load(&conn, kind, &[id])?;
    Ok(raw.remove(id).and_then(|by_lang| pick(by_lang, lang)))
}

/// Resolve a batch of ids in `lang` (fallback requested -> en -> any), keyed by
/// id. Ids with no translation at all are absent from the map. One indexed query
/// per id-chunk the hot home / listing path.
pub fn resolve_many(
    conn: &Connection,
    kind: &str,
    ids: &[&str],
    lang: &str,
) -> Result<HashMap<String, TransData>> {
    let raw = load(conn, kind, ids)?;
    Ok(raw
        .into_iter()
        .filter_map(|(id, by_lang)| pick(by_lang, lang).map(|d| (id, d)))
        .collect())
}

/// Every stored translation for a subject kind, grouped by id (all languages).
/// The search indexer uses this to index title/overview/genres in every language
/// so a query matches whatever language the user typed.
pub fn all_for_kind(pool: &Pool, kind: &str) -> Result<HashMap<String, Vec<TransData>>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT subject_id, data FROM translations WHERE subject_kind = ?1")?;
    let rows = stmt.query_map(params![kind], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out: HashMap<String, Vec<TransData>> = HashMap::new();
    for row in rows {
        let (id, json) = row?;
        if let Ok(data) = serde_json::from_str::<TransData>(&json) {
            out.entry(id).or_default().push(data);
        }
    }
    Ok(out)
}

/// Which languages a subject already has stored (for enrichment gap-filling).
pub fn languages_for(pool: &Pool, kind: &str, id: &str) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT lang FROM translations WHERE subject_kind=?1 AND subject_id=?2")?;
    let rows = stmt.query_map(params![kind, id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Load every stored language for a set of ids as `id -> (lang -> data)` (no
/// fallback the caller picks per locale, e.g. curated rows carrying all langs).
pub fn load_all(pool: &Pool, kind: &str, ids: &[&str]) -> Result<HashMap<String, HashMap<String, TransData>>> {
    let conn = pool.get()?;
    load(&conn, kind, ids)
}

/// Delete every language row for one subject (re-language / reprocess).
pub fn delete_all(conn: &Connection, kind: &str, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM translations WHERE subject_kind=?1 AND subject_id=?2",
        params![kind, id],
    )?;
    Ok(())
}

/// Pick the best available payload: requested lang, then `en`, then any.
fn pick(mut by_lang: HashMap<String, TransData>, lang: &str) -> Option<TransData> {
    by_lang
        .remove(lang)
        .or_else(|| by_lang.remove("en"))
        .or_else(|| by_lang.into_values().next())
}

/// Load all stored languages for a set of ids, as `id -> (lang -> data)`.
fn load(
    conn: &Connection,
    kind: &str,
    ids: &[&str],
) -> Result<HashMap<String, HashMap<String, TransData>>> {
    let mut out: HashMap<String, HashMap<String, TransData>> = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT subject_id,lang,data FROM translations \
             WHERE subject_kind=? AND subject_id IN ({ph})"
        ))?;
        let params_iter = std::iter::once(kind).chain(chunk.iter().copied());
        let rows = stmt.query_map(rusqlite::params_from_iter(params_iter), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (id, lang, json) = row?;
            let data = serde_json::from_str::<TransData>(&json).unwrap_or_default();
            out.entry(id).or_default().insert(lang, data);
        }
    }
    Ok(out)
}
