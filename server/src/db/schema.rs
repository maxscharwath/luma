//! SQLite DDL: connection pragmas, the table/index schema, the canonical column
//! lists for item/file SELECTs, and the `init`/`migrate` that apply them. Moved
//! out of [`super`] (the directory root) verbatim to keep that file focused on
//! the connection pool and the shared row-mappers.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::{Pool, PoolInner};

pub(crate) const PRAGMAS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
    PRAGMA temp_store = MEMORY;
    PRAGMA busy_timeout = 5000;
    PRAGMA mmap_size = 268435456;
    PRAGMA cache_size = -16000;
";

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS libraries (
        id        TEXT PRIMARY KEY,
        name      TEXT NOT NULL,
        kind      TEXT NOT NULL,
        path      TEXT NOT NULL,
        added_at  TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS shows (
        id        TEXT PRIMARY KEY,
        library   TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
        title     TEXT NOT NULL,
        year      INTEGER,
        metadata  TEXT,
        added_at  TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS items (
        id            TEXT PRIMARY KEY,
        kind          TEXT NOT NULL,
        title         TEXT NOT NULL,
        year          INTEGER,
        duration_ms   INTEGER,
        container     TEXT NOT NULL,
        v_codec       TEXT,
        v_width       INTEGER,
        v_height      INTEGER,
        v_hdr         INTEGER,
        v_bit_depth   INTEGER,
        a_codec       TEXT,
        a_channels    INTEGER,
        a_language    TEXT,
        subtitles     TEXT NOT NULL DEFAULT '[]',
        library       TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
        show_id       TEXT REFERENCES shows(id) ON DELETE CASCADE,
        show_title    TEXT,
        season        INTEGER,
        episode       INTEGER,
        episode_end   INTEGER,
        episode_title TEXT,
        rel_path      TEXT,
        abs_path      TEXT,
        metadata      TEXT,
        added_at      TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS files (
        id          TEXT PRIMARY KEY,
        item_id     TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        abs_path    TEXT NOT NULL UNIQUE,
        rel_path    TEXT,
        container   TEXT NOT NULL DEFAULT '',
        size        INTEGER,
        mtime       INTEGER,
        edition     TEXT,
        duration_ms INTEGER,
        v_codec     TEXT,
        v_width     INTEGER,
        v_height    INTEGER,
        v_hdr       INTEGER,
        v_bit_depth INTEGER,
        a_codec     TEXT,
        a_channels  INTEGER,
        a_language  TEXT,
        audio_tracks TEXT NOT NULL DEFAULT '[]',
        subtitles   TEXT NOT NULL DEFAULT '[]',
        probed      INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_items_library ON items(library);
    CREATE INDEX IF NOT EXISTS idx_items_kind    ON items(kind);
    CREATE INDEX IF NOT EXISTS idx_items_show    ON items(show_id, season, episode);
    CREATE INDEX IF NOT EXISTS idx_shows_library ON shows(library);
    CREATE INDEX IF NOT EXISTS idx_files_item    ON files(item_id);
    CREATE INDEX IF NOT EXISTS idx_files_abs     ON files(abs_path);
    CREATE INDEX IF NOT EXISTS idx_files_probed  ON files(probed);

    CREATE TABLE IF NOT EXISTS users (
        id            TEXT PRIMARY KEY,
        email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
        username      TEXT NOT NULL,
        password_hash TEXT NOT NULL,
        avatar_url    TEXT,
        permissions   TEXT NOT NULL DEFAULT '[\"playback\"]',
        created_at    TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS sessions (
        token      TEXT PRIMARY KEY,
        user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at TEXT NOT NULL,
        expires_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS progress (
        user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        item_id     TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        position_ms INTEGER NOT NULL,
        duration_ms INTEGER,
        updated_at  TEXT NOT NULL,
        PRIMARY KEY (user_id, item_id)
    );
    CREATE TABLE IF NOT EXISTS invites (
        token       TEXT PRIMARY KEY,
        permissions TEXT NOT NULL DEFAULT '[\"playback\"]',
        created_by  TEXT REFERENCES users(id) ON DELETE SET NULL,
        created_at  TEXT NOT NULL,
        expires_at  INTEGER NOT NULL,
        used_at     TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
    CREATE INDEX IF NOT EXISTS idx_progress_user ON progress(user_id, updated_at DESC);

    CREATE TABLE IF NOT EXISTS settings (
        key        TEXT PRIMARY KEY,
        value      TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS play_history (
        id         TEXT PRIMARY KEY,
        user_id    TEXT,
        username   TEXT,
        item_id    TEXT,
        kind       TEXT NOT NULL,
        title      TEXT NOT NULL,
        library    TEXT,
        started_at INTEGER NOT NULL,
        ended_at   INTEGER NOT NULL,
        watched_ms INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_history_user  ON play_history(user_id, ended_at DESC);
    CREATE INDEX IF NOT EXISTS idx_history_ended ON play_history(ended_at DESC);

    CREATE TABLE IF NOT EXISTS item_vectors (
        id         TEXT PRIMARY KEY,
        dim        INTEGER NOT NULL,
        vec        BLOB NOT NULL,
        updated_at TEXT NOT NULL
    );
";

/// Explicit column list for file SELECTs — keeps [`super::row_to_file`] index-stable.
pub(crate) const FILE_COLS: &str = "id,rel_path,container,size,edition,probed,\
    duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,\
    a_codec,a_channels,a_language,subtitles,abs_path,audio_tracks";

/// Explicit column list for item SELECTs — keeps [`super::row_to_item`] index-stable.
/// `metadata` is appended last (index 25).
pub(crate) const ITEM_COLS: &str = "id,kind,title,year,duration_ms,container,\
    v_codec,v_width,v_height,v_hdr,v_bit_depth,a_codec,a_channels,a_language,subtitles,\
    library,show_id,show_title,season,episode,episode_end,episode_title,rel_path,abs_path,added_at,metadata";

/// Open (creating if needed) the database and ensure schema + pragmas.
pub fn init(path: &Path) -> Result<Pool> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let pool = Arc::new(PoolInner {
        path: path.to_path_buf(),
        idle: Mutex::new(Vec::new()),
        max_idle: 8,
    });

    let conn = pool.get()?;
    conn.execute_batch(SCHEMA).context("failed to apply schema")?;
    migrate(&conn);
    Ok(pool)
}

/// Idempotent column additions for databases created before a column existed.
/// `ALTER TABLE … ADD COLUMN` errors with "duplicate column name" once the
/// column is present, which we ignore.
fn migrate(conn: &Connection) {
    for sql in [
        "ALTER TABLE items ADD COLUMN metadata TEXT",
        "ALTER TABLE shows ADD COLUMN metadata TEXT",
        // Per-user permissions for accounts created before they existed.
        "ALTER TABLE users ADD COLUMN permissions TEXT NOT NULL DEFAULT '[\"playback\"]'",
        // Full per-file audio-track list (was a single representative track).
        "ALTER TABLE files ADD COLUMN audio_tracks TEXT NOT NULL DEFAULT '[]'",
        // Last-seen timestamp for the admin "Membres & partage" activity column.
        "ALTER TABLE users ADD COLUMN last_seen TEXT",
        // Per-account preferred UI locale ("fr" | "en"), synced across devices.
        "ALTER TABLE users ADD COLUMN language TEXT",
        // Optional profile-lock PIN (PBKDF2 hash, own salt). NULL = no PIN.
        "ALTER TABLE users ADD COLUMN pin_hash TEXT",
    ] {
        let _ = conn.execute(sql, []);
    }
}
