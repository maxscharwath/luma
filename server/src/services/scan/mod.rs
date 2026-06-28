//! Library scanning: walk media roots, parse names, group files into logical
//! items (Plex-style), and build the set of libraries/shows/items to persist.
//!
//! Phase 1 (this module's [`scan_all`]) is **fast**: it only `stat`s each video
//! file (size + mtime — no read, no ffprobe). Files are grouped into logical
//! items so the library is browsable in seconds. The slow per-file probing runs
//! later in [`crate::infra::probe`]'s background pass.
//!
//! Split into the per-folder filesystem [`walk`] worker and the stable
//! logical-id / edition [`ids`] derivation; this module owns the orchestration
//! that aggregates them into the [`ScanData`] handed to [`crate::db::sync_all`].

mod ids;
mod walk;

use std::collections::HashMap;
use std::path::Path;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::{info, warn};

use crate::model::{Library, LibraryKind, MediaItem, Show};
use crate::services::settings::LibraryDef;

use walk::scan_root;

pub use ids::short_hash;

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

/// Current time as an RFC3339 / ISO8601 string (UTC).
pub fn now_iso8601() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
