//! The phase-1 filesystem walk: a parallel jwalk traversal of one library
//! folder that `stat`s each video file and groups it (via [`crate::domain::naming`])
//! into the shared logical-item / show maps.

use std::collections::HashMap;
use std::path::Path;

use jwalk::{Parallelism, WalkDirGeneric};
use tracing::debug;

use crate::domain::naming::{self, Parsed};
use crate::model::{Kind, MediaFile, MediaItem, Show};

use super::ids::{detect_edition, episode_logical_id, movie_logical_id, short_hash, show_key};
use super::now_iso8601;

/// Extensions we treat as playable video. Public so the relocated acquisition
/// importer (tv.kroma.torrents) can share the exact same set.
pub const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v", "mov", "webm", "avi", "ts"];

/// Scan one folder belonging to `lib_id`, accumulating items (by logical id) and
/// shows into the shared maps. Flags/ids track what this library contributed so
/// the caller can compute its kind + item count across all its folders.
#[allow(clippy::too_many_arguments)]
pub(super) fn scan_root(
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
        .parallelism(Parallelism::RayonNewPool(walk_threads()))
        .process_read_dir(|_depth, _path, _state, children| prepare_children(children));

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
        // path no extra stat / symlink resolution per file.
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

        index_parsed(
            naming::parse(root, &path),
            lib_id,
            &added_at,
            file,
            mtime,
            items,
            shows,
            mtimes,
            lib_item_ids,
            movie_seen,
            episode_seen,
        );

        debug!("indexed file under {}", lib_name);
    }
}

/// jwalk `process_read_dir` body: prune Synology/hidden dirs from the descent and
/// stat each file *in the walk's thread pool* (so the SMB round-trips overlap).
fn prepare_children(children: &mut Vec<jwalk::Result<jwalk::DirEntry<((), FileMeta)>>>) {
    children.retain(|res| match res {
        Ok(e) => !(e.file_type().is_dir() && is_pruned_dir(&e.file_name)),
        Err(_) => true,
    });
    for e in children.iter_mut().flatten() {
        if e.file_type().is_file() {
            e.client_state = file_meta(&e.path());
        }
    }
}

/// Fold one parsed video file into the shared item / show maps: a movie upserts a
/// `Movie` item; an episode upserts its show (widening a missing year) then the
/// episode item. Either way the file + its mtime are recorded.
#[allow(clippy::too_many_arguments)]
fn index_parsed(
    parsed: Parsed,
    lib_id: &str,
    added_at: &str,
    file: MediaFile,
    mtime: Option<i64>,
    items: &mut HashMap<String, MediaItem>,
    shows: &mut HashMap<String, Show>,
    mtimes: &mut HashMap<String, Option<i64>>,
    lib_item_ids: &mut std::collections::HashSet<String>,
    movie_seen: &mut bool,
    episode_seen: &mut bool,
) {
    match parsed {
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
                added_at: added_at.to_string(),
                metadata: None,
                abs_path: None,
                files: Vec::new(),
                default_file_id: None,
                markers: Vec::new(),
                audio_analysis: None,
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
                    added_at: added_at.to_string(),
                    metadata: None,
                    progress: None,
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
                added_at: added_at.to_string(),
                metadata: None,
                abs_path: None,
                files: Vec::new(),
                default_file_id: None,
                markers: Vec::new(),
                audio_analysis: None,
            });
            mtimes.insert(file.id.clone(), mtime);
            item.files.push(file);
        }
    }
}

/// Per-file metadata carried through the parallel jwalk: (size, mtime-secs).
type FileMeta = Option<(u64, i64)>;

/// Concurrency for the directory walk. Metadata ops are latency-bound (not CPU),
/// so more threads than cores overlap the round-trips but a NAS serving its
/// own local disks gains nothing past a handful, and 64 idle-blocked threads
/// still cost stacks + scheduler churn on a 2-4 core box. `KROMA_WALK_THREADS`
/// overrides for genuinely remote mounts.
fn walk_threads() -> usize {
    if let Some(n) = std::env::var("KROMA_WALK_THREADS").ok().and_then(|s| s.parse().ok()) {
        return n;
    }
    let cores = std::thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(4);
    (cores * 4).clamp(8, 32)
}

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

fn has_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn has_video_extension_matches_known_containers_case_insensitively() {
        assert!(has_video_extension(Path::new("/movies/Film.mkv")));
        assert!(has_video_extension(Path::new("/movies/Film.MP4"))); // case-insensitive
        assert!(has_video_extension(Path::new("clip.MOV")));
        assert!(has_video_extension(Path::new("a.ts")));
        // Non-video and edge cases.
        assert!(!has_video_extension(Path::new("/movies/poster.jpg")));
        assert!(!has_video_extension(Path::new("/movies/subs.srt")));
        assert!(!has_video_extension(Path::new("/movies/noext")));
        assert!(!has_video_extension(Path::new("/movies/.hidden")));
    }

    #[test]
    fn is_pruned_dir_skips_synology_and_hidden() {
        assert!(is_pruned_dir(OsStr::new("@eaDir")));
        assert!(is_pruned_dir(OsStr::new(".git")));
        assert!(is_pruned_dir(OsStr::new(".DS_Store")));
        assert!(!is_pruned_dir(OsStr::new("Season 1")));
        assert!(!is_pruned_dir(OsStr::new("Movies")));
    }

    #[test]
    fn walk_threads_default_range_and_env_override() {
        // Default (no env): clamped into [8, 32].
        std::env::remove_var("KROMA_WALK_THREADS");
        let n = walk_threads();
        assert!((8..=32).contains(&n), "default {n} out of clamp range");

        // Explicit override is honored verbatim (not re-clamped).
        std::env::set_var("KROMA_WALK_THREADS", "3");
        assert_eq!(walk_threads(), 3);
        std::env::set_var("KROMA_WALK_THREADS", "100");
        assert_eq!(walk_threads(), 100);

        // A non-numeric value falls back to the computed default.
        std::env::set_var("KROMA_WALK_THREADS", "not-a-number");
        assert!((8..=32).contains(&walk_threads()));

        std::env::remove_var("KROMA_WALK_THREADS");
    }

    fn temp_scan_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kroma-scanroot-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_root_groups_movies_and_episodes_and_skips_noise() {
        let root = temp_scan_dir();
        // One movie (year in filename), a show with two episodes in its own folder,
        // a non-video file, and a hidden dir that must be pruned from the descent.
        std::fs::write(root.join("The Matrix (1999).mkv"), b"x").unwrap();
        let show_dir = root.join("Breaking Bad");
        std::fs::create_dir_all(&show_dir).unwrap();
        std::fs::write(show_dir.join("Breaking Bad S01E01.mkv"), b"x").unwrap();
        std::fs::write(show_dir.join("Breaking Bad S01E02.mkv"), b"x").unwrap();
        std::fs::write(root.join("poster.jpg"), b"x").unwrap();
        let hidden = root.join(".hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("Ghost (2000).mkv"), b"x").unwrap();

        let mut items: HashMap<String, MediaItem> = HashMap::new();
        let mut shows: HashMap<String, Show> = HashMap::new();
        let mut mtimes: HashMap<String, Option<i64>> = HashMap::new();
        let mut lib_item_ids = std::collections::HashSet::new();
        let mut movie_seen = false;
        let mut episode_seen = false;

        scan_root(
            "lib1",
            &root,
            &mut items,
            &mut shows,
            &mut mtimes,
            &mut lib_item_ids,
            &mut movie_seen,
            &mut episode_seen,
        );

        assert!(movie_seen);
        assert!(episode_seen);
        // 1 movie + 2 episodes = 3 logical items; jpg and hidden dir ignored.
        assert_eq!(items.len(), 3, "movie + 2 episodes, noise excluded");
        assert_eq!(shows.len(), 1, "both episodes grouped under one show");
        assert_eq!(lib_item_ids.len(), 3);
        assert_eq!(mtimes.len(), 3, "an mtime is recorded per scanned file");

        let movie = items.values().find(|i| i.kind == Kind::Movie).unwrap();
        assert_eq!(movie.title, "The Matrix");
        assert_eq!(movie.year, Some(1999));
        assert_eq!(movie.files.len(), 1);
        assert_eq!(movie.library, "lib1");

        let show = shows.values().next().unwrap();
        assert_eq!(show.title, "Breaking Bad");
        let episodes: Vec<&MediaItem> =
            items.values().filter(|i| i.kind == Kind::Episode).collect();
        assert_eq!(episodes.len(), 2);
        assert!(episodes.iter().all(|e| e.show_id.as_deref() == Some(show.id.as_str())));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_root_untitled_movie_gets_placeholder_title() {
        let root = temp_scan_dir();
        // A bare numeric/year-only name parses to an empty movie title, which
        // index_parsed replaces with "Untitled".
        std::fs::write(root.join("1999.mkv"), b"x").unwrap();

        let mut items: HashMap<String, MediaItem> = HashMap::new();
        let mut shows: HashMap<String, Show> = HashMap::new();
        let mut mtimes: HashMap<String, Option<i64>> = HashMap::new();
        let mut lib_item_ids = std::collections::HashSet::new();
        let mut movie_seen = false;
        let mut episode_seen = false;
        scan_root(
            "lib1",
            &root,
            &mut items,
            &mut shows,
            &mut mtimes,
            &mut lib_item_ids,
            &mut movie_seen,
            &mut episode_seen,
        );

        assert!(movie_seen);
        assert!(!episode_seen);
        assert_eq!(items.len(), 1);
        assert_eq!(items.values().next().unwrap().title, "Untitled");
        let _ = std::fs::remove_dir_all(&root);
    }
}
