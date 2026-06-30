//! Persistence for per-item **AI suggestions** ("Suggestions IA" on the detail
//! page): one cached row per seed item, generated lazily by the LLM connector
//! ([`crate::services::llm::suggest_for`]) and served as a plain hydrate. Empty
//! `item_ids` is a terminal "tried, nothing usable" marker (so the client stops
//! polling instead of re-triggering generation forever).

use super::*;

use rusqlite::OptionalExtension;

/// A cached AI-suggestion row: resolved member ids + a bilingual one-line reason.
#[derive(Debug, Clone)]
pub struct SuggestionRow {
    pub item_ids: Vec<String>,
    pub reason_fr: Option<String>,
    pub reason_en: Option<String>,
}

/// The cached suggestion for one seed item, if generated. `Some` with empty
/// `item_ids` means generation ran but found nothing a terminal state.
pub fn get_suggestion(pool: &Pool, item_id: &str) -> Result<Option<SuggestionRow>> {
    let conn = pool.get()?;
    let row = conn
        .query_row(
            "SELECT item_ids, reason_fr, reason_en FROM item_suggestions WHERE item_id = ?1",
            params![item_id],
            |r| {
                let ids_json: String = r.get(0)?;
                Ok(SuggestionRow {
                    item_ids: serde_json::from_str(&ids_json).unwrap_or_default(),
                    reason_fr: r.get(1)?,
                    reason_en: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Upsert the cached suggestion for one seed item (replace-on-conflict).
pub fn set_suggestion(pool: &Pool, item_id: &str, ids: &[String], reason_fr: Option<&str>, reason_en: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO item_suggestions (item_id, item_ids, reason_fr, reason_en, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(item_id) DO UPDATE SET \
            item_ids=excluded.item_ids, reason_fr=excluded.reason_fr, \
            reason_en=excluded.reason_en, updated_at=excluded.updated_at",
        params![item_id, ids_json, reason_fr, reason_en, crate::services::jobs::now_ms()],
    )?;
    Ok(())
}
