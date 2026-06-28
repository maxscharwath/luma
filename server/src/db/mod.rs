//! SQLite persistence (rusqlite + r2d2 pool).
//!
//! The whole library lives in SQLite. A scan computes the full set of
//! libraries/shows/items and atomically swaps it in via [`replace_all`]. Read
//! queries run on `spawn_blocking` threads against a small connection pool.
//!
//! Performance: WAL journaling, `synchronous=NORMAL`, a 256 MiB mmap and a 16
//! MiB page cache are set on every pooled connection; reads never block the
//! single writer, and the indices below keep movie/show/episode lookups O(log n).
//!
//! This module is the directory root: it owns the connection pool, schema/init
//! and the shared row-mappers/helpers, and re-exports the per-domain query
//! submodules below as a flat namespace so `db::list_movies(...)` etc. resolve
//! unchanged.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::metadata::Metadata;
use crate::model::{
    AudioStream, Kind, MediaFile, MediaItem, Permission, SubtitleTrack, User, VideoStream,
};

mod media;
mod ingest;
mod accounts;
mod playback;
mod library;
mod admin;

pub use media::*;
pub use ingest::*;
pub use accounts::*;
pub use playback::*;
pub use library::*;
pub use admin::*;

/// A small, cheap-to-clone WAL connection pool. Cloning shares the same idle
/// connection set (it's an `Arc` inside). Read queries on separate
/// `spawn_blocking` threads each check out their own connection, so WAL readers
/// run concurrently with the single writer.
pub type Pool = Arc<PoolInner>;

pub struct PoolInner {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
    max_idle: usize,
}

impl PoolInner {
    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).context("open sqlite connection")?;
        conn.execute_batch(PRAGMAS).context("apply pragmas")?;
        Ok(conn)
    }

    /// Check out a connection (reused or freshly opened). Returned to the pool on
    /// drop, up to `max_idle`.
    pub fn get(self: &Arc<Self>) -> Result<PooledConn> {
        let reused = self.idle.lock().unwrap().pop();
        let conn = match reused {
            Some(c) => c,
            None => self.open()?,
        };
        Ok(PooledConn {
            inner: Some(conn),
            pool: Arc::clone(self),
        })
    }
}

/// RAII connection handle; derefs to [`rusqlite::Connection`].
pub struct PooledConn {
    inner: Option<Connection>,
    pool: Pool,
}

impl Deref for PooledConn {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.inner.as_ref().expect("connection present")
    }
}

impl DerefMut for PooledConn {
    fn deref_mut(&mut self) -> &mut Connection {
        self.inner.as_mut().expect("connection present")
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        if let Some(conn) = self.inner.take() {
            let mut idle = self.pool.idle.lock().unwrap();
            if idle.len() < self.pool.max_idle {
                idle.push(conn);
            }
        }
    }
}

const PRAGMAS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
    PRAGMA temp_store = MEMORY;
    PRAGMA busy_timeout = 5000;
    PRAGMA mmap_size = 268435456;
    PRAGMA cache_size = -16000;
";

const SCHEMA: &str = "
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
";

/// Explicit column list for file SELECTs — keeps [`row_to_file`] index-stable.
const FILE_COLS: &str = "id,rel_path,container,size,edition,probed,\
    duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,\
    a_codec,a_channels,a_language,subtitles,abs_path,audio_tracks";

/// Explicit column list for item SELECTs — keeps [`row_to_item`] index-stable.
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

// ----- shared row-mappers / helpers -------------------------------------------

/// Parse a stored `metadata` JSON blob into [`Metadata`]; tolerant of nulls and
/// stale shapes (returns `None`).
pub(crate) fn parse_metadata(json: Option<String>) -> Option<Metadata> {
    json.and_then(|j| serde_json::from_str::<Metadata>(&j).ok())
}

/// Map a row of
/// `id,email,username,avatar_url,created_at,permissions,language,has_pin` to a
/// [`User`]. Column 7 is a boolean (`pin_hash IS NOT NULL`) — every SELECT that
/// feeds this must project it (the password-hash lookups carry it before their
/// trailing `password_hash`). Column 6 is read as `language`; the admin members
/// query repurposes it for `last_seen` (which the caller re-reads itself).
pub(crate) fn row_to_user(r: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        email: r.get(1)?,
        username: r.get(2)?,
        avatar_url: r.get(3)?,
        created_at: r.get(4)?,
        permissions: parse_permissions(&r.get::<_, String>(5)?),
        language: r.get(6)?,
        has_pin: r.get(7)?,
    })
}

/// Parse a stored `permissions` JSON array of string keys, dropping any unknown
/// keys (tolerant forward-compat). Falls back to `[Playback]` on malformed JSON.
pub(crate) fn parse_permissions(json: &str) -> Vec<Permission> {
    match serde_json::from_str::<Vec<String>>(json) {
        Ok(keys) => keys.iter().filter_map(|k| Permission::parse(k)).collect(),
        Err(_) => vec![Permission::Playback],
    }
}

/// Build a [`MediaFile`] from a row selected with [`FILE_COLS`].
fn row_to_file(r: &Row) -> rusqlite::Result<MediaFile> {
    let probed: i64 = r.get(5)?;
    let v_codec: Option<String> = r.get(7)?;
    let video = v_codec.map(|codec| VideoStream {
        codec,
        width: r.get(8).ok().flatten(),
        height: r.get(9).ok().flatten(),
        hdr: r.get::<_, Option<i64>>(10).ok().flatten().unwrap_or(0) != 0,
        bit_depth: r.get(11).ok().flatten(),
    });
    let subs_json: String = r.get(15)?;
    let subtitles: Vec<SubtitleTrack> = serde_json::from_str(&subs_json).unwrap_or_default();
    let tracks_json: String = r.get(17)?;
    let audio_tracks: Vec<AudioStream> = serde_json::from_str(&tracks_json).unwrap_or_default();
    // Representative audio = first listed track. Fall back to the legacy
    // a_codec/a_channels/a_language columns for rows probed before audio_tracks
    // existed (their JSON is still `[]`).
    let audio = audio_tracks.first().cloned().or_else(|| {
        r.get::<_, Option<String>>(12).ok().flatten().map(|codec| AudioStream {
            index: 0,
            codec,
            channels: r.get(13).ok().flatten(),
            language: r.get(14).ok().flatten(),
            title: None,
            default: true,
        })
    });

    Ok(MediaFile {
        id: r.get(0)?,
        rel_path: r.get(1)?,
        container: r.get(2)?,
        size: r.get::<_, Option<i64>>(3)?.map(|s| s as u64),
        edition: r.get(4)?,
        probed: probed != 0,
        duration_ms: r.get::<_, Option<i64>>(6)?.map(|d| d as u64),
        video,
        audio,
        audio_tracks,
        subtitles,
        abs_path: r.get(16)?,
    })
}

/// Load every file for one item, ordered best-first (highest resolution).
fn files_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<Vec<MediaFile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FILE_COLS} FROM files WHERE item_id = ?1 \
         ORDER BY (probed=1) DESC, v_width DESC NULLS LAST, id",
    ))?;
    let files = stmt
        .query_map(params![item_id], row_to_file)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(files)
}

/// Attach `files[]` to an item and mirror its representative file into the
/// top-level fields (video/audio/duration/container/subtitles/abs_path) for
/// backward compatibility. The representative is the highest-resolution probed
/// file; if none is probed yet, the first file (streams stay null).
pub(crate) fn attach_files(conn: &Connection, item: &mut MediaItem) -> rusqlite::Result<()> {
    let files = files_for_item(conn, &item.id)?;
    // Representative = first probed file (files are ordered probed-first,
    // highest-res-first), else the first file.
    let rep = files
        .iter()
        .find(|f| f.probed)
        .or_else(|| files.first());
    if let Some(rep) = rep {
        item.default_file_id = Some(rep.id.clone());
        // Demo files carry a synthetic `demo://` path and aren't streamable; keep
        // `abs_path` None for them so `/stream` returns the demo error.
        item.abs_path = rep
            .abs_path
            .clone()
            .filter(|p| !p.starts_with("demo://"));
        if rep.probed {
            item.container = rep.container.clone();
            item.duration_ms = rep.duration_ms;
            item.video = rep.video.clone();
            item.audio = rep.audio.clone();
            item.audio_tracks = rep.audio_tracks.clone();
            item.subtitles = rep.subtitles.clone();
            item.rel_path = rep.rel_path.clone();
        } else {
            // Unprobed: keep streams null but expose container/rel for browsing.
            item.container = rep.container.clone();
            item.rel_path = rep.rel_path.clone();
        }
    }
    item.files = files;
    Ok(())
}

/// Build a [`MediaItem`] base from a row selected with [`ITEM_COLS`]. The
/// representative stream fields and `files[]` are filled in afterwards by
/// [`attach_files`]; the legacy `items.v_*`/`a_*` columns are ignored (stream
/// data now lives on `files`).
pub(crate) fn row_to_item(r: &Row) -> rusqlite::Result<MediaItem> {
    let subs_json: String = r.get(14)?;
    let subtitles: Vec<SubtitleTrack> = serde_json::from_str(&subs_json).unwrap_or_default();

    let metadata = parse_metadata(r.get(25)?);

    Ok(MediaItem {
        id: r.get(0)?,
        kind: parse_kind(&r.get::<_, String>(1)?),
        title: r.get(2)?,
        year: r.get(3)?,
        duration_ms: r.get::<_, Option<i64>>(4)?.map(|d| d as u64),
        container: r.get(5)?,
        video: None,
        audio: None,
        audio_tracks: Vec::new(),
        subtitles,
        library: r.get(15)?,
        show_id: r.get(16)?,
        show_title: r.get(17)?,
        season: r.get(18)?,
        episode: r.get(19)?,
        episode_end: r.get(20)?,
        episode_title: r.get(21)?,
        rel_path: r.get(22)?,
        abs_path: r.get(23)?,
        added_at: r.get(24)?,
        metadata,
        files: Vec::new(),
        default_file_id: None,
    })
}

pub(crate) fn parse_kind(s: &str) -> Kind {
    match s {
        "episode" => Kind::Episode,
        "video" => Kind::Video,
        _ => Kind::Movie,
    }
}

pub(crate) fn now_or_blank() -> String {
    crate::scan::now_iso8601()
}
