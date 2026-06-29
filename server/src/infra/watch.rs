//! Library watcher — keeps the catalog in sync with the filesystem.
//!
//! Two mechanisms feed one debounced worker thread:
//!   * a **periodic re-scan** (`LUMA_WATCH_INTERVAL` seconds, default 300, `0`
//!     disables) — the reliable path for **network mounts** (SMB/NFS), where the
//!     OS delivers no change events for edits made on the NAS itself;
//!   * a best-effort **`notify` filesystem watcher** for instant pickup of local
//!     changes (e.g. when the server runs on the NAS / a local disk).
//!
//! A re-scan re-applies + notifies clients only when the file set actually
//! changed (cheap signature compare), so an idle library causes zero churn. The
//! phase-1 scan is now parallel/fast, so periodic re-scans are cheap.

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tracing::{info, warn};

use crate::infra::events::ServerEvent;
use crate::model::MediaItem;
use crate::state::SharedState;
use crate::db;
use crate::infra::probe;
use crate::services::{activity, enrich, scan};

const DEFAULT_INTERVAL_SECS: u64 = 300;
const DEBOUNCE: Duration = Duration::from_secs(2);

/// Start watching the configured media dirs. `baseline` is the signature of the
/// startup scan (already persisted), so we don't re-emit until something changes.
pub fn spawn(state: SharedState, baseline: u64) {
    if state.config.media_dirs.is_empty() {
        return; // demo mode — nothing on disk to watch
    }
    let interval = std::env::var("LUMA_WATCH_INTERVAL")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS);

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<()>();
        let watcher = make_watcher(&state, tx); // kept alive for the thread's life
        let poll = (interval > 0).then(|| Duration::from_secs(interval));
        info!(
            interval_secs = interval,
            fs_events = watcher.is_some(),
            "library watcher started"
        );

        let mut last = baseline;
        loop {
            let from_event = match poll {
                Some(d) => match rx.recv_timeout(d) {
                    Ok(()) => true,
                    Err(RecvTimeoutError::Timeout) => false, // periodic tick
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(()) => true,
                    Err(_) => break,
                },
            };
            // Coalesce a burst of FS events into a single re-scan.
            if from_event {
                while rx.recv_timeout(DEBOUNCE).is_ok() {}
            }
            rescan_if_changed(&state, &mut last);
        }
        drop(watcher);
    });
}

fn make_watcher(state: &SharedState, tx: mpsc::Sender<()>) -> Option<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .map_err(|e| warn!(error = %e, "filesystem watcher unavailable (periodic re-scan only)"))
    .ok()?;

    for dir in crate::services::settings::all_folders(&state.settings, &state.config) {
        if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
            warn!(path = %dir.display(), error = %e, "could not watch media dir");
        }
    }
    Some(watcher)
}

/// Re-scan (fast, phase-1). If the file set changed since `last`, persist the
/// diff, notify clients, and kick off probing/enrichment of the new files.
fn rescan_if_changed(state: &SharedState, last: &mut u64) {
    let defs = crate::services::settings::library_defs(&state.settings, &state.config);
    let data = scan::scan_all(&defs);
    let sig = signature(&data.items, &data.mtimes);
    if sig == *last {
        return; // nothing changed — no DB writes, no client churn
    }
    *last = sig;
    info!("watcher: filesystem change detected — re-syncing");

    state.events.publish(ServerEvent::ScanStarted);
    activity::scan_started(&state.activity);
    if let Err(e) = db::sync_all(&state.db, &data.libraries, &data.shows, &data.items, &data.mtimes) {
        warn!(error = %e, "watcher: library sync failed");
        return;
    }
    let (libraries, shows, items) = (data.libraries.len(), data.shows.len(), data.items.len());
    activity::scan_completed(&state.activity, libraries, shows, items, scan::now_iso8601());
    state.events.publish(ServerEvent::ScanCompleted { items, shows, libraries });
    state.events.publish(ServerEvent::LibraryUpdated);

    // Probe only the new/changed files (sync marked them probed=0); enrichment is
    // cache-deduped so existing titles are cheap.
    probe::spawn_probe_pass(
        state.db.clone(),
        state.ffprobe_available,
        state.events.clone(),
        state.activity.clone(),
    );
    crate::services::search::spawn_reindex(state.clone());
    enrich::maybe_spawn(state, &data.items, &data.shows);
}

/// Cheap change-detection signature: file count + Σsize + Σmtime across all files.
pub fn signature(items: &[MediaItem], mtimes: &HashMap<String, Option<i64>>) -> u64 {
    let mut count: u64 = 0;
    let mut acc: u64 = 0;
    for item in items {
        for f in &item.files {
            count = count.wrapping_add(1);
            acc = acc.wrapping_add(f.size.unwrap_or(0));
            if let Some(Some(m)) = mtimes.get(&f.id) {
                acc = acc.wrapping_add(*m as u64);
            }
        }
    }
    count.wrapping_mul(1_000_003).wrapping_add(acc)
}
