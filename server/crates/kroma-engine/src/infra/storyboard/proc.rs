//! Low-level process plumbing shared by the storyboard render stages: the
//! cancellable, stderr-capturing ffmpeg runner plus the atomic temp-file helpers,
//! and the shared cancel-poll alias / temp-name sequence they build on.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A cheap "should I stop?" poll threaded through the ffmpeg passes so cancelling
/// the job interrupts the current pass at the next tick. `Sync` so the scoped tile
/// workers can all share the one closure.
pub(super) type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);

/// Distinct temp suffixes so two concurrent writers never clobber each other.
pub(super) static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Spawn `cmd`, wait up to `dur` (killing it on timeout), and **capture stderr**
/// so a failure can explain itself the previous version discarded stderr, which
/// is exactly why a broken pass left no trace. `Ok(())` on a clean exit;
/// `Err(reason)` on a spawn error (ffmpeg missing), a non-zero exit (with the
/// stderr tail), or a timeout. A sync stand-in for `tokio::time::timeout`
/// (generation runs on a blocking thread).
pub(super) fn run_capturing(cmd: Command, dur: Duration) -> std::result::Result<(), String> {
    run_capturing_cancellable(cmd, dur, &|| false)
}

/// [`run_capturing`] that also polls `cancel` each tick, killing the child and
/// returning `Err("cancelled")` the moment it flips so a cancelled job/stage stops
/// the in-flight ffmpeg (a tile seek can otherwise hang up to `TILE_TIMEOUT`)
/// instead of waiting out `dur`.
pub(super) fn run_capturing_cancellable(
    mut cmd: Command,
    dur: Duration,
    cancel: Cancel,
) -> std::result::Result<(), String> {
    use std::io::Read;
    // One slot from the process-wide ffmpeg budget, held until this pass exits, so
    // the tile fan-out + montage + jpeg never oversubscribe the box (see
    // `infra::ffmpeg_gate`). Acquired before spawn; released on return.
    let _permit = crate::infra::ffmpeg_gate::acquire();
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start ffmpeg (is it installed and on PATH?): {e}"))?;
    // Drain stderr on a side thread so a chatty child can never deadlock on a full
    // pipe buffer while we poll for the timeout.
    let drain = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });
    let start = Instant::now();
    // Poll with exponential backoff: a sub-second tile seek is noticed in a few
    // ms (a fixed 200 ms sleep would waste up to that per tile, and there are
    // hundreds), while a long montage never busy-spins.
    let mut backoff = Duration::from_millis(2);
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if cancel() {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("cancelled".to_string());
                }
                if start.elapsed() >= dur {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("timed out after {}s", dur.as_secs()));
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(100));
            }
            Err(e) => break Err(format!("waiting on ffmpeg failed: {e}")),
        }
    };
    let stderr = drain.and_then(|h| h.join().ok()).unwrap_or_default();
    match outcome {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let code = status.code().map_or_else(|| "killed by signal".to_string(), |c| format!("exit {c}"));
            Err(format!("{code}{}", stderr_tail(&stderr)))
        }
        Err(reason) => Err(reason),
    }
}

/// A compact, single-line tail of captured stderr for an error/log line (so a
/// failure shows ffmpeg's own message without flooding the log).
fn stderr_tail(stderr: &str) -> String {
    let cleaned: String = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return String::new();
    }
    let n = cleaned.chars().count();
    let tail: String = cleaned.chars().skip(n.saturating_sub(300)).collect();
    format!(" ({tail})")
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
