//! Library scanning: walk media roots, parse names, group files into logical
//! items (Plex-style), and build the set of libraries/shows/items to persist.
//!
//! Phase 1 (this module's [`scan_all`]) is **fast**: it only `stat`s each video
//! file (size + mtime no read, no ffprobe). Files are grouped into logical
//! items so the library is browsable in seconds. The slow per-file probing runs
//! later in [`crate::infra::probe`]'s background pass.
//!
//! Split into the per-folder filesystem [`walk`] worker and the stable
//! logical-id / edition [`ids`] derivation; this module owns the orchestration
//! that aggregates them into the [`ScanData`] handed to [`crate::db::sync_all`].

mod ids;
pub mod walk;

use std::collections::HashMap;
use std::path::Path;

use tracing::{info, warn};

use crate::model::{Library, LibraryKind, MediaItem, Show};
use crate::services::settings::LibraryDef;

use walk::scan_root;

pub use ids::{movie_logical_id, short_hash};

/// Everything a phase-1 scan produces, ready to hand to [`crate::db::sync_all`].
#[derive(Debug, Default)]
pub struct ScanData {
    pub libraries: Vec<Library>,
    pub shows: Vec<Show>,
    pub items: Vec<MediaItem>,
    /// `file_id -> mtime-secs` for every scanned file. Carried here (rather than
    /// on `MediaFile`, which is the client JSON contract) so the DB sync can
    /// detect changed files. Owned by this scan no shared global, so two
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

/// Current time as an RFC3339 / ISO8601 string (UTC). Re-exported from luma-primitives.
pub use luma_primitives::now_iso8601;

/// Phase-1 rescan + DB sync, demo-seeding only in demo mode (no libraries
/// configured). Pure work (no events / no background spawns) so both the `POST
/// /api/scan` handler and the `library.scan` job can share it and add their own
/// notifications. Blocking (walk + SQLite) call from a blocking context.
pub fn rescan_sync(state: &crate::state::SharedState) -> anyhow::Result<ScanData> {
    let defs = crate::services::settings::library_defs(&state.settings, &state.config);
    let mut data = scan_all(&defs);
    // Seed demo content only when nothing is configured (true demo mode). A
    // configured library that momentarily reads empty NAS/SMB unmount, slow
    // mount, permission glitch must NOT be clobbered with demo movies.
    if data.items.is_empty() && defs.is_empty() {
        info!("no libraries configured and scan is empty; seeding demo content");
        data = crate::services::demo::demo_data();
    }
    crate::db::sync_all(&state.db, &data.libraries, &data.shows, &data.items, &data.mtimes)?;
    Ok(data)
}

/// Publish "scan started", run phase-1 [`rescan_sync`], then announce the
/// catalog change with the resulting counts. Blocking; returns the synced data.
/// Shared by `POST /api/scan` and the `library.scan` job each wraps it with
/// its own logging / response.
pub fn scan_and_publish(state: &crate::state::SharedState) -> anyhow::Result<ScanData> {
    use crate::infra::events::ServerEvent;
    state.events.publish(ServerEvent::ScanStarted);
    crate::services::activity::scan_started(&state.activity);

    let data = rescan_sync(state)?;
    let (libraries, shows, items) = (data.libraries.len(), data.shows.len(), data.items.len());
    crate::services::activity::scan_completed(&state.activity, libraries, shows, items, now_iso8601());
    state.events.publish(ServerEvent::ScanCompleted { items, shows, libraries });
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(data)
}

/// Kick the phase-2 background follow-ups after a scan media probing, search
/// reindex and TMDB enrichment (each reports its own progress in the activity
/// feed). Shared by `POST /api/scan` and the `library.scan` job.
pub fn spawn_follow_ups(state: &crate::state::SharedState, data: &ScanData) {
    crate::infra::probe::spawn_probe_pass(
        state.db.clone(),
        state.ffprobe_available,
        state.events.clone(),
        state.activity.clone(),
    );
    crate::services::search::spawn_reindex(state.clone());
    crate::services::enrich::maybe_spawn(state, &data.items, &data.shows);
}
