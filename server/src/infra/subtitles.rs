//! Embedded TEXT subtitle → WebVTT extraction + disk cache.
//!
//! Extracting a text subtitle demuxes the WHOLE container (cues are interleaved
//! end-to-end, so ffmpeg must read the file front-to-back), which is slow over a
//! network mount - especially while the HLS remux competes for the same file. So
//! we do it ONCE per `(file, mtime, track)` and cache the WebVTT under
//! `<data>/subs/`, served instantly thereafter.
//!
//! [`extract_batch_blocking`] demuxes the file a SINGLE time and writes EVERY
//! requested track (one `-map 0:s:N` output each), so N tracks cost one whole-file
//! read, not N. The pipeline `subtitles` stage calls it to pre-warm the cache
//! before anyone hits play; the on-demand `/subtitles/:track` endpoint calls it on
//! a cache miss (warming every track off that one read).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::domain::media::SubtitleTrack;

/// Extraction wall-clock budget, scaled to the file: a text-subtitle demux is
/// bandwidth-bound (ffmpeg reads the container front-to-back), so budget the
/// size at a conservative 20 MB/s, clamped to 150s..900s. The old FIXED 150s
/// permanently starved big files: the pass timed out, the partial output was
/// discarded, and the next toggle started the whole read from zero again.
pub fn timeout_for(abs: &str) -> Duration {
    let size = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
    let secs = (size / (20 * 1024 * 1024)).clamp(150, 900);
    Duration::from_secs(secs)
}

/// Per-file extraction locks. Concurrent callers for the SAME file (a viewer's
/// toggle racing the playback pre-warm or the pipeline stage, two clients on
/// one film) serialize here; the losers then find the cache already written and
/// no-op instead of demuxing the whole file a second time in parallel.
fn file_lock(abs: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let map = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock().unwrap().entry(abs.to_string()).or_default().clone()
}

/// Extract every still-missing text track of `abs`, serialized per file: the
/// pending set is computed UNDER the lock, so whichever caller ran first has
/// already filled the cache and later callers are a cheap stat + no-op. This is
/// the one entry point the endpoint, the playback pre-warm and the pipeline
/// stage all share. Blocking; run it on a blocking thread.
pub fn extract_pending_locked(
    data_dir: &Path,
    abs: &str,
    subs: &[SubtitleTrack],
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    // Offline mount / moved file: one stat instead of an ffmpeg spawn per caller.
    if !Path::new(abs).exists() {
        return Err("media file unavailable (mount offline?)".to_string());
    }
    let lock = file_lock(abs);
    let _guard = lock.lock().map_err(|_| "subtitle extraction lock poisoned".to_string())?;
    let pending = pending_text_tracks(data_dir, abs, subs);
    extract_batch_blocking_cancellable(abs, &pending, cancel)
}

/// Distinct temp suffixes so two concurrent extractions of the SAME file never
/// clobber each other's `.part` output (the pid alone collides in-process). Mirrors
/// `infra::storyboard`'s `TMP_SEQ`.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Whether a (normalized) subtitle codec can be converted to WebVTT. Image subs
/// (PGS/VobSub/DVD) are bitmap and cannot be rendered as text, so they are skipped.
/// Mirrors the clients' `isTextSubtitle` against `probe::normalize_codec` output.
pub fn is_text_codec(codec: &str) -> bool {
    matches!(
        codec,
        "subrip" | "srt" | "ass" | "ssa" | "mov_text" | "webvtt" | "vtt"
    )
}

/// `<data>/subs/<hash>.vtt`, keyed by file path + mtime + track index so a replaced
/// file re-extracts and each track caches independently.
pub fn cache_path(data_dir: &Path, abs: &str, index: usize) -> PathBuf {
    let mtime = std::fs::metadata(abs)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // `short_hash` (sha256) is STABLE across std/toolchain versions, unlike
    // `DefaultHasher` whose seed can shift and silently orphan every cached VTT.
    // Mirrors `infra::storyboard`'s `key()`.
    let key = crate::services::scan::short_hash(&format!("{abs}:{mtime}:{index}"));
    data_dir.join("subs").join(format!("{key}.vtt"))
}

/// The text tracks of `subs` not yet cached, as `(0:s:<index>, cache_path)` pairs.
/// Image subs and already-extracted tracks are dropped, so an empty result means
/// "nothing to do".
pub fn pending_text_tracks(
    data_dir: &Path,
    abs: &str,
    subs: &[SubtitleTrack],
) -> Vec<(usize, PathBuf)> {
    subs.iter()
        .enumerate()
        .filter(|(_, s)| is_text_codec(&s.codec))
        .map(|(i, _)| (i, cache_path(data_dir, abs, i)))
        .filter(|(_, path)| !path.exists())
        .collect()
}

/// Delete every cached WebVTT for `abs`'s text tracks so a reprocess rebuilds them
/// from scratch (mirrors `storyboard::invalidate`). Best-effort.
pub fn invalidate(data_dir: &Path, abs: &str, subs: &[SubtitleTrack]) {
    for (i, s) in subs.iter().enumerate() {
        if is_text_codec(&s.codec) {
            let _ = std::fs::remove_file(cache_path(data_dir, abs, i));
        }
    }
}

/// Extract each requested text subtitle track to its cache file in ONE ffmpeg pass
/// (the file is demuxed once for all `tracks`). Each output is written to a temp
/// sibling (scoped by pid AND a per-call sequence, so two concurrent extractions of
/// the SAME file never collide on one `.part`) and atomically renamed on success,
/// so a concurrent reader never sees a half-written cue list. Blocking (runs on a
/// job thread); bounded by [`timeout_for`]. Aborts the in-flight ffmpeg the moment
/// `cancel` flips. `Ok(())` when nothing is pending or ffmpeg exits clean. Callers
/// normally go through [`extract_pending_locked`] for the per-file dedupe.
fn extract_batch_blocking_cancellable(
    abs: &str,
    tracks: &[(usize, PathBuf)],
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    if tracks.is_empty() {
        return Ok(());
    }
    if let Some(dir) = tracks[0].1.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create the subtitle cache dir: {e}"))?;
    }
    let pid = std::process::id();
    // A per-call sequence so two concurrent extractions of the same file write to
    // distinct temp files before the final rename (mirrors storyboard's `TMP_SEQ`).
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmps: Vec<PathBuf> = tracks
        .iter()
        .map(|(_, out)| out.with_extension(format!("{pid}.{seq}.part")))
        .collect();

    let mut cmd = Command::new("ffmpeg");
    // Text-subtitle decode is trivial; the cost is the demux read. One thread
    // keeps this from competing with a live remux for cores.
    cmd.args(["-v", "error", "-nostdin", "-threads", "1", "-y", "-i"]).arg(abs);
    for ((sidx, _), tmp) in tracks.iter().zip(&tmps) {
        cmd.arg("-map").arg(format!("0:s:{sidx}")).args(["-f", "webvtt"]).arg(tmp);
    }

    let outcome = run_capturing_cancellable(cmd, timeout_for(abs), cancel);

    // Move each non-empty output into place; clean up the rest either way so a
    // failed/partial pass never leaves temp files behind.
    let mut moved = 0usize;
    for ((_, out), tmp) in tracks.iter().zip(&tmps) {
        let ok = outcome.is_ok()
            && std::fs::metadata(tmp).map(|m| m.len() > 0).unwrap_or(false)
            && std::fs::rename(tmp, out).is_ok();
        if ok {
            moved += 1;
        } else {
            let _ = std::fs::remove_file(tmp);
        }
    }
    match outcome {
        Ok(()) if moved > 0 => Ok(()),
        // ffmpeg succeeded but produced nothing usable (e.g. every mapped track was
        // empty): not a hard error, just nothing to cache.
        Ok(()) => Ok(()),
        Err(reason) => Err(reason),
    }
}

/// Spawn `cmd`, wait up to `dur` (killing it on timeout), capture stderr for a
/// meaningful failure message. A sync stand-in for `tokio::time::timeout` (the
/// pipeline stage runs on a blocking thread). Mirrors `infra::storyboard`. Kept as
/// the stable non-cancellable entry point; delegates to the cancellable variant.
#[allow(dead_code)]
fn run_capturing(cmd: Command, dur: Duration) -> Result<(), String> {
    run_capturing_cancellable(cmd, dur, &|| false)
}

/// [`run_capturing`] that also polls `cancel` each tick, killing the child and
/// returning `Err("cancelled")` the moment it flips so a cancelled job/stage stops
/// the in-flight ffmpeg instead of waiting out `dur`.
fn run_capturing_cancellable(
    mut cmd: Command,
    dur: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    use std::io::Read;
    // Draw one slot from the process-wide ffmpeg budget (see `infra::ffmpeg_gate`)
    // so subtitle extraction shares the same cap as storyboard/marker work.
    let _permit = crate::infra::ffmpeg_gate::acquire();
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start ffmpeg (is it installed and on PATH?): {e}"))?;
    let drain = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });
    let start = Instant::now();
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
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => break Err(format!("waiting on ffmpeg failed: {e}")),
        }
    };
    let stderr = drain.and_then(|h| h.join().ok()).unwrap_or_default();
    match outcome {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let code = status.code().map_or_else(|| "killed by signal".to_string(), |c| format!("exit {c}"));
            let cleaned: String = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
            let n = cleaned.chars().count();
            let tail: String = cleaned.chars().skip(n.saturating_sub(300)).collect();
            Err(if tail.is_empty() { code } else { format!("{code} ({tail})") })
        }
        Err(reason) => Err(reason),
    }
}
