//! Library scanning: walk media roots, parse names, group files into logical
//! items (Plex-style), and build the set of libraries/shows/items to persist.
//!
//! Phase 1 (this module's [`scan_all`]) is **fast**: it only `stat`s each video
//! file (size + mtime — no read, no ffprobe). Files are grouped into logical
//! items so the library is browsable in seconds. The slow per-file probing runs
//! later in [`crate::probe`]'s background pass.

use std::collections::HashMap;
use std::path::Path;

use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::{debug, info, warn};
use jwalk::{Parallelism, WalkDirGeneric};

use crate::model::{Kind, Library, LibraryKind, MediaFile, MediaItem, Show};
use crate::naming::{self, Parsed};
use crate::settings::LibraryDef;

/// Extensions we treat as playable video.
const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v", "mov", "webm", "avi", "ts"];

/// Everything a phase-1 scan produces, ready to hand to [`crate::db::sync_all`].
#[derive(Debug, Default)]
pub struct ScanData {
    pub libraries: Vec<Library>,
    pub shows: Vec<Show>,
    pub items: Vec<MediaItem>,
    /// `file_id -> mtime-secs` for every scanned file. Carried here (rather than
    /// on `MediaFile`, which is the client JSON contract) so the DB sync can
    /// detect changed files. Owned by this scan — no shared global, so two
    /// overlapping scans (watcher rescan + `POST /api/scan`) can't steal each
    /// other's entries.
    pub mtimes: HashMap<String, Option<i64>>,
}

/// Walk every configured library (each may span multiple folders) and build the
/// full index (phase 1, fast: no ffprobe). Files are `stat`-ed and grouped into
/// logical items. Items from every folder of a library share that library's id.
pub fn scan_all(defs: &[LibraryDef]) -> ScanData {
    let mut data = ScanData::default();
    // Logical items, keyed by stable logical id, accumulating their files.
    let mut items: HashMap<String, MediaItem> = HashMap::new();
    // Dedupe shows across the whole scan by show id.
    let mut shows: HashMap<String, Show> = HashMap::new();

    for def in defs {
        let mut movie_seen = false;
        let mut episode_seen = false;
        // Logical ids first seen in this library, to compute item_count.
        let mut lib_item_ids = std::collections::HashSet::new();

        for folder in &def.folders {
            let root = Path::new(folder);
            if !root.is_dir() {
                warn!(path = %root.display(), "media dir does not exist or is not a directory; skipping");
                continue;
            }
            scan_root(
                &def.id,
                root,
                &mut items,
                &mut shows,
                &mut data.mtimes,
                &mut lib_item_ids,
                &mut movie_seen,
                &mut episode_seen,
            );
        }

        // Auto-detect kind from contents, unless the def pins one.
        let detected = match (movie_seen, episode_seen) {
            (false, true) => LibraryKind::Shows,
            (true, true) => LibraryKind::Mixed,
            _ => LibraryKind::Movies,
        };
        let kind = match def.kind.as_str() {
            "movies" => LibraryKind::Movies,
            "shows" => LibraryKind::Shows,
            "mixed" => LibraryKind::Mixed,
            _ => detected,
        };

        info!(library = %def.name, items = lib_item_ids.len(), "scanned library");
        data.libraries.push(Library {
            id: def.id.clone(),
            name: def.name.clone(),
            kind,
            path: def.folders.join(", "),
            item_count: lib_item_ids.len(),
        });
    }

    data.shows = shows.into_values().collect();
    data.items = items.into_values().collect();
    data
}

/// Scan one folder belonging to `lib_id`, accumulating items (by logical id) and
/// shows into the shared maps. Flags/ids track what this library contributed so
/// the caller can compute its kind + item count across all its folders.
#[allow(clippy::too_many_arguments)]
fn scan_root(
    lib_id: &str,
    root: &Path,
    items: &mut HashMap<String, MediaItem>,
    shows: &mut HashMap<String, Show>,
    mtimes: &mut HashMap<String, Option<i64>>,
    lib_item_ids: &mut std::collections::HashSet<String>,
    movie_seen: &mut bool,
    episode_seen: &mut bool,
) {
    let lib_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library")
        .to_string();
    let added_at = now_iso8601();

    // Resolve the root once (cheap, single syscall) so file abs-paths are stable
    // without a per-file `canonicalize()` (which is very slow over SMB).
    let abs_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    // Phase-1 walk: jwalk traverses directories on a large thread pool and stats
    // files *in that pool*, so the readdir/stat network round-trips over SMB run
    // concurrently instead of serially (minutes → seconds). Synology `@eaDir`
    // and hidden dirs are pruned from the descent.
    let walk = WalkDirGeneric::<((), FileMeta)>::new(root)
        .follow_links(true)
        .skip_hidden(false)
        .parallelism(Parallelism::RayonNewPool(WALK_THREADS))
        .process_read_dir(|_depth, _path, _state, children| {
            children.retain(|res| match res {
                Ok(e) => !(e.file_type().is_dir() && is_pruned_dir(&e.file_name)),
                Err(_) => true,
            });
            for e in children.iter_mut().flatten() {
                if e.file_type().is_file() {
                    e.client_state = file_meta(&e.path());
                }
            }
        });

    for entry in walk.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !has_video_extension(&path) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        // Build the absolute path by joining the resolved root with the relative
        // path — no extra stat / symlink resolution per file.
        let abs = abs_root.join(&rel);

        // size + mtime were fetched during the parallel walk (above).
        let (size, mtime) = match entry.client_state {
            Some((s, m)) => (Some(s), Some(m)),
            None => (None, None),
        };

        let container = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let edition = detect_edition(file_name);

        let file = MediaFile {
            id: short_hash(&abs.to_string_lossy()),
            rel_path: Some(rel),
            container,
            duration_ms: None,
            video: None,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            size,
            edition,
            probed: false,
            abs_path: Some(abs.to_string_lossy().to_string()),
        };
        // Carry mtime alongside the file for the DB sync (not part of the JSON
        // contract). We stash it in a parallel map keyed by file id below.

        match naming::parse(root, &path) {
            Parsed::Movie { title, year } => {
                *movie_seen = true;
                let title = if title.is_empty() { "Untitled".into() } else { title };
                let logical = movie_logical_id(lib_id, &title, year);
                lib_item_ids.insert(logical.clone());
                let item = items.entry(logical.clone()).or_insert_with(|| MediaItem {
                    id: logical.clone(),
                    title: title.clone(),
                    kind: Kind::Movie,
                    year,
                    duration_ms: None,
                    container: String::new(),
                    video: None,
                    audio: None,
                    audio_tracks: Vec::new(),
                    subtitles: Vec::new(),
                    library: lib_id.to_string(),
                    show_id: None,
                    show_title: None,
                    season: None,
                    episode: None,
                    episode_end: None,
                    episode_title: None,
                    rel_path: None,
                    added_at: added_at.clone(),
                    metadata: None,
                    abs_path: None,
                    files: Vec::new(),
                    default_file_id: None,
                });
                mtimes.insert(file.id.clone(), mtime);
                item.files.push(file);
            }
            Parsed::Episode {
                show_title,
                show_year,
                season,
                episode,
                episode_end,
                episode_title,
            } => {
                *episode_seen = true;
                let show_id = show_key(lib_id, &show_title);
                shows
                    .entry(show_id.clone())
                    .and_modify(|s| {
                        if s.year.is_none() {
                            s.year = show_year;
                        }
                    })
                    .or_insert_with(|| Show {
                        id: show_id.clone(),
                        title: show_title.clone(),
                        year: show_year,
                        library: lib_id.to_string(),
                        season_count: 0,
                        episode_count: 0,
                        video: None,
                        added_at: added_at.clone(),
                        metadata: None,
                    });

                let logical = episode_logical_id(&show_id, season, episode);
                lib_item_ids.insert(logical.clone());
                let title = episode_title
                    .clone()
                    .unwrap_or_else(|| format!("S{season:02}E{episode:02}"));
                let item = items.entry(logical.clone()).or_insert_with(|| MediaItem {
                    id: logical.clone(),
                    title: title.clone(),
                    kind: Kind::Episode,
                    year: show_year,
                    duration_ms: None,
                    container: String::new(),
                    video: None,
                    audio: None,
                    audio_tracks: Vec::new(),
                    subtitles: Vec::new(),
                    library: lib_id.to_string(),
                    show_id: Some(show_id.clone()),
                    show_title: Some(show_title.clone()),
                    season: Some(season),
                    episode: Some(episode),
                    episode_end,
                    episode_title: episode_title.clone(),
                    rel_path: None,
                    added_at: added_at.clone(),
                    metadata: None,
                    abs_path: None,
                    files: Vec::new(),
                    default_file_id: None,
                });
                mtimes.insert(file.id.clone(), mtime);
                item.files.push(file);
            }
        }

        debug!("indexed file under {}", lib_name);
    }
}

/// Per-file metadata carried through the parallel jwalk: (size, mtime-secs).
type FileMeta = Option<(u64, i64)>;

/// Concurrency for the directory walk. SMB metadata ops are latency-bound (not
/// CPU), so we use many more threads than cores to overlap the round-trips.
const WALK_THREADS: usize = 64;

/// Directories pruned from the walk: Synology metadata (`@eaDir`) and hidden.
fn is_pruned_dir(name: &std::ffi::OsStr) -> bool {
    let n = name.to_string_lossy();
    n == "@eaDir" || n.starts_with('.')
}

/// `stat` one file (run inside the parallel walk pool): (size, mtime-as-unix-secs).
fn file_meta(path: &Path) -> FileMeta {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some((md.len(), mtime))
}

// ----- logical ids ------------------------------------------------------------

/// Stable movie logical id: same title+year → one item.
fn movie_logical_id(lib_id: &str, title: &str, year: Option<u32>) -> String {
    let norm = normalize_title(title);
    let year = year.map(|y| y.to_string()).unwrap_or_default();
    short_hash(&format!("{lib_id}|movie|{norm}|{year}"))
}

/// Stable episode logical id: same show/season/episode → one item.
fn episode_logical_id(show_id: &str, season: u32, episode: u32) -> String {
    short_hash(&format!("{show_id}|{season}|{episode}"))
}

fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Stable show id from library + normalised show title.
fn show_key(lib_id: &str, show_title: &str) -> String {
    let norm = normalize_title(show_title);
    short_hash(&format!("{lib_id}|show|{norm}"))
}

// ----- edition detection ------------------------------------------------------

/// Best-effort edition label from a filename. Keep it simple: scan for a known
/// set of edition/quality tokens and return the first match (preferring cut
/// labels over resolution/source). `None` when nothing notable is present.
fn detect_edition(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    // (needle, label) — cut/edition labels first, then source/quality.
    const TABLE: &[(&str, &str)] = &[
        ("director's cut", "Director's Cut"),
        ("directors cut", "Director's Cut"),
        ("director.cut", "Director's Cut"),
        ("extended", "Extended"),
        ("uncut", "Uncut"),
        ("unrated", "Unrated"),
        ("theatrical", "Theatrical"),
        ("remastered", "Remastered"),
        ("imax", "IMAX"),
        ("remux", "Remux"),
        ("2160p", "4K"),
        ("4k", "4K"),
        ("uhd", "4K"),
        ("1080p", "1080p"),
        ("720p", "720p"),
        ("480p", "480p"),
    ];
    TABLE
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, label)| label.to_string())
}

fn has_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// `hex(sha256(input))[..16]` — stable, short, collision-resistant enough.
pub fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

/// Current time as an RFC3339 / ISO8601 string (UTC).
pub fn now_iso8601() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
