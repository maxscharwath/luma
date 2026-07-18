//! The cancellable, stderr-capturing ffmpeg runner shared by the media stages
//! (storyboard render + subtitle extraction). A sync stand-in for
//! `tokio::time::timeout`, since generation/extraction run on blocking threads.
//!
//! Every pass draws one slot from the process-wide [`crate::infra::ffmpeg_gate`]
//! budget so the tile fan-out, montage, jpeg and subtitle demux never
//! oversubscribe the box.

use std::process::Command;
use std::time::{Duration, Instant};

/// Spawn `cmd`, wait up to `dur` (killing it on timeout), and **capture stderr**
/// so a failure can explain itself. `Ok(())` on a clean exit; `Err(reason)` on a
/// spawn error (ffmpeg missing), a non-zero exit (with the stderr tail), or a
/// timeout.
pub(crate) fn run_capturing(cmd: Command, dur: Duration) -> Result<(), String> {
    run_capturing_cancellable(cmd, dur, &|| false)
}

/// [`run_capturing`] that also polls `cancel` each tick, killing the child and
/// returning `Err("cancelled")` the moment it flips so a cancelled job/stage stops
/// the in-flight ffmpeg (a tile seek can otherwise hang up to its timeout) instead
/// of waiting out `dur`.
pub(crate) fn run_capturing_cancellable(
    mut cmd: Command,
    dur: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    use std::io::Read;
    // One slot from the process-wide ffmpeg budget, held until this pass exits, so
    // the tile fan-out + montage + jpeg + subtitle demux never oversubscribe the
    // box (see `infra::ffmpeg_gate`). Acquired before spawn; released on return.
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
