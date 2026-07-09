//! Accounts: users, registration invites and sessions.

use super::*;

use rusqlite::OptionalExtension;

use luma_domain::{Invite, PublicUser};

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
    // note: pre-existing token primitive used at the db layer to salt the id.
    let id = luma_primitives::short_hash(&format!("user|{email}|{}", luma_primitives::random_token()));
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
        audio_language: None,
        subtitle_language: None,
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
    // invite the loser's UPDATE matches no row and yields `None`. (The pool
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
        "SELECT id,email,username,avatar_url,created_at,permissions,language,(pin_hash IS NOT NULL),audio_language,subtitle_language,password_hash FROM users WHERE email = ?1",
    )?;
    let mut rows = stmt.query_map(params![email], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(10)?))
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
        "SELECT id,email,username,avatar_url,created_at,permissions,language,(pin_hash IS NOT NULL),audio_language,subtitle_language,password_hash FROM users \
         WHERE email = ?1 COLLATE NOCASE OR username = ?1 LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![identifier], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(10)?))
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// Fetch a full account by id (e.g. to mint tokens after a passkey assertion
/// resolves which user signed in). `None` if unknown.
pub fn user_by_id(pool: &Pool, id: &str) -> Result<Option<User>> {
    let conn = pool.get()?;
    let user = conn
        .query_row(
            "SELECT id,email,username,avatar_url,created_at,permissions,language,(pin_hash IS NOT NULL),audio_language,subtitle_language FROM users WHERE id = ?1",
            params![id],
            row_to_user,
        )
        .optional()?;
    Ok(user)
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

/// Every non-null avatar URL across all users. Uploaded avatars live in the same
/// `images` dir as the regenerable art cache, so the cache-cleanup job uses this
/// to avoid trimming them (they can't be re-downloaded).
pub fn avatar_urls(pool: &Pool) -> Result<Vec<String>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT avatar_url FROM users WHERE avatar_url IS NOT NULL")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
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

/// Set (or clear, with `None`) a user's preferred playback audio language.
pub fn set_user_audio_language(pool: &Pool, user_id: &str, language: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET audio_language = ?2 WHERE id = ?1",
        params![user_id, language],
    )?;
    Ok(())
}

/// Set (or clear, with `None`) a user's preferred playback subtitle language
/// (the sentinel `"off"` is a stored value meaning "force subtitles off").
pub fn set_user_subtitle_language(pool: &Pool, user_id: &str, language: Option<&str>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET subtitle_language = ?2 WHERE id = ?1",
        params![user_id, language],
    )?;
    Ok(())
}

/// Whether `username` is already taken by *another* account, checking BOTH the
/// username column (case-sensitive, as username login resolves) AND the email
/// column (case-insensitive). Rejecting a username that equals someone's email
/// closes the ambiguity in `find_user_by_login` (`email = ?1 OR username = ?1`),
/// where an attacker could otherwise register/rename to a victim's email and
/// shadow their email login. `exclude_id` skips the caller's own row.
pub fn username_taken(pool: &Pool, username: &str, exclude_id: Option<&str>) -> Result<bool> {
    let conn = pool.get()?;
    let taken: i64 = match exclude_id {
        Some(id) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE (username = ?1 OR email = ?1 COLLATE NOCASE) AND id <> ?2)",
            params![username, id],
            |r| r.get(0),
        )?,
        None => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = ?1 OR email = ?1 COLLATE NOCASE)",
            params![username],
            |r| r.get(0),
        )?,
    };
    Ok(taken != 0)
}

/// Change a user's email. The caller must pre-check for a duplicate to surface a
/// clean 409; the `UNIQUE COLLATE NOCASE` constraint is the atomic backstop
/// (a `rusqlite` error here is that collision).
pub fn set_user_email(pool: &Pool, user_id: &str, email: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET email = ?2 WHERE id = ?1",
        params![user_id, email],
    )?;
    Ok(())
}

/// The stored password hash for a user (for verifying the *current* password on
/// a self-service change). `None` if the user id is unknown.
pub fn user_password_hash(pool: &Pool, user_id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let hash = conn
        .query_row("SELECT password_hash FROM users WHERE id = ?1", params![user_id], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    Ok(hash)
}

/// Replace a user's password hash (self-service change; the caller verifies the
/// current password first).
pub fn set_user_password(pool: &Pool, user_id: &str, password_hash: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET password_hash = ?2 WHERE id = ?1",
        params![user_id, password_hash],
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
/// comparison). `access_token` records the device credential this session was
/// minted from, so the account's session list can flag the current device.
pub fn create_session(
    pool: &Pool,
    token: &str,
    user_id: &str,
    expires_at: i64,
    access_token: Option<&str>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO sessions (token,user_id,created_at,expires_at,access_token) VALUES (?1,?2,?3,?4,?5)",
        params![token, user_id, now_or_blank(), expires_at, access_token],
    )?;
    Ok(())
}

/// The non-secret id (`short_hash`) of the device credential a live session was
/// minted from. Lets the sessions endpoint flag the caller's current device
/// without exposing (or re-hashing) the raw token in the handler. `None` when
/// the session predates parent-token tracking.
pub fn session_device_id(pool: &Pool, token: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let access = conn
        .query_row(
            "SELECT access_token FROM sessions WHERE token = ?1",
            params![token],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(access.map(|t| luma_primitives::short_hash(&t)))
}

/// Resolve a session token to its (non-expired) user.
pub fn session_user(pool: &Pool, token: &str) -> Result<Option<User>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut stmt = conn.prepare(
        "SELECT u.id,u.email,u.username,u.avatar_url,u.created_at,u.permissions,u.language,(u.pin_hash IS NOT NULL),u.audio_language,u.subtitle_language \
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

// ----- access tokens (long-lived device credential) ---------------------------

/// Persist a new access token. `pin_verified` is true when it was minted through
/// a strong check (password login / correct PIN) so the exchange can skip the
/// PIN on subsequent silent refreshes.
pub fn create_access_token(
    pool: &Pool,
    token: &str,
    user_id: &str,
    expires_at: i64,
    pin_verified: bool,
    user_agent: Option<&str>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO access_tokens (token,user_id,created_at,expires_at,pin_verified,last_seen,user_agent) \
         VALUES (?1,?2,?3,?4,?5,?3,?6)",
        params![token, user_id, now_or_blank(), expires_at, pin_verified as i64, user_agent],
    )?;
    Ok(())
}

/// One device credential in the account's session list.
pub struct AccessTokenRow {
    /// A stable, non-secret id for the token (a short hash) safe to expose to the
    /// client and to revoke by, without leaking the token itself.
    pub id: String,
    /// The device's captured User-Agent (may be empty/unknown).
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_seen: Option<String>,
}

/// List a user's live (non-expired) device credentials, newest first, each with
/// a non-secret id (`short_hash(token)`) for display + revocation.
pub fn list_access_tokens(pool: &Pool, user_id: &str) -> Result<Vec<AccessTokenRow>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut stmt = conn.prepare(
        "SELECT token,created_at,last_seen,user_agent FROM access_tokens \
         WHERE user_id = ?1 AND expires_at > ?2 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![user_id, now], |r| {
        let token: String = r.get(0)?;
        Ok(AccessTokenRow {
            id: luma_primitives::short_hash(&token),
            created_at: r.get(1)?,
            last_seen: r.get(2)?,
            user_agent: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Revoke one of a user's device credentials by its non-secret id
/// (`short_hash(token)`), also deleting any live sessions minted from it so the
/// device is signed out immediately. Returns whether a matching token was found.
/// Scoped to `user_id` so a caller can only revoke their own devices.
pub fn delete_access_token_by_id(pool: &Pool, user_id: &str, id: &str) -> Result<bool> {
    let conn = pool.get()?;
    // Tokens are opaque and only reversible by hashing, so find the owner's token
    // whose short-hash matches, then delete it + its sessions.
    let mut stmt =
        conn.prepare("SELECT token FROM access_tokens WHERE user_id = ?1")?;
    let tokens = stmt
        .query_map(params![user_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let Some(token) = tokens.into_iter().find(|t| luma_primitives::short_hash(t) == id) else {
        return Ok(false);
    };
    conn.execute("DELETE FROM sessions WHERE access_token = ?1", params![token])?;
    conn.execute("DELETE FROM access_tokens WHERE token = ?1", params![token])?;
    Ok(true)
}

/// Resolve a (non-expired) access token to its user plus the stored
/// `pin_verified` flag. `None` when unknown/expired.
pub fn access_token_user(pool: &Pool, token: &str) -> Result<Option<(User, bool)>> {
    let conn = pool.get()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut stmt = conn.prepare(
        "SELECT u.id,u.email,u.username,u.avatar_url,u.created_at,u.permissions,u.language,(u.pin_hash IS NOT NULL),u.audio_language,u.subtitle_language,a.pin_verified \
         FROM access_tokens a JOIN users u ON u.id = a.user_id \
         WHERE a.token = ?1 AND a.expires_at > ?2",
    )?;
    let mut rows = stmt.query_map(params![token, now], |r| {
        Ok((row_to_user(r)?, r.get::<_, i64>(10)? != 0))
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// Mark an access token PIN-verified (after a correct PIN on exchange), so later
/// silent refreshes for a PIN-locked account skip the prompt.
pub fn set_access_pin_verified(pool: &Pool, token: &str, verified: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE access_tokens SET pin_verified = ?2 WHERE token = ?1",
        params![token, verified as i64],
    )?;
    Ok(())
}

/// Re-lock every access token for a user (clear `pin_verified`). Called when the
/// PIN is set/rotated/cleared so all devices must re-confirm the new state.
pub fn reset_access_pin_verified(pool: &Pool, user_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE access_tokens SET pin_verified = 0 WHERE user_id = ?1",
        params![user_id],
    )?;
    Ok(())
}

/// Delete an access token (device logout / disconnect). No-op if unknown.
pub fn delete_access_token(pool: &Pool, token: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM access_tokens WHERE token = ?1", params![token])?;
    Ok(())
}
