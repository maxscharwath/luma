//! Persistence for per-user LLM taste: the evolving natural-language profile and
//! the cached personalized home sections (see the `sections.personalize` job).

use super::*;

/// One user's stored taste: `(profile, sections_json)`. `None` if never generated.
pub fn get_user_taste(pool: &Pool, user_id: &str) -> Result<Option<(Option<String>, String)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT profile, sections FROM user_taste WHERE user_id = ?1")?;
    let mut rows = stmt.query_map(params![user_id], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// Upsert a user's profile + personalized sections (`sections` is a JSON array).
pub fn set_user_taste(pool: &Pool, user_id: &str, profile: Option<&str>, sections: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO user_taste (user_id, profile, sections, updated_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(user_id) DO UPDATE SET \
            profile=excluded.profile, sections=excluded.sections, updated_at=excluded.updated_at",
        params![user_id, profile, sections, kroma_primitives::now_ms()],
    )?;
    Ok(())
}

/// Every account as `(id, language)`, so the personalize job can iterate users
/// and prompt the LLM in each one's locale.
pub fn all_users_with_lang(pool: &Pool) -> Result<Vec<(String, Option<String>)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT id, language FROM users")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
