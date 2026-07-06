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
//! This module is the directory root: it owns the connection pool and the
//! shared row-mappers/helpers (the schema DDL plus `init`/`migrate` live in the
//! [`schema`] submodule), and re-exports the per-domain query submodules below
//! as a flat namespace so `db::list_movies(...)` etc. resolve unchanged.

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::model::{
    AudioStream, Kind, MediaFile, MediaItem, Metadata, Permission, SubtitleTrack, User, VideoStream,
};

mod media;
mod catalog_query;
mod ingest;
mod markers;
mod downloaded_subs;
mod downloads;
mod indexers;
mod accounts;
mod playback;
mod library;
mod admin;
mod jobs;
// Kept namespaced (`db::pipeline::…`) rather than glob-exported: its `counts`
// would clash with `media::counts`, and the call sites read clearer scoped.
pub mod pipeline;
mod requests;
mod taste;
mod curated;
mod suggest;
mod schema;
mod vectors;
mod home;
mod backup;

pub use media::*;
pub use catalog_query::*;
pub use ingest::*;
pub use markers::*;
pub use downloaded_subs::*;
pub use downloads::*;
pub use indexers::*;
pub use vectors::*;
pub use home::*;
pub use accounts::*;
pub use playback::*;
pub use library::*;
pub use admin::*;
pub use jobs::*;
pub use requests::*;
pub use taste::*;
pub use curated::*;
pub use suggest::*;
pub use backup::*;
pub use schema::init;
pub(crate) use schema::{FILE_COLS, ITEM_COLS, PRAGMAS};

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

// ----- shared row-mappers / helpers -------------------------------------------

/// Parse a stored `metadata` JSON blob into [`Metadata`]; tolerant of nulls and
/// stale shapes (returns `None`).
pub(crate) fn parse_metadata(json: Option<String>) -> Option<Metadata> {
    json.and_then(|j| serde_json::from_str::<Metadata>(&j).ok())
}

/// Map a row of
/// `id,email,username,avatar_url,created_at,permissions,language,has_pin` to a
/// [`User`]. Column 7 is a boolean (`pin_hash IS NOT NULL`) every SELECT that
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
    apply_files(item, files);
    // Episodes carry intro/credits markers (skip-intro + next-up-at-credits).
    if item.kind == Kind::Episode {
        item.markers = markers::markers_for_item(conn, &item.id)?;
    }
    Ok(())
}

/// [`attach_files`] over a whole slice in a fixed number of queries: one files
/// query + one markers query per id-chunk, instead of 1-2 queries *per item*.
/// Every multi-item read path (listings, home rows, continue watching, search
/// and recommendation hydration) goes through this; on an HDD-backed NAS the
/// per-query overhead of the N+1 pattern dominated those endpoints.
pub(crate) fn attach_files_batch(conn: &Connection, items: &mut [MediaItem]) -> rusqlite::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    use std::collections::HashMap;

    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let mut files_by_item: HashMap<String, Vec<MediaFile>> = HashMap::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        // Appending item_id after FILE_COLS keeps row_to_file's indices stable.
        // The ORDER BY matches files_for_item, so each per-item group arrives
        // best-first and pushing preserves that order.
        let mut stmt = conn.prepare(&format!(
            "SELECT {FILE_COLS},item_id FROM files WHERE item_id IN ({ph}) \
             ORDER BY (probed=1) DESC, v_width DESC NULLS LAST, id",
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
            Ok((r.get::<_, String>(18)?, row_to_file(r)?))
        })?;
        for row in rows {
            let (item_id, file) = row?;
            files_by_item.entry(item_id).or_default().push(file);
        }
    }

    let episode_ids: Vec<&str> = items
        .iter()
        .filter(|i| i.kind == Kind::Episode)
        .map(|i| i.id.as_str())
        .collect();
    let mut markers_by_item = markers::markers_for_items(conn, &episode_ids)?;

    for item in items.iter_mut() {
        let files = files_by_item.remove(&item.id).unwrap_or_default();
        apply_files(item, files);
        if item.kind == Kind::Episode {
            item.markers = markers_by_item.remove(&item.id).unwrap_or_default();
        }
    }
    Ok(())
}

/// Chunk size for `IN (...)` id lists: comfortably under SQLite's bound-variable
/// limit while keeping the query count per batch effectively constant.
pub(crate) const IN_CHUNK: usize = 800;

/// Mirror the representative file into the item's top-level fields (the shared
/// tail of [`attach_files`] / [`attach_files_batch`]).
fn apply_files(item: &mut MediaItem, files: Vec<MediaFile>) {
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
}

/// Hydrate ids into full [`MediaItem`]s (files + markers batched), preserving
/// the input order and silently dropping unknown ids.
pub(crate) fn items_by_ids_ordered(conn: &Connection, ids: &[&str]) -> rusqlite::Result<Vec<MediaItem>> {
    use std::collections::HashMap;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_id: HashMap<String, MediaItem> = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt =
            conn.prepare(&format!("SELECT {ITEM_COLS} FROM items WHERE id IN ({ph})"))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), row_to_item)?;
        for item in rows {
            let item = item?;
            by_id.insert(item.id.clone(), item);
        }
    }
    let mut items: Vec<MediaItem> = ids.iter().filter_map(|id| by_id.remove(*id)).collect();
    attach_files_batch(conn, &mut items)?;
    Ok(items)
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
        markers: Vec::new(),
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
    crate::services::scan::now_iso8601()
}
