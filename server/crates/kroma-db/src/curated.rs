//! Persistence for global editorial curated collections (the `sections.curate`
//! job): a replace-all set, read by the `curated` home source. Members are
//! resolved to real item/show ids at write time so serving is a plain hydrate.

use std::collections::{HashMap, HashSet};

use super::translations::{self, TransData};
use super::*;

/// One curated collection. `item_ids` are resolved member ids (movies or shows);
/// `source` is `"director"` (deterministic) or `"llm"` (editorial). The localized
/// `titles`/`reasons` (locale -> string) live in the generic `translations` cache
/// (`subject_kind='curated'`), not in per-language columns.
#[derive(Debug, Clone, Default)]
pub struct CuratedRow {
    pub key: String,
    pub rank: i64,
    pub source: String,
    pub item_ids: Vec<String>,
    pub titles: HashMap<String, String>,
    pub reasons: HashMap<String, String>,
}

/// All curated collections, lowest `rank` first, with every stored language's
/// title/reason hydrated from the translation cache.
pub fn get_curated(pool: &Pool) -> Result<Vec<CuratedRow>> {
    let mut rows: Vec<CuratedRow> = {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT key, rank, source, item_ids FROM curated_sections ORDER BY rank ASC",
        )?;
        let mapped = stmt.query_map([], |r| {
            let ids_json: String = r.get(3)?;
            Ok(CuratedRow {
                key: r.get(0)?,
                rank: r.get(1)?,
                source: r.get(2)?,
                item_ids: serde_json::from_str(&ids_json).unwrap_or_default(),
                titles: HashMap::new(),
                reasons: HashMap::new(),
            })
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    // Hydrate localized title/reason (all languages) from the translation cache.
    let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    let mut by_key = translations::load_all(pool, "curated", &keys)?;
    for row in &mut rows {
        if let Some(by_lang) = by_key.remove(&row.key) {
            for (lang, data) in by_lang {
                if let Some(t) = data.title {
                    row.titles.insert(lang.clone(), t);
                }
                if let Some(rs) = data.reason {
                    row.reasons.insert(lang, rs);
                }
            }
        }
    }
    Ok(rows)
}

/// Replace the entire curated set in one transaction (the job regenerates it):
/// the base rows in `curated_sections`, the localized title/reason per language
/// in `translations` (`subject_kind='curated'`).
pub fn set_curated(pool: &Pool, rows: &[CuratedRow]) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM curated_sections", [])?;
    tx.execute("DELETE FROM translations WHERE subject_kind = 'curated'", [])?;
    let now = kroma_primitives::now_ms();
    // Skip duplicate keys: the director + LLM producers can independently emit the
    // same slug (e.g. two spellings of a director's name normalize alike). `key`
    // is the PRIMARY KEY, so a plain INSERT of a dup would abort the whole
    // transaction and wipe out every curated row keep the first, drop the rest.
    let mut seen = HashSet::new();
    for row in rows {
        if !seen.insert(row.key.as_str()) {
            continue;
        }
        let ids = serde_json::to_string(&row.item_ids).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO curated_sections (key, rank, source, item_ids, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![row.key, row.rank, row.source, ids, now],
        )?;
        let langs: HashSet<&str> =
            row.titles.keys().chain(row.reasons.keys()).map(String::as_str).collect();
        for lang in langs {
            let data = TransData {
                title: row.titles.get(lang).cloned(),
                reason: row.reasons.get(lang).cloned(),
                ..Default::default()
            };
            translations::write(&tx, "curated", &row.key, lang, translations::LLM, &data)?;
        }
    }
    tx.commit()?;
    Ok(())
}
