//! Persistence for global editorial curated collections (the `sections.curate`
//! job): a replace-all set, read by the `curated` home source. Members are
//! resolved to real item/show ids at write time so serving is a plain hydrate.

use super::*;

/// One curated collection. `item_ids` are resolved member ids (movies or shows);
/// `source` is `"director"` (deterministic) or `"llm"` (editorial).
#[derive(Debug, Clone)]
pub struct CuratedRow {
    pub key: String,
    pub rank: i64,
    pub source: String,
    pub item_ids: Vec<String>,
    pub title_fr: Option<String>,
    pub title_en: Option<String>,
    pub reason_fr: Option<String>,
    pub reason_en: Option<String>,
}

/// All curated collections, lowest `rank` first.
pub fn get_curated(pool: &Pool) -> Result<Vec<CuratedRow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT key, rank, source, item_ids, title_fr, title_en, reason_fr, reason_en \
         FROM curated_sections ORDER BY rank ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        let ids_json: String = r.get(3)?;
        Ok(CuratedRow {
            key: r.get(0)?,
            rank: r.get(1)?,
            source: r.get(2)?,
            item_ids: serde_json::from_str(&ids_json).unwrap_or_default(),
            title_fr: r.get(4)?,
            title_en: r.get(5)?,
            reason_fr: r.get(6)?,
            reason_en: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Replace the entire curated set in one transaction (the job regenerates it).
pub fn set_curated(pool: &Pool, rows: &[CuratedRow]) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM curated_sections", [])?;
    let now = crate::services::jobs::now_ms();
    // Skip duplicate keys: the director + LLM producers can independently emit the
    // same slug (e.g. two spellings of a director's name normalize alike). `key`
    // is the PRIMARY KEY, so a plain INSERT of a dup would abort the whole
    // transaction and wipe out every curated row keep the first, drop the rest.
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        if !seen.insert(row.key.as_str()) {
            continue;
        }
        let ids = serde_json::to_string(&row.item_ids).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO curated_sections \
             (key, rank, source, item_ids, title_fr, title_en, reason_fr, reason_en, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.key, row.rank, row.source, ids,
                row.title_fr, row.title_en, row.reason_fr, row.reason_en, now
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}
