//! Admin console: settings store, member management, play history + analytics
//! and library/storage stats.

use super::*;

use rusqlite::OptionalExtension;

// ----- settings store ---------------------------------------------------------

/// Every persisted setting as `(key, value)` pairs (value is parsed JSON).
pub fn settings_all(pool: &Pool) -> Result<Vec<(String, serde_json::Value)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT key,value FROM settings")?;
    let rows = stmt.query_map([], |r| {
        let k: String = r.get(0)?;
        let v: String = r.get(1)?;
        Ok((k, v))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (k, raw) = row?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            out.push((k, v));
        }
    }
    Ok(out)
}

/// Upsert one setting (value stored as compact JSON).
pub fn settings_set(pool: &Pool, key: &str, value: &serde_json::Value) -> Result<()> {
    let conn = pool.get()?;
    let json = serde_json::to_string(value)?;
    conn.execute(
        "INSERT INTO settings (key,value,updated_at) VALUES (?1,?2,?3) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![key, json, now_or_blank()],
    )?;
    Ok(())
}

// ----- admin: users -----------------------------------------------------------

fn row_to_admin_user(r: &Row) -> rusqlite::Result<User> {
    // Reuse the User shape: cols 0..=5 match row_to_user, col 6 carries last_seen
    // (read as `language`, ignored by the caller, which re-reads col 6 itself),
    // col 7 is the has_pin flag, cols 8..=9 the playback-language prefs. The
    // caller's SELECT must project all ten.
    row_to_user(r)
}

/// All accounts for the admin "Membres & partage" table, oldest first (owner is
/// account 0). `online` is left false here the handler fills it from the live
/// playback registry.
pub fn admin_users(pool: &Pool) -> Result<Vec<kroma_domain::AdminUser>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,email,username,avatar_url,created_at,permissions,last_seen,(pin_hash IS NOT NULL),audio_language,subtitle_language \
         FROM users ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |r| {
        let user = row_to_admin_user(r)?;
        let last_seen: Option<String> = r.get(6)?;
        Ok((user, last_seen))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (u, last_seen) = row?;
        out.push(kroma_domain::AdminUser {
            role: kroma_domain::role_label(&u.permissions).to_string(),
            id: u.id,
            email: u.email,
            username: u.username,
            avatar_url: u.avatar_url,
            permissions: u.permissions,
            created_at: u.created_at,
            last_seen,
            online: false,
        });
    }
    Ok(out)
}

/// Fetch one full user by id (with email + permissions), or `None`.
#[allow(dead_code)] // public lookup helper; used by admin tooling/tests.
pub fn get_user(pool: &Pool, id: &str) -> Result<Option<User>> {
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

/// Replace a user's permission set.
pub fn update_user_permissions(pool: &Pool, id: &str, permissions: &[Permission]) -> Result<()> {
    let conn = pool.get()?;
    let perms_json = serde_json::to_string(permissions).unwrap_or_else(|_| "[\"playback\"]".into());
    conn.execute(
        "UPDATE users SET permissions = ?2 WHERE id = ?1",
        params![id, perms_json],
    )?;
    Ok(())
}

/// Rename a user.
pub fn set_user_username(pool: &Pool, id: &str, username: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET username = ?2 WHERE id = ?1",
        params![id, username],
    )?;
    Ok(())
}

/// Delete a user (cascades sessions + progress).
pub fn delete_user(pool: &Pool, id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM users WHERE id = ?1", params![id])?;
    Ok(())
}

/// Stamp a user's last-seen time (called on login + playback ping).
pub fn touch_last_seen(pool: &Pool, id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE users SET last_seen = ?2 WHERE id = ?1",
        params![id, now_or_blank()],
    )?;
    Ok(())
}

// ----- admin: play history + analytics ---------------------------------------

/// Append one finished playback to the history log.
#[allow(clippy::too_many_arguments)]
pub fn record_play(
    pool: &Pool,
    user_id: Option<&str>,
    username: Option<&str>,
    item_id: Option<&str>,
    kind: &str,
    title: &str,
    library: Option<&str>,
    started_at: i64,
    ended_at: i64,
    watched_ms: i64,
) -> Result<()> {
    let conn = pool.get()?;
    let id = kroma_primitives::short_hash(&format!(
        "play|{}|{}|{started_at}|{}",
        user_id.unwrap_or("?"),
        item_id.unwrap_or("?"),
        kroma_primitives::random_token()
    ));
    conn.execute(
        "INSERT INTO play_history \
         (id,user_id,username,item_id,kind,title,library,started_at,ended_at,watched_ms) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![id, user_id, username, item_id, kind, title, library, started_at, ended_at, watched_ms],
    )?;
    Ok(())
}

/// Per-user watch aggregates since `since` (unix-seconds), best watchers first.
pub fn top_users(pool: &Pool, since: i64, limit: usize) -> Result<Vec<kroma_domain::TopUser>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(username,'?') AS u, COUNT(*) AS plays, \
            SUM(watched_ms) AS total, \
            SUM(CASE WHEN kind='movie' THEN watched_ms ELSE 0 END) AS films, \
            SUM(CASE WHEN kind IN ('episode','video') THEN watched_ms ELSE 0 END) AS tv \
         FROM play_history WHERE ended_at >= ?1 \
         GROUP BY username ORDER BY total DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![since, limit as i64], |r| {
        Ok(kroma_domain::TopUser {
            username: r.get(0)?,
            plays: r.get(1)?,
            watched_ms: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            films_ms: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            tv_ms: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Raw history rows since `since` (unix-seconds) for client/server-side bucketing.
pub fn history_since(pool: &Pool, since: i64) -> Result<Vec<kroma_domain::HistoryRow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT ended_at,kind,watched_ms FROM play_history WHERE ended_at >= ?1 ORDER BY ended_at",
    )?;
    let rows = stmt.query_map(params![since], |r| {
        Ok(kroma_domain::HistoryRow {
            ended_at: r.get(0)?,
            kind: parse_kind(&r.get::<_, String>(1)?),
            watched_ms: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ----- admin: library + storage stats ----------------------------------------

/// Per-library item count + total bytes on disk (joins items→files).
pub fn library_stats(pool: &Pool) -> Result<Vec<kroma_domain::LibraryStat>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT i.library, COUNT(DISTINCT i.id) AS items, COALESCE(SUM(f.size),0) AS bytes \
         FROM items i LEFT JOIN files f ON f.item_id = i.id \
         GROUP BY i.library",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(kroma_domain::LibraryStat {
            id: r.get(0)?,
            item_count: r.get(1)?,
            total_bytes: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Total bytes across all indexed files (the "Utilisé" storage stat).
pub fn total_media_bytes(pool: &Pool) -> Result<i64> {
    let conn = pool.get()?;
    Ok(conn.query_row("SELECT COALESCE(SUM(size),0) FROM files", [], |r| r.get(0))?)
}

/// Counts for the cache panel: `(enriched items, enriched shows, embeddings)`
/// how many movies/videos and shows carry resolved TMDB metadata, and how many
/// title embeddings are stored.
pub fn metadata_counts(pool: &Pool) -> Result<(i64, i64, i64)> {
    let conn = pool.get()?;
    // Episodes also carry metadata but aren't "titles"; exclude them so the
    // count matches the movie/loose-video figure the panel documents.
    let items: i64 = conn.query_row(
        "SELECT COUNT(*) FROM items WHERE metadata IS NOT NULL AND kind != 'episode'",
        [],
        |r| r.get(0),
    )?;
    let shows: i64 =
        conn.query_row("SELECT COUNT(*) FROM shows WHERE metadata IS NOT NULL", [], |r| r.get(0))?;
    let vectors: i64 = conn.query_row("SELECT COUNT(*) FROM item_vectors", [], |r| r.get(0))?;
    Ok((items, shows, vectors))
}
