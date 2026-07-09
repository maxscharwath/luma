//! Passkeys: stored WebAuthn credentials for passwordless sign-in.
//!
//! Storage-only the WebAuthn ceremonies (register/authenticate) live in the
//! server binary (`api::passkeys`), which serializes the webauthn-rs `Passkey`
//! into the `credential` text column and reads it back here.

use super::*;

use rusqlite::OptionalExtension;

/// One registered authenticator in the account's passkey list.
pub struct PasskeyRow {
    /// Credential id (base64url) the authenticator's handle.
    pub id: String,
    /// Friendly label the user gave it.
    pub name: String,
    pub created_at: String,
    pub last_used: Option<String>,
}

/// Persist a freshly-registered credential → the `created_at` timestamp written
/// (so the caller can echo the full row back without a re-read).
pub fn insert_passkey(
    pool: &Pool,
    id: &str,
    user_id: &str,
    name: &str,
    credential: &str,
) -> Result<String> {
    let conn = pool.get()?;
    let created_at = now_or_blank();
    conn.execute(
        "INSERT INTO passkeys (id,user_id,name,credential,created_at,last_used) \
         VALUES (?1,?2,?3,?4,?5,NULL)",
        params![id, user_id, name, credential, created_at],
    )?;
    Ok(created_at)
}

/// The account's registered passkeys (display shape), newest first.
pub fn list_passkeys(pool: &Pool, user_id: &str) -> Result<Vec<PasskeyRow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,name,created_at,last_used FROM passkeys \
         WHERE user_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![user_id], |r| {
        Ok(PasskeyRow {
            id: r.get(0)?,
            name: r.get(1)?,
            created_at: r.get(2)?,
            last_used: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The serialized `Passkey` JSON blobs for a user needed to build the exclude
/// list on registration and the allow-list on authentication.
pub fn passkey_credentials(pool: &Pool, user_id: &str) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT credential FROM passkeys WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Distinct account ids that have at least one passkey. Usernameless
/// (discoverable) sign-in maps the assertion's user handle back to an account by
/// matching against these ids, so only accounts with passkeys are considered.
pub fn passkey_user_ids(pool: &Pool) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT DISTINCT user_id FROM passkeys")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Mark a credential used, optionally replacing the stored blob when the
/// authenticator's signature counter advanced (webauthn-rs `needs_update`).
pub fn touch_passkey(pool: &Pool, id: &str, credential: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    match credential {
        Some(cred) => conn.execute(
            "UPDATE passkeys SET last_used = ?2, credential = ?3 WHERE id = ?1",
            params![id, now_or_blank(), cred],
        )?,
        None => conn.execute(
            "UPDATE passkeys SET last_used = ?2 WHERE id = ?1",
            params![id, now_or_blank()],
        )?,
    };
    Ok(())
}

/// Remove one of a user's passkeys by id. Scoped to `user_id` so a caller can
/// only delete their own. Returns whether a row was removed.
pub fn delete_passkey(pool: &Pool, user_id: &str, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    let n = conn.execute(
        "DELETE FROM passkeys WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(n > 0)
}

/// Look up the credential id of a passkey (exists check used when finishing
/// registration to reject a duplicate). Currently unused externally but kept for
/// symmetry with the other lookups.
#[allow(dead_code)]
pub fn passkey_exists(pool: &Pool, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM passkeys WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?;
    Ok(found.is_some())
}
