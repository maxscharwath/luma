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

    -- Segment markers per episode (skip-intro + next-up at credits). One row per
    -- (item, kind); kind is 'intro' | 'credits' | …; bounds in ms. Populated from
    -- embedded chapters and the audio-fingerprint job.
    CREATE TABLE IF NOT EXISTS markers (
        item_id    TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        kind       TEXT NOT NULL,
        start_ms   INTEGER NOT NULL,
        end_ms     INTEGER NOT NULL,
        source     TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (item_id, kind)
    );

    -- Subtitles fetched from an online provider (OpenSubtitles, …), converted to
    -- WebVTT and cached under <data>/subs/downloaded/. Merged into the item's
    -- subtitle list so they appear in the player alongside embedded tracks.
    CREATE TABLE IF NOT EXISTS downloaded_subtitles (
        id         TEXT PRIMARY KEY,
        item_id    TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        language   TEXT,
        label      TEXT NOT NULL,
        provider   TEXT NOT NULL,
        path       TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

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
    -- `item_id` is a catalogue id: a movie item id OR a show id (shows live in
    -- their own table, so this column is intentionally NOT an items FK a show
    -- can be marked watched as a whole).
    CREATE TABLE IF NOT EXISTS watched (
        user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        item_id    TEXT NOT NULL,
        watched_at TEXT NOT NULL,
        PRIMARY KEY (user_id, item_id)
    );
    -- Per-season TMDB cast (the show's seasons are derived from items, so this
    -- holds the season-level credits keyed by (show, season number)).
    CREATE TABLE IF NOT EXISTS season_meta (
        show_id TEXT NOT NULL REFERENCES shows(id) ON DELETE CASCADE,
        season  INTEGER NOT NULL,
        casts   TEXT NOT NULL,
        PRIMARY KEY (show_id, season)
    );
    -- Ma liste: user-bookmarked titles (movie item ids OR show ids; same
    -- no-items-FK rationale as `watched`). Synced across web + TV.
    CREATE TABLE IF NOT EXISTS my_list (
        user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        item_id  TEXT NOT NULL,
        added_at TEXT NOT NULL,
        PRIMARY KEY (user_id, item_id)
    );
    CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
    CREATE INDEX IF NOT EXISTS idx_progress_user ON progress(user_id, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_watched_user  ON watched(user_id);
    CREATE INDEX IF NOT EXISTS idx_my_list_user  ON my_list(user_id, added_at DESC);

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

    -- Background job system (see services::jobs). Per-job schedule overrides,
    -- one row per execution, and per-run log lines.
    CREATE TABLE IF NOT EXISTS job_schedules (
        key        TEXT PRIMARY KEY,
        schedule   TEXT,
        enabled    INTEGER NOT NULL DEFAULT 1,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS job_runs (
        id             TEXT PRIMARY KEY,
        job_key        TEXT NOT NULL,
        trigger_kind   TEXT NOT NULL,
        status         TEXT NOT NULL,
        started_at     INTEGER NOT NULL,
        finished_at    INTEGER,
        progress_done  INTEGER,
        progress_total INTEGER,
        error          TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_job_runs_key ON job_runs(job_key, started_at DESC);
    CREATE TABLE IF NOT EXISTS job_logs (
        run_id  TEXT NOT NULL,
        ts      INTEGER NOT NULL,
        level   TEXT NOT NULL,
        message TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_job_logs_run ON job_logs(run_id, ts);

    -- Per-user LLM-generated taste: a natural-language profile that evolves each
    -- run, plus the cached personalized home sections (JSON). See the
    -- `sections.personalize` job + services::sections.
    CREATE TABLE IF NOT EXISTS user_taste (
        user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        profile    TEXT,
        sections   TEXT NOT NULL DEFAULT '[]',
        updated_at INTEGER NOT NULL
    );

    -- Global, editorial LLM/director-curated collections (same for everyone):
    -- 'Spielberg', 'Best Horror', 'Top IMDb', …. Regenerated by the
    -- `sections.curate` job; member ids resolved at write time. `source` is
    -- 'director' (deterministic, grouped by crew) or 'llm'.
    CREATE TABLE IF NOT EXISTS curated_sections (
        key        TEXT PRIMARY KEY,
        rank       INTEGER NOT NULL DEFAULT 0,
        source     TEXT NOT NULL DEFAULT 'llm',
        item_ids   TEXT NOT NULL DEFAULT '[]',
        title_fr   TEXT,
        title_en   TEXT,
        reason_fr  TEXT,
        reason_en  TEXT,
        updated_at INTEGER NOT NULL
    );

    -- Per-movie/show AI suggestions ('Suggestions IA' on the detail page), one
    -- row per seed item. Lazily generated by the LLM connector on first view and
    -- cached here (member ids resolved at write time); empty item_ids = 'tried,
    -- nothing usable' (terminal, so the client stops polling).
    CREATE TABLE IF NOT EXISTS item_suggestions (
        item_id    TEXT PRIMARY KEY,
        item_ids   TEXT NOT NULL DEFAULT '[]',
        reason_fr  TEXT,
        reason_en  TEXT,
        updated_at INTEGER NOT NULL
    );

    -- Keyframe-derived HLS segment table per physical file (see infra::hls).
    -- Computed lazily on the first HLS request and revalidated by mtime/size/
    -- version; `segments` is a JSON array of [start_us, end_us] keyframe-aligned
    -- ranges, `v_codec` the cached RFC6381 video codec string for the master.
    CREATE TABLE IF NOT EXISTS file_segments (
        file_id     TEXT PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
        mtime       INTEGER,
        size        INTEGER,
        version     INTEGER NOT NULL,
        duration_us INTEGER NOT NULL,
        v_codec     TEXT,
        segments    TEXT NOT NULL,
        updated_at  INTEGER NOT NULL
    );
";

/// Explicit column list for file SELECTs keeps [`super::row_to_file`] index-stable.
pub(crate) const FILE_COLS: &str = "id,rel_path,container,size,edition,probed,\
    duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,\
    a_codec,a_channels,a_language,subtitles,abs_path,audio_tracks";

/// Explicit column list for item SELECTs keeps [`super::row_to_item`] index-stable.
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
        // `season_meta` shipped briefly with a `cast` column a reserved SQLite
        // keyword that breaks unquoted SELECT/INSERT. Rename to `casts`. Errors
        // ("no such column") once renamed / on fresh DBs, which we ignore.
        "ALTER TABLE season_meta RENAME COLUMN \"cast\" TO casts",
        // Keyframe-derived HLS segment tables (infra::hls). `CREATE TABLE IF NOT
        // EXISTS` is idempotent for DBs created before the table existed.
        "CREATE TABLE IF NOT EXISTS file_segments (\
            file_id TEXT PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,\
            mtime INTEGER, size INTEGER, version INTEGER NOT NULL,\
            duration_us INTEGER NOT NULL, v_codec TEXT,\
            segments TEXT NOT NULL, updated_at INTEGER NOT NULL)",
    ] {
        let _ = conn.execute(sql, []);
    }
}
