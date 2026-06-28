//! SQLite persistence (rusqlite + r2d2 pool).
//!
//! The whole library lives in SQLite. A scan computes the full set of
//! libraries/shows/items and atomically swaps it in via [`replace_all`]. Read
//! queries run on `spawn_blocking` threads against a small connection pool.
//!
//! Performance: WAL journaling, `synchronous=NORMAL`, a 256 MiB mmap and a 16
//! MiB page cache are set on every pooled connection; reads never block the
//! single writer, and the indices below keep movie/show/episode lookups O(log n).

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

use std::collections::HashMap;

use crate::metadata::Metadata;
use crate::model::{
    AudioStream, ContinueItem, Invite, Kind, Library, LibraryKind, MediaFile, MediaItem,
    Permission, ProgressEntry, PublicUser, Season, Show, ShowDetail, SubtitleTrack, User,
    VideoStream,
};

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
const ITEM_COLS: &str = "id,kind,title,year,duration_ms,container,\
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
    ] {
        let _ = conn.execute(sql, []);
    }
}

/// Parse a stored `metadata` JSON blob into [`Metadata`]; tolerant of nulls and
/// stale shapes (returns `None`).
fn parse_metadata(json: Option<String>) -> Option<Metadata> {
    json.and_then(|j| serde_json::from_str::<Metadata>(&j).ok())
}

/// Attach resolved TMDB metadata to one item (used by the enrichment pass).
pub fn set_item_metadata(pool: &Pool, id: &str, meta: &Metadata) -> Result<()> {
    let conn = pool.get()?;
    let json = serde_json::to_string(meta)?;
    conn.execute("UPDATE items SET metadata = ?2 WHERE id = ?1", params![id, json])?;
    Ok(())
}

/// Attach resolved TMDB metadata to one show (used by the enrichment pass).
pub fn set_show_metadata(pool: &Pool, id: &str, meta: &Metadata) -> Result<()> {
    let conn = pool.get()?;
    let json = serde_json::to_string(meta)?;
    conn.execute("UPDATE shows SET metadata = ?2 WHERE id = ?1", params![id, json])?;
    Ok(())
}

/// (file_id, abs_path, owning item_id) for every file awaiting an ffprobe pass.
/// Drives the phase-2 background probing.
pub fn unprobed_files(pool: &Pool) -> Result<Vec<(String, String, String)>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT id, abs_path, item_id FROM files WHERE probed = 0")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Whether a given item already has at least one probed file (used to decide
/// whether a probe is the *first* one for an item → emit an ItemUpdated).
pub fn item_has_probed_file(pool: &Pool, item_id: &str) -> Result<bool> {
    let conn = pool.get()?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE item_id = ?1 AND probed = 1",
        params![item_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Persist the probe result for one file (sets stream columns + `probed=1`),
/// then recompute the owning item's representative columns.
pub fn set_file_probe(
    pool: &Pool,
    file_id: &str,
    duration_ms: Option<u64>,
    video: Option<&VideoStream>,
    audio: Option<&AudioStream>,
    audio_tracks: &[AudioStream],
    subtitles: &[SubtitleTrack],
) -> Result<()> {
    let conn = pool.get()?;
    let subs = serde_json::to_string(subtitles).unwrap_or_else(|_| "[]".into());
    let a_tracks = serde_json::to_string(audio_tracks).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE files SET probed=1, duration_ms=?2, \
            v_codec=?3, v_width=?4, v_height=?5, v_hdr=?6, v_bit_depth=?7, \
            a_codec=?8, a_channels=?9, a_language=?10, subtitles=?11, audio_tracks=?12 \
         WHERE id = ?1",
        params![
            file_id,
            duration_ms.map(|d| d as i64),
            video.map(|v| v.codec.clone()),
            video.and_then(|v| v.width),
            video.and_then(|v| v.height),
            video.map(|v| v.hdr as i64),
            video.and_then(|v| v.bit_depth),
            audio.map(|a| a.codec.clone()),
            audio.and_then(|a| a.channels),
            audio.and_then(|a| a.language.clone()),
            subs,
            a_tracks,
        ],
    )?;

    // Recompute the owning item's representative columns.
    let item_id: Option<String> = conn
        .query_row("SELECT item_id FROM files WHERE id = ?1", params![file_id], |r| r.get(0))
        .ok();
    if let Some(item_id) = item_id {
        recompute_item_representative(&conn, &item_id)?;
    }
    Ok(())
}

/// Recompute one item's representative (top-level) columns from its
/// highest-resolution probed file: container/duration/video/audio/subtitles and
/// the representative `abs_path`/`rel_path`.
fn recompute_item_representative(conn: &Connection, item_id: &str) -> Result<()> {
    // Best probed file for this item = highest v_width (then any probed).
    let best: Option<(String, String, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT abs_path, container, rel_path, duration_ms FROM files \
             WHERE item_id = ?1 AND probed = 1 \
             ORDER BY v_width DESC NULLS LAST, id LIMIT 1",
            params![item_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .ok();

    if let Some((abs, container, rel, duration)) = best {
        conn.execute(
            "UPDATE items SET container=?2, abs_path=?3, rel_path=?4, duration_ms=?5 WHERE id=?1",
            params![item_id, container, abs, rel, duration],
        )?;
    }
    Ok(())
}

/// Recompute representative columns for every item that has a probed file.
fn recompute_all_representatives(pool: &Pool) -> Result<()> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT item_id FROM files WHERE probed = 1",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        recompute_item_representative(&conn, &id)?;
    }
    Ok(())
}

/// Diff-sync the scanned index into the DB in one transaction.
///
/// Unlike a blunt DELETE-all + INSERT, this PRESERVES the expensive bits across
/// rescans:
///   * `items.metadata` / `shows.metadata` (TMDB art) is never overwritten.
///   * A file's probed stream data is kept when its `size`+`mtime` are unchanged;
///     only new or modified files get `probed=0` and will be re-probed.
///
/// `mtimes` maps file id → unix-seconds mtime collected during the scan (see
/// [`crate::scan::take_mtimes`]). `items` carry their `files[]`.
pub fn sync_all(
    pool: &Pool,
    libraries: &[Library],
    shows: &[Show],
    items: &[MediaItem],
    mtimes: &HashMap<String, Option<i64>>,
) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;

    // 1) Libraries — UPSERT by id. We must NOT `DELETE FROM libraries` wholesale:
    //    `items`/`files` cascade-delete from libraries, which would wipe all the
    //    precious probed data and metadata we're trying to preserve. Instead
    //    upsert each library, then delete only libraries no longer scanned (whose
    //    cascade is the correct behaviour — their items are gone too).
    {
        let mut lib_stmt = tx.prepare(
            "INSERT INTO libraries (id,name,kind,path,added_at) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, path=excluded.path",
        )?;
        for l in libraries {
            lib_stmt.execute(params![l.id, l.name, library_kind_str(&l.kind), l.path, now_or_blank()])?;
        }
        // Delete libraries that vanished from the scan (cascades their items/files).
        let keep: Vec<String> = libraries.iter().map(|l| l.id.clone()).collect();
        let mut existing: Vec<String> = Vec::new();
        {
            let mut q = tx.prepare("SELECT id FROM libraries")?;
            let rows = q.query_map([], |r| r.get::<_, String>(0))?;
            for r in rows {
                existing.push(r?);
            }
        }
        let mut del = tx.prepare("DELETE FROM libraries WHERE id = ?1")?;
        for id in &existing {
            if !keep.contains(id) {
                del.execute(params![id])?;
            }
        }
    }

    // 2) Shows — upsert without ever touching `metadata`.
    {
        let mut show_stmt = tx.prepare(
            "INSERT INTO shows (id,library,title,year,added_at) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET library=excluded.library, title=excluded.title, \
                 year=COALESCE(excluded.year, shows.year)",
        )?;
        for s in shows {
            show_stmt.execute(params![s.id, s.library, s.title, s.year, s.added_at])?;
        }
    }

    // 3) Items — upsert without ever touching `metadata`.
    {
        let mut item_stmt = tx.prepare(
            "INSERT INTO items \
                (id,kind,title,year,container,library,show_id,show_title,\
                 season,episode,episode_end,episode_title,rel_path,abs_path,added_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) \
             ON CONFLICT(id) DO UPDATE SET \
                 kind=excluded.kind, title=excluded.title, year=excluded.year, \
                 library=excluded.library, show_id=excluded.show_id, \
                 show_title=excluded.show_title, season=excluded.season, \
                 episode=excluded.episode, episode_end=excluded.episode_end, \
                 episode_title=excluded.episode_title",
        )?;
        for i in items {
            // The item's container/rel_path/abs_path mirror its (first) file until
            // probing recomputes the representative; pick the first file as seed.
            let seed = i.files.first();
            let container = seed.map(|f| f.container.clone()).unwrap_or_default();
            let rel_path = seed.and_then(|f| f.rel_path.clone());
            let abs_path = seed.and_then(|f| f.abs_path.clone());
            item_stmt.execute(params![
                i.id,
                kind_str(&i.kind),
                i.title,
                i.year,
                container,
                i.library,
                i.show_id,
                i.show_title,
                i.season,
                i.episode,
                i.episode_end,
                i.episode_title,
                rel_path,
                abs_path,
                i.added_at,
            ])?;
        }
    }

    // 4) Files — diff sync by abs_path. Delete files no longer on disk; upsert
    //    scanned files, resetting `probed=0` only when size/mtime changed.
    {
        // Build the set of abs_paths we just scanned.
        let scanned: std::collections::HashSet<&str> = items
            .iter()
            .flat_map(|i| i.files.iter())
            .filter_map(|f| f.abs_path.as_deref())
            .collect();

        // Delete DB file rows whose abs_path is gone from disk.
        let mut existing: Vec<(String, String)> = Vec::new();
        {
            let mut q = tx.prepare("SELECT id, abs_path FROM files")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for r in rows {
                existing.push(r?);
            }
        }
        {
            let mut del = tx.prepare("DELETE FROM files WHERE id = ?1")?;
            for (id, abs) in &existing {
                if !scanned.contains(abs.as_str()) {
                    del.execute(params![id])?;
                }
            }
        }

        // Existing (size, mtime, probed) keyed by abs_path, to decide reuse.
        let mut prev: HashMap<String, (Option<i64>, Option<i64>, i64)> = HashMap::new();
        {
            let mut q = tx.prepare("SELECT abs_path, size, mtime, probed FROM files")?;
            let rows = q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?;
            for r in rows {
                let (abs, size, mtime, probed) = r?;
                prev.insert(abs, (size, mtime, probed));
            }
        }

        // Upsert each scanned file. When size+mtime match an already-probed row,
        // keep probed=1 and DON'T touch its stream columns. Otherwise reset.
        let mut keep_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition",
        )?;
        let mut reset_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition,probed,\
                 duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,a_codec,a_channels,a_language,subtitles,audio_tracks) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'[]','[]') \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition, probed=0, duration_ms=NULL, v_codec=NULL, v_width=NULL, \
                 v_height=NULL, v_hdr=NULL, v_bit_depth=NULL, a_codec=NULL, a_channels=NULL, \
                 a_language=NULL, subtitles='[]', audio_tracks='[]'",
        )?;
        // Files that arrive already probed (demo/seed content): store their stream
        // data directly as probed=1 so they never enter the phase-2 pass.
        let mut preprobed_stmt = tx.prepare(
            "INSERT INTO files (id,item_id,abs_path,rel_path,container,size,mtime,edition,probed,\
                 duration_ms,v_codec,v_width,v_height,v_hdr,v_bit_depth,a_codec,a_channels,a_language,subtitles,audio_tracks) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19) \
             ON CONFLICT(abs_path) DO UPDATE SET \
                 id=excluded.id, item_id=excluded.item_id, rel_path=excluded.rel_path, \
                 container=excluded.container, size=excluded.size, mtime=excluded.mtime, \
                 edition=excluded.edition, probed=1, duration_ms=excluded.duration_ms, \
                 v_codec=excluded.v_codec, v_width=excluded.v_width, v_height=excluded.v_height, \
                 v_hdr=excluded.v_hdr, v_bit_depth=excluded.v_bit_depth, a_codec=excluded.a_codec, \
                 a_channels=excluded.a_channels, a_language=excluded.a_language, subtitles=excluded.subtitles, \
                 audio_tracks=excluded.audio_tracks",
        )?;

        for i in items {
            for f in &i.files {
                let Some(abs) = f.abs_path.as_deref() else { continue };
                let size = f.size.map(|s| s as i64);
                let mtime = mtimes.get(&f.id).copied().flatten();

                if f.probed {
                    // Pre-probed (demo): store the supplied stream data.
                    let v = f.video.as_ref();
                    let a = f.audio.as_ref();
                    let subs = serde_json::to_string(&f.subtitles).unwrap_or_else(|_| "[]".into());
                    let a_tracks = serde_json::to_string(&f.audio_tracks).unwrap_or_else(|_| "[]".into());
                    preprobed_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                        f.duration_ms.map(|d| d as i64),
                        v.map(|v| v.codec.clone()),
                        v.and_then(|v| v.width),
                        v.and_then(|v| v.height),
                        v.map(|v| v.hdr as i64),
                        v.and_then(|v| v.bit_depth),
                        a.map(|a| a.codec.clone()),
                        a.and_then(|a| a.channels),
                        a.and_then(|a| a.language.clone()),
                        subs,
                        a_tracks,
                    ])?;
                    continue;
                }

                let unchanged_probed = prev.get(abs).is_some_and(|(psize, pmtime, probed)| {
                    *probed == 1 && *psize == size && *pmtime == mtime
                });
                if unchanged_probed {
                    keep_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                    ])?;
                } else {
                    reset_stmt.execute(params![
                        f.id, i.id, abs, f.rel_path, f.container, size, mtime, f.edition,
                    ])?;
                }
            }
        }
    }

    // 5) Prune items/shows that now have zero backing files/episodes.
    tx.execute("DELETE FROM items WHERE id NOT IN (SELECT DISTINCT item_id FROM files)", [])?;
    tx.execute("DELETE FROM shows WHERE id NOT IN (SELECT DISTINCT show_id FROM items WHERE show_id IS NOT NULL)", [])?;

    tx.commit()?;

    // 6) Recompute every item's representative columns from its probed files.
    recompute_all_representatives(pool)?;
    Ok(())
}

/// (libraries, items, shows) counts for `/api/health`.
pub fn counts(pool: &Pool) -> Result<(usize, usize, usize)> {
    let conn = pool.get()?;
    let libs: i64 = conn.query_row("SELECT COUNT(*) FROM libraries", [], |r| r.get(0))?;
    let items: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
    let shows: i64 = conn.query_row("SELECT COUNT(*) FROM shows", [], |r| r.get(0))?;
    Ok((libs as usize, items as usize, shows as usize))
}

pub fn list_libraries(pool: &Pool) -> Result<Vec<Library>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,name,kind,path,(SELECT COUNT(*) FROM items i WHERE i.library=l.id) \
         FROM libraries l ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Library {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: parse_library_kind(&r.get::<_, String>(2)?),
            path: r.get(3)?,
            item_count: r.get::<_, i64>(4)? as usize,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Movies (and loose videos) — everything that isn't an episode.
pub fn list_movies(pool: &Pool, library: Option<&str>) -> Result<Vec<MediaItem>> {
    query_items(
        pool,
        &format!("SELECT {ITEM_COLS} FROM items WHERE kind != 'episode'"),
        library,
        "ORDER BY title COLLATE NOCASE",
    )
}

/// All playable items (movies + episodes) — backwards-compatible `/api/items`.
pub fn list_items(pool: &Pool, library: Option<&str>) -> Result<Vec<MediaItem>> {
    query_items(
        pool,
        &format!("SELECT {ITEM_COLS} FROM items"),
        library,
        "ORDER BY title COLLATE NOCASE",
    )
}

pub fn get_item(pool: &Pool, id: &str) -> Result<Option<MediaItem>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_item)?;
    match rows.next() {
        Some(item) => {
            let mut item = item?;
            attach_files(&conn, &mut item)?;
            Ok(Some(item))
        }
        None => Ok(None),
    }
}

pub fn list_shows(pool: &Pool, library: Option<&str>) -> Result<Vec<Show>> {
    let conn = pool.get()?;
    let (where_sql, want_lib) = match library {
        Some(_) => ("WHERE s.library = ?1", true),
        None => ("", false),
    };
    let sql = format!(
        "SELECT s.id,s.title,s.year,s.library,s.added_at,\
            (SELECT COUNT(DISTINCT i.season) FROM items i WHERE i.show_id=s.id),\
            (SELECT COUNT(*) FROM items i WHERE i.show_id=s.id),\
            s.metadata \
         FROM shows s {where_sql} ORDER BY s.title COLLATE NOCASE",
    );
    let mut stmt = conn.prepare(&sql)?;

    let map = |r: &Row| -> rusqlite::Result<Show> {
        Ok(Show {
            id: r.get(0)?,
            title: r.get(1)?,
            year: r.get(2)?,
            library: r.get(3)?,
            added_at: r.get(4)?,
            season_count: r.get::<_, i64>(5)? as u32,
            episode_count: r.get::<_, i64>(6)? as u32,
            video: None,
            metadata: parse_metadata(r.get(7)?),
        })
    };
    let mut shows: Vec<Show> = if want_lib {
        stmt.query_map(params![library.unwrap()], map)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for s in &mut shows {
        s.video = representative_video(&conn, &s.id)?;
    }
    Ok(shows)
}

/// Cheap title lookup for show poster rendering.
pub fn show_title(pool: &Pool, id: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    Ok(conn
        .query_row("SELECT title FROM shows WHERE id = ?1", params![id], |r| r.get(0))
        .ok())
}

pub fn get_show(pool: &Pool, id: &str) -> Result<Option<ShowDetail>> {
    let conn = pool.get()?;
    let show = conn
        .query_row(
            "SELECT id,title,year,library,added_at,metadata FROM shows WHERE id = ?1",
            params![id],
            |r| {
                Ok(Show {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    year: r.get(2)?,
                    library: r.get(3)?,
                    added_at: r.get(4)?,
                    season_count: 0,
                    episode_count: 0,
                    video: None,
                    metadata: parse_metadata(r.get(5)?),
                })
            },
        )
        .ok();

    let Some(mut show) = show else { return Ok(None) };

    let mut stmt = conn.prepare(&format!(
        "SELECT {ITEM_COLS} FROM items WHERE show_id = ?1 \
         ORDER BY season, episode",
    ))?;
    let mut episodes: Vec<MediaItem> = stmt
        .query_map(params![id], row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for ep in &mut episodes {
        attach_files(&conn, ep)?;
    }

    // Group into seasons.
    let mut seasons: Vec<Season> = Vec::new();
    for ep in episodes.iter().cloned() {
        let n = ep.season.unwrap_or(0);
        match seasons.iter_mut().find(|s| s.number == n) {
            Some(s) => s.episodes.push(ep),
            None => seasons.push(Season { number: n, episodes: vec![ep] }),
        }
    }
    seasons.sort_by_key(|s| s.number);

    show.episode_count = episodes.len() as u32;
    show.season_count = seasons.len() as u32;
    show.video = representative_video(&conn, id)?;

    Ok(Some(ShowDetail { show, seasons }))
}

/// Pick a representative video stream for a show — the highest-resolution probed
/// file across all of the show's episodes.
fn representative_video(conn: &rusqlite::Connection, show_id: &str) -> Result<Option<VideoStream>> {
    let mut stmt = conn.prepare(
        "SELECT f.v_codec,f.v_width,f.v_height,f.v_hdr,f.v_bit_depth \
         FROM files f JOIN items i ON f.item_id = i.id \
         WHERE i.show_id = ?1 AND f.probed = 1 AND f.v_codec IS NOT NULL \
         ORDER BY f.v_width DESC NULLS LAST LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![show_id], |r| {
        Ok(VideoStream {
            codec: r.get::<_, String>(0)?,
            width: r.get(1)?,
            height: r.get(2)?,
            hdr: r.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0,
            bit_depth: r.get(4)?,
        })
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

// ----- users / sessions / progress --------------------------------------------

/// Map a row of `id,email,username,avatar_url,created_at,permissions,language` to
/// a [`User`].
fn row_to_user(r: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        email: r.get(1)?,
        username: r.get(2)?,
        avatar_url: r.get(3)?,
        created_at: r.get(4)?,
        permissions: parse_permissions(&r.get::<_, String>(5)?),
        language: r.get(6)?,
    })
}

/// Parse a stored `permissions` JSON array of string keys, dropping any unknown
/// keys (tolerant forward-compat). Falls back to `[Playback]` on malformed JSON.
fn parse_permissions(json: &str) -> Vec<Permission> {
    match serde_json::from_str::<Vec<String>>(json) {
        Ok(keys) => keys.iter().filter_map(|k| Permission::parse(k)).collect(),
        Err(_) => vec![Permission::Playback],
    }
}

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
        "SELECT id,email,username,avatar_url,created_at,permissions,language,password_hash FROM users WHERE email = ?1",
    )?;
    let mut rows = stmt.query_map(params![email], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(7)?))
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
        "SELECT id,email,username,avatar_url,created_at,permissions,language,password_hash FROM users \
         WHERE email = ?1 COLLATE NOCASE OR username = ?1 LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![identifier], |r| {
        Ok((row_to_user(r)?, r.get::<_, String>(7)?))
    })?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// All users as the public (no-email) shape, for the profile picker.
pub fn list_users(pool: &Pool) -> Result<Vec<PublicUser>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT id,username,avatar_url FROM users ORDER BY created_at")?;
    let rows = stmt.query_map([], |r| {
        Ok(PublicUser {
            id: r.get(0)?,
            username: r.get(1)?,
            avatar_url: r.get(2)?,
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
        "SELECT u.id,u.email,u.username,u.avatar_url,u.created_at,u.permissions,u.language \
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

/// Upsert one item's playback position for a user.
pub fn upsert_progress(
    pool: &Pool,
    user_id: &str,
    item_id: &str,
    position_ms: i64,
    duration_ms: Option<i64>,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO progress (user_id,item_id,position_ms,duration_ms,updated_at) \
         VALUES (?1,?2,?3,?4,?5) \
         ON CONFLICT(user_id,item_id) DO UPDATE SET \
            position_ms=excluded.position_ms, duration_ms=excluded.duration_ms, \
            updated_at=excluded.updated_at",
        params![user_id, item_id, position_ms, duration_ms, now_or_blank()],
    )?;
    Ok(())
}

/// One item's saved progress for a user, if any.
pub fn get_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<Option<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 AND item_id = ?2",
    )?;
    let mut rows = stmt.query_map(params![user_id, item_id], row_to_progress)?;
    match rows.next() {
        Some(p) => Ok(Some(p?)),
        None => Ok(None),
    }
}

/// Every saved progress row for a user (newest first).
pub fn list_progress(pool: &Pool, user_id: &str) -> Result<Vec<ProgressEntry>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT item_id,position_ms,duration_ms,updated_at FROM progress \
         WHERE user_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map(params![user_id], row_to_progress)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Remove a saved position (e.g. finished, or "remove from Continue Watching").
pub fn delete_progress(pool: &Pool, user_id: &str, item_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM progress WHERE user_id = ?1 AND item_id = ?2",
        params![user_id, item_id],
    )?;
    Ok(())
}

/// "Continue watching": resumable items (started, not yet ~finished), newest
/// first, each carried as a full [`MediaItem`] so clients render normal cards.
pub fn continue_watching(pool: &Pool, user_id: &str) -> Result<Vec<ContinueItem>> {
    let conn = pool.get()?;
    // 1) The resumable item ids + their progress. The JOIN drops any orphan
    //    progress row whose item no longer exists.
    let mut stmt = conn.prepare(
        "SELECT p.item_id,p.position_ms,p.duration_ms,p.updated_at \
         FROM progress p JOIN items i ON i.id = p.item_id \
         WHERE p.user_id = ?1 AND p.position_ms > 15000 \
           AND (p.duration_ms IS NULL OR p.position_ms < p.duration_ms * 95 / 100) \
         ORDER BY p.updated_at DESC LIMIT 30",
    )?;
    let rows: Vec<(String, i64, Option<i64>, String)> = stmt
        .query_map(params![user_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // 2) Hydrate each into a full item (with files) on the same connection.
    let mut item_stmt = conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"))?;
    let mut out = Vec::with_capacity(rows.len());
    for (item_id, position_ms, duration_ms, updated_at) in rows {
        let mut it = item_stmt.query_map(params![item_id], row_to_item)?;
        if let Some(item) = it.next() {
            let mut item = item?;
            attach_files(&conn, &mut item)?;
            out.push(ContinueItem { item, position_ms, duration_ms, updated_at });
        }
    }
    Ok(out)
}

/// Map a row of `item_id,position_ms,duration_ms,updated_at` to a [`ProgressEntry`].
fn row_to_progress(r: &Row) -> rusqlite::Result<ProgressEntry> {
    Ok(ProgressEntry {
        item_id: r.get(0)?,
        position_ms: r.get(1)?,
        duration_ms: r.get(2)?,
        updated_at: r.get(3)?,
    })
}

// ----- helpers ----------------------------------------------------------------

fn query_items(pool: &Pool, base: &str, library: Option<&str>, tail: &str) -> Result<Vec<MediaItem>> {
    let conn = pool.get()?;
    let mut items: Vec<MediaItem> = match library {
        Some(lib) => {
            let sql = format!("{base} {} {tail}", if base.contains("WHERE") { "AND library = ?1" } else { "WHERE library = ?1" });
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![lib], row_to_item)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            let sql = format!("{base} {tail}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], row_to_item)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    for item in &mut items {
        attach_files(&conn, item)?;
    }
    Ok(items)
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
fn attach_files(conn: &Connection, item: &mut MediaItem) -> rusqlite::Result<()> {
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
fn row_to_item(r: &Row) -> rusqlite::Result<MediaItem> {
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

fn kind_str(k: &Kind) -> &'static str {
    match k {
        Kind::Movie => "movie",
        Kind::Episode => "episode",
        Kind::Video => "video",
    }
}

fn parse_kind(s: &str) -> Kind {
    match s {
        "episode" => Kind::Episode,
        "video" => Kind::Video,
        _ => Kind::Movie,
    }
}

fn library_kind_str(k: &LibraryKind) -> &'static str {
    match k {
        LibraryKind::Movies => "movies",
        LibraryKind::Shows => "shows",
        LibraryKind::Mixed => "mixed",
    }
}

fn parse_library_kind(s: &str) -> LibraryKind {
    match s {
        "shows" => LibraryKind::Shows,
        "mixed" => LibraryKind::Mixed,
        _ => LibraryKind::Movies,
    }
}

fn now_or_blank() -> String {
    crate::scan::now_iso8601()
}

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
    // Reuse the User shape (cols 0..=5 match row_to_user); last_seen handled by caller.
    row_to_user(r)
}

/// All accounts for the admin "Membres & partage" table, oldest first (owner is
/// account 0). `online` is left false here — the handler fills it from the live
/// playback registry.
pub fn admin_users(pool: &Pool) -> Result<Vec<crate::model::AdminUser>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,email,username,avatar_url,created_at,permissions,last_seen \
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
        out.push(crate::model::AdminUser {
            role: crate::model::role_label(&u.permissions).to_string(),
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
            "SELECT id,email,username,avatar_url,created_at,permissions,language FROM users WHERE id = ?1",
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
    let id = crate::scan::short_hash(&format!(
        "play|{}|{}|{started_at}|{}",
        user_id.unwrap_or("?"),
        item_id.unwrap_or("?"),
        crate::auth::random_token()
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
pub fn top_users(pool: &Pool, since: i64, limit: usize) -> Result<Vec<crate::model::TopUser>> {
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
        Ok(crate::model::TopUser {
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
pub fn history_since(pool: &Pool, since: i64) -> Result<Vec<crate::model::HistoryRow>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT ended_at,kind,watched_ms FROM play_history WHERE ended_at >= ?1 ORDER BY ended_at",
    )?;
    let rows = stmt.query_map(params![since], |r| {
        Ok(crate::model::HistoryRow {
            ended_at: r.get(0)?,
            kind: parse_kind(&r.get::<_, String>(1)?),
            watched_ms: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ----- admin: library + storage stats ----------------------------------------

/// Per-library item count + total bytes on disk (joins items→files).
pub fn library_stats(pool: &Pool) -> Result<Vec<crate::model::LibraryStat>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT i.library, COUNT(DISTINCT i.id) AS items, COALESCE(SUM(f.size),0) AS bytes \
         FROM items i LEFT JOIN files f ON f.item_id = i.id \
         GROUP BY i.library",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::model::LibraryStat {
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
