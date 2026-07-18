//! Low-level process plumbing shared by the storyboard render stages: the
//! cancellable, stderr-capturing ffmpeg runner plus the atomic temp-file helpers,
//! and the shared cancel-poll alias / temp-name sequence they build on.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A cheap "should I stop?" poll threaded through the ffmpeg passes so cancelling
/// the job interrupts the current pass at the next tick. `Sync` so the scoped tile
/// workers can all share the one closure.
pub(super) type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);

/// Distinct temp suffixes so two concurrent writers never clobber each other.
pub(super) static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Spawn `cmd`, wait up to `dur` (killing it on timeout), and capture stderr so a
/// failure can explain itself. A thin wrapper over the shared
/// [`crate::infra::ffmpeg_run`] runner (also used by subtitle extraction).
pub(super) fn run_capturing(cmd: Command, dur: Duration) -> std::result::Result<(), String> {
    crate::infra::ffmpeg_run::run_capturing(cmd, dur)
}

/// [`run_capturing`] that also polls `cancel` each tick, killing the in-flight
/// ffmpeg the moment it flips so a cancelled job/stage stops (a tile seek can
/// otherwise hang up to `TILE_TIMEOUT`) instead of waiting out `dur`.
pub(super) fn run_capturing_cancellable(
    cmd: Command,
    dur: Duration,
    cancel: Cancel,
) -> std::result::Result<(), String> {
    crate::infra::ffmpeg_run::run_capturing_cancellable(cmd, dur, cancel)
}

/// A unique sibling temp path for `out` that KEEPS `out`'s extension last (so
/// ffmpeg/cwebp still infer the format) and can't collide with a concurrent writer.
pub(super) fn unique_tmp(out: &Path) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let base = out.file_name().and_then(|n| n.to_str()).unwrap_or("sheet");
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    out.with_file_name(format!("{base}.{}.{seq}.tmp.{ext}", std::process::id()))
}

/// Atomically move a freshly-written temp onto its served path (cleaning up on
/// failure). Returns whether it landed.
pub(super) fn finalize(tmp: &Path, out: &Path) -> bool {
    match std::fs::rename(tmp, out) {
        Ok(()) => true,
        Err(_) => {
            let _ = std::fs::remove_file(tmp);
            false
        }
    }
}
