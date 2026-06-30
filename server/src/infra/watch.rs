//! Library watcher keeps the catalog in sync with the filesystem.
//!
//! Two mechanisms feed one debounced worker thread:
//!   * a **periodic re-scan** (`LUMA_WATCH_INTERVAL` seconds, default 300, `0`
//!     disables) the reliable path for **network mounts** (SMB/NFS), where the
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
use tokio::runtime::Handle;
use tracing::{info, warn};

use crate::model::MediaItem;
use crate::services::jobs::{Trigger, TriggerError};
use crate::services::scan;
use crate::state::SharedState;

const DEFAULT_INTERVAL_SECS: u64 = 300;
const DEBOUNCE: Duration = Duration::from_secs(2);

/// Start watching the configured media dirs. `baseline` is the signature of the
/// startup scan (already persisted), so we don't re-emit until something changes.
pub fn spawn(state: SharedState, baseline: u64) {
    if state.config.media_dirs.is_empty() {
        return; // demo mode nothing on disk to watch
    }
    // Captured from the (tokio) main thread so the watcher's std::thread can hand
    // re-scans to the tracked job manager, which spawns work onto the runtime.
    let handle = Handle::current();

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<()>();
        let watcher = make_watcher(&state, tx); // kept alive for the thread's life
        info!(fs_events = watcher.is_some(), "library watcher started");

        let mut last = baseline;
        loop {
            // Re-read the cadence each iteration so an admin change takes effect
            // without a restart. `0` → block on FS events only (no periodic tick).
            let interval = watch_interval(&state);
            let recv = if interval > 0 {
                rx.recv_timeout(Duration::from_secs(interval))
            } else {
                rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            let from_event = match recv {
                Ok(()) => true,
                Err(RecvTimeoutError::Timeout) => false, // periodic tick
                Err(RecvTimeoutError::Disconnected) => break,
            };
            // Coalesce a burst of FS events into a single re-scan.
            if from_event {
                while rx.recv_timeout(DEBOUNCE).is_ok() {}
            }
            trigger_if_changed(&state, &handle, &mut last);
        }
        drop(watcher);
    });
}

/// The periodic re-scan cadence (seconds): the `watchIntervalSecs` setting, or
/// when it's `-1` (unset) the `LUMA_WATCH_INTERVAL` env / 300s default. `0`
/// disables the periodic tick (FS events still fire).
fn watch_interval(state: &SharedState) -> u64 {
    match state.settings.get_i64("watchIntervalSecs", -1) {
        n if n >= 0 => n as u64,
        _ => std::env::var("LUMA_WATCH_INTERVAL")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_SECS),
    }
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

/// Cheap change-detection (fast phase-1 scan + signature compare); on an actual
/// change, hand off to the **tracked** jobs that opted into [`Trigger::LibraryChange`]
/// (i.e. `library.scan`) so an auto-scan shows in the Tâches console with logs +
/// progress, unified with manual scans, and the full sync / probe / enrich /
/// reindex lives in one place. An idle library triggers nothing (no DB writes, no
/// client churn).
fn trigger_if_changed(state: &SharedState, handle: &Handle, last: &mut u64) {
    if !state.settings.get_bool("watchAutoScan", true) {
        return; // auto-scan disabled by the admin
    }
    let defs = crate::services::settings::library_defs(&state.settings, &state.config);
    let data = scan::scan_all(&defs);
    let sig = signature(&data.items, &data.mtimes);
    if sig == *last {
        return; // nothing changed
    }
    info!("watcher: filesystem change detected triggering library-change jobs");

    // The job manager spawns blocking work, so run the trigger on the runtime.
    // Only advance `last` once the change is actually owned by a run (or nothing
    // opted in): if a scan is already in flight the trigger returns
    // `AlreadyRunning`, and advancing would consume this change without syncing it
    // (the running scan may predate the new file) so we keep `last` and retry
    // next tick instead of losing it.
    let jobs = state.jobs.clone();
    let st = state.clone();
    let owned = handle.block_on(async move {
        let mut started = false;
        let mut busy = false;
        for id in jobs.jobs_for_trigger(Trigger::LibraryChange) {
            match jobs.trigger(st.clone(), id, "watch") {
                Ok(_) => started = true,
                Err(TriggerError::AlreadyRunning) => busy = true,
                Err(TriggerError::Unknown) => {}
            }
        }
        // Advance unless the sole outcome was "already running" (nothing started).
        started || !busy
    });
    if owned {
        *last = sig;
    }
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
