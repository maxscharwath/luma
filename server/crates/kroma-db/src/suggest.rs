//! Persistence for per-item **AI suggestions** ("Suggestions IA" on the detail
//! page): one cached row per seed item, generated lazily by the LLM connector
//! (`crate::services::llm::suggest_for`) and served as a plain hydrate. Empty
//! `item_ids` is a terminal "tried, nothing usable" marker (so the client stops
//! polling instead of re-triggering generation forever).

use std::collections::HashMap;

use super::translations::{self, TransData};
use super::*;

use rusqlite::OptionalExtension;

/// A cached AI-suggestion row: resolved member ids + a localized one-line reason
/// per language (`locale -> reason`, from the `translations` cache).
#[derive(Debug, Clone, Default)]
pub struct SuggestionRow {
    pub item_ids: Vec<String>,
    pub reasons: HashMap<String, String>,
}

/// The cached suggestion for one seed item, if generated. `Some` with empty
/// `item_ids` means generation ran but found nothing a terminal state.
pub fn get_suggestion(pool: &Pool, item_id: &str) -> Result<Option<SuggestionRow>> {
    let conn = pool.get()?;
    let base = conn
        .query_row(
            "SELECT item_ids FROM item_suggestions WHERE item_id = ?1",
            params![item_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    let Some(ids_json) = base else { return Ok(None) };
    let mut row = SuggestionRow {
        item_ids: serde_json::from_str(&ids_json).unwrap_or_default(),
        reasons: HashMap::new(),
    };
    for by_lang in translations::load_all(pool, "suggestion", &[item_id])?.into_values() {
        for (lang, data) in by_lang {
            if let Some(reason) = data.reason {
                row.reasons.insert(lang, reason);
            }
        }
    }
    Ok(Some(row))
}

/// Upsert the cached suggestion for one seed item: the base row (ids) plus one
/// localized reason per language in the `translations` cache. `reasons` is
/// `locale -> reason` (empty for the terminal "nothing found" marker).
pub fn set_suggestion(pool: &Pool, item_id: &str, ids: &[String], reasons: &HashMap<String, String>) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    tx.execute(
        "INSERT INTO item_suggestions (item_id, item_ids, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(item_id) DO UPDATE SET item_ids=excluded.item_ids, updated_at=excluded.updated_at",
        params![item_id, ids_json, kroma_primitives::now_ms()],
    )?;
    // Replace this seed's reasons wholesale (a fresh generation supersedes them).
    tx.execute(
        "DELETE FROM translations WHERE subject_kind='suggestion' AND subject_id=?1",
        params![item_id],
    )?;
    for (lang, reason) in reasons {
        if reason.trim().is_empty() {
            continue;
        }
        let data = TransData { reason: Some(reason.clone()), ..Default::default() };
        translations::write(&tx, "suggestion", item_id, lang, translations::LLM, &data)?;
    }
    tx.commit()?;
    Ok(())
}
