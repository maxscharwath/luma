//! Accounts: users, registration invites and sessions.

use super::*;

use rusqlite::OptionalExtension;

use crate::model::{Invite, PublicUser};

/// Create a user with an already-hashed password. The id is random (not derived
/// from the email) so it isn't guessable. Returns the created [`User`]; the
/// caller should pre-check the email to surface a clean 409 (the `UNIQUE`
/// constraint is the hard guard).
pub fn create_user(
    pool: &Pool,
    email: &str,
    username: &str,
    password_hash: &str,
    permissions: &[Permission],
) -> Result<User> {
    let conn = pool.get()?;
    let permissions = permissions.to_vec();
    let perms_json = serde_json::to_string(&permissions).unwrap_or_else(|_| "[\"playback\"]".into());
    let id = crate::scan::short_hash(&format!("user|{email}|{}", crate::auth::random_token()));
    let created_at = now_or_blank();
    conn.execute(
        "INSERT INTO users (id,email,username,password_hash,avatar_url,permissions,created_at) \
         VALUES (?1,?2,?3,?4,NULL,?5,?6)",
        params![id, email, username, password_hash, perms_json, created_at],
    )?;
    Ok(User {
        id,
        email: email.to_string(),
        username: username.to_string(),
        avatar_url: None,
        language: None,
        permissions,
        created_at,
        has_pin: false,
    })
}

/// Total number of accounts (used to detect the bootstrap owner registration).
pub fn user_count(pool: &Pool) -> Result<i64> {
    let conn = pool.get()?;
    Ok(conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
}

// ----- invitations ------------------------------------------------------------

fn row_to_invite(r: &Row) -> rusqlite::Result<Invite> {
    let used_at: Option<String> = r.get(5)?;
    Ok(Invite {
        token: r.get(0)?,
        permissions: parse_permissions(&r.get::<_, String>(1)?),
        created_by: r.get(2)?,
        created_at: r.get(3)?,
        expires_at: r.get(4)?,
        used: used_at.is_some(),
    })
}

/// Create a registration invite granting `permissions`, expiring at `expires_at`.
pub fn create_invite(
    pool: &Pool,
    token: &str,
    permissions: &[Permission],
    created_by: &str,
    expires_at: i64,
) -> Result<()> {
    let conn = pool.get()?;
    let perms_json = serde_json::to_string(permissions).unwrap_or_else(|_| "[\"playback\"]".into());
    conn.execute(
        "INSERT INTO invites (token,permissions,created_by,created_at,expires_at,used_at) \
         VALUES (?1,?2,?3,?4,?5,NULL)",
        params![token, perms_json, created_by, now_or_blank(), expires_at],
    )?;
    Ok(())
}

/// Fetch one invite by token (regardless of state).
pub fn get_invite(pool: &Pool, token: &str) -> Result<Option<Invite>> {
    let conn = pool.get()?;
    let inv = conn
        .query_row(
            "SELECT token,permissions,created_by,created_at,expires_at,used_at FROM invites WHERE token = ?1",
            params![token],
            row_to_invite,
        )
        .optional()?;
    Ok(inv)
}

/// Atomically consume a valid (unused, unexpired) invite → its granted
/// permissions. Returns `None` if the token is unknown / used / expired.
pub fn consume_invite(pool: &Pool, token: &str) -> Result<Option<Vec<Permission>>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    // Atomic check-and-consume: the `used_at IS NULL` guard lives in the same
    // statement that stamps `used_at`, and `RETURNING` hands back the granted
    // permissions only if this call is the one that flipped the row. Two
    // concurrent invite-only registrations therefore can't both win a single-use
    // invite — the loser's UPDATE matches no row and yields `None`. (The pool
    // hands each caller its own WAL connection, so the prior SELECT-then-UPDATE
    // had a real TOCTOU window.)
    let perms: Option<String> = conn
        .query_row(
            "UPDATE invites SET used_at = ?2 \
             WHERE token = ?1 AND used_at IS NULL AND expires_at > ?3 \
             RETURNING permissions",
            params![token, now_or_blank(), now],
            |r| r.get(0),
        )
        .optional()?;
    Ok(perms.map(|json| parse_permissions(&json)))
}

/// Pending invites (unused, unexpired), newest first.
pub fn list_invites(pool: &Pool) -> Result<Vec<Invite>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut stmt = conn.prepare(
        "SELECT token,permissions,created_by,created_at,expires_at,used_at FROM invites \
         WHERE used_at IS NULL AND expires_at > ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![now], row_to_invite)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Revoke (delete) an invite. No-op if unknown.
pub fn delete_invite(pool: &Pool, token: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM invites WHERE token = ?1", params![token])?;
    Ok(())
}

/// Look up a user by email (case-insensitive), returning the user plus its
/// stored password hash for verification. `None` if no such email.
pub fn find_user_by_email(pool: &Pool, email: &str) -> Result<Option<(User, String)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,email,username,avatar_url,created_at,permissions,language,(pin_hash IS NOT NULL),password_hash FROM users WHERE email = ?1",
    )?;
    let mut rows = stmt.query_map(params![email], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(8)?))
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// Look up a user by an identifier that may be either their email
/// (case-insensitive) or their username, returning the user plus its stored
/// password hash. Lets the profile picker (which only knows usernames) log in.
pub fn find_user_by_login(pool: &Pool, identifier: &str) -> Result<Option<(User, String)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,email,username,avatar_url,created_at,permissions,language,(pin_hash IS NOT NULL),password_hash FROM users \
         WHERE email = ?1 COLLATE NOCASE OR username = ?1 LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![identifier], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(8)?))
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// All users as the public (no-email) shape, for the profile picker.
pub fn list_users(pool: &Pool) -> Result<Vec<PublicUser>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,username,avatar_url,(pin_hash IS NOT NULL) FROM users ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PublicUser {
            id: r.get(0)?,
            username: r.get(1)?,
            avatar_url: r.get(2)?,
            has_pin: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Set (or clear) a user's avatar URL.
pub fn set_user_avatar(pool: &Pool, user_id: &str, avatar_url: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET avatar_url = ?2 WHERE id = ?1",
        params![user_id, avatar_url],
    )?;
    Ok(())
}

/// Set (or clear, with `None`) a user's preferred UI locale.
pub fn set_user_language(pool: &Pool, user_id: &str, language: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET language = ?2 WHERE id = ?1",
        params![user_id, language],
    )?;
    Ok(())
}

/// The stored PBKDF2 PIN hash for a user, or `None` when no PIN is set. Used by
/// `/api/auth/pin/verify` and the set/clear handlers to compare the supplied PIN.
pub fn user_pin_hash(pool: &Pool, user_id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let hash = conn
        .query_row("SELECT pin_hash FROM users WHERE id = ?1", params![user_id], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten();
    Ok(hash)
}

/// Set (or clear, with `None`) a user's PIN hash.
pub fn set_user_pin(pool: &Pool, user_id: &str, pin_hash: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET pin_hash = ?2 WHERE id = ?1",
        params![user_id, pin_hash],
    )?;
    Ok(())
}

/// Persist a new session token (expiry as a unix-seconds integer for robust
/// comparison).
pub fn create_session(pool: &Pool, token: &str, user_id: &str, expires_at: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO sessions (token,user_id,created_at,expires_at) VALUES (?1,?2,?3,?4)",
        params![token, user_id, now_or_blank(), expires_at],
    )?;
    Ok(())
}

/// Resolve a session token to its (non-expired) user.
pub fn session_user(pool: &Pool, token: &str) -> Result<Option<User>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut stmt = conn.prepare(
        "SELECT u.id,u.email,u.username,u.avatar_url,u.created_at,u.permissions,u.language,(u.pin_hash IS NOT NULL) \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token = ?1 AND s.expires_at > ?2",
    )?;
    let mut rows = stmt.query_map(params![token, now], row_to_user)?;
    match rows.next() {
        Some(u) => Ok(Some(u?)),
        None => Ok(None),
    }
}

/// Delete a session (logout). No-op if the token is unknown.
pub fn delete_session(pool: &Pool, token: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
    Ok(())
}
