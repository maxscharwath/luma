//! Tile extraction: the parallel keyframe-seek workers that grab each thumbnail,
//! the black-tile gap fill, and the one-time hardware-decode probe that decides
//! whether `-hwaccel auto` is worth it on this box.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing::info;

use super::proc::{run_capturing, run_capturing_cancellable, Cancel, TMP_SEQ};
use super::{Plan, TILE_H, TILE_W};

/// Wall-clock ceiling for a SINGLE tile's keyframe seek (a stalled mount is
/// killed, not hung on). The happy path is well under a second.
const TILE_TIMEOUT: Duration = Duration::from_secs(120);
/// Upper bound on the per-item tile thread pool. Enough to hide seek/decode
/// latency on real storage without one item monopolising the box (the outer
/// [`MAX_CONCURRENT`] still bounds how many items generate at once).
const MAX_TILE_WORKERS: usize = 8;

/// How many tiles to extract in parallel: one per core, clamped to a sane band.
/// Seeks are IO- and decode-bound, so a handful of workers hides the latency;
/// past that, extra processes only thrash shared storage.
fn tile_workers() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .clamp(2, MAX_TILE_WORKERS)
}

/// hw must beat sw by this margin to be worth it (avoids flapping on noise).
const HWACCEL_MARGIN: f64 = 0.9;
/// Machine-wide, process-lifetime decode decision: `true` = pass `-hwaccel auto`.
static HWACCEL: OnceLock<bool> = OnceLock::new();

/// Whether `-hwaccel auto` is worth it here, decided ONCE (per process) by a real
/// head-to-head probe and then cached. Hardware decode offloads the CPU, but each
/// tile is its own ffmpeg process, so the per-process device init can make it a
/// net loss for single-frame seeks on a fast CPU while a clear win on a weak NAS
/// with an iGPU it depends on the box, so we measure rather than guess.
pub(super) fn use_hwaccel(abs: &str, dur_s: f64) -> bool {
    *HWACCEL.get_or_init(|| probe_hwaccel(abs, dur_s))
}

/// Time software vs `-hwaccel auto` decoding the SAME warmed mid-film keyframe
/// (so the comparison is pure decode path, IO neutralised) and pick hardware only
/// if it is clearly faster. `-hwaccel auto` itself falls back to software per
/// stream, so choosing it is always safe; this only decides whether it is faster.
fn probe_hwaccel(abs: &str, dur_s: f64) -> bool {
    // No GPU device node = `-hwaccel auto` can only ever fall back to software,
    // so the head-to-head (4 extra full keyframe decodes, ~10s on a weak NAS)
    // would be pure startup waste. Typical GPU-less Synology boxes land here.
    #[cfg(target_os = "linux")]
    if !Path::new("/dev/dri").exists() {
        info!("storyboard decode path: software (no /dev/dri)");
        return false;
    }
    let t = (dur_s * 0.5) as u32;
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let probe = std::env::temp_dir().join(format!("sb-probe-{}-{seq}.png", std::process::id()));
    // Warm the GOP into cache (and confirm software decode works at all). This is a
    // one-time, process-lifetime probe, so it is not itself cancellable.
    if extract_one(abs, t, &probe, false, &|| false).is_err() {
        return false;
    }
    // Interleave sw/hw and take the best of two, so scheduling/thermal noise and
    // any residual warming don't systematically favour one path.
    let timed = |hw: bool| -> Option<Duration> {
        let s = Instant::now();
        extract_one(abs, t, &probe, hw, &|| false).ok().map(|()| s.elapsed())
    };
    let (sw1, hw1, sw2, hw2) = (timed(false), timed(true), timed(false), timed(true));
    let _ = std::fs::remove_file(&probe);
    let sw = [sw1, sw2].into_iter().flatten().min();
    let hw = [hw1, hw2].into_iter().flatten().min();
    let decision = matches!((sw, hw), (Some(sw), Some(hw)) if hw.as_secs_f64() < sw.as_secs_f64() * HWACCEL_MARGIN);
    info!(
        hwaccel = decision,
        sw = ?sw,
        hw = ?hw,
        "storyboard decode path: {}",
        if decision { "hardware (-hwaccel auto)" } else { "software" },
    );
    decision
}

/// Grab each tile with a fast keyframe seek, [`tile_workers`] in parallel, writing
/// `px_<NNNN>.png` into `scratch`. A single failed seek is tolerated (it becomes
/// black padding via [`fill_gaps`]); an all-empty result (ffmpeg missing, file
/// unreadable) is a hard error carrying the first captured cause.
pub(super) fn extract_tiles(abs: &str, scratch: &Path, plan: &Plan, hwaccel: bool, cancel: Cancel) -> std::result::Result<(), String> {
    let next = AtomicU32::new(0);
    let first_err: Mutex<Option<String>> = Mutex::new(None);
    let count = plan.count;
    let interval = plan.interval;

    // A scoped pool: each worker pulls the next index until the range is drained.
    // `scope` joins every thread before returning, so all tiles are on disk after.
    std::thread::scope(|s| {
        for _ in 0..tile_workers() {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= count || cancel() {
                    break;
                }
                let out = scratch.join(format!("px_{i:04}.png"));
                if let Err(e) = extract_one(abs, i * interval, &out, hwaccel, cancel) {
                    let mut g = first_err.lock().unwrap();
                    if g.is_none() {
                        *g = Some(e);
                    }
                }
            });
        }
    });

    if fill_gaps(scratch, count) == 0 {
        return Err(first_err
            .into_inner()
            .unwrap()
            .unwrap_or_else(|| "no tiles produced".to_string()));
    }
    Ok(())
}

/// One tile: fast input seek (`-ss` before `-i` jumps to the GOP at `t_secs`
/// without decoding up to it) and grab a single keyframe, scaled+cropped to an
/// exact, letterbox-free 160x90. With `hwaccel`, `-hwaccel auto` offloads decode
/// to the GPU (it falls back to software per stream, so it is always safe).
/// `Err` carries ffmpeg's captured cause.
fn extract_one(abs: &str, t_secs: u32, out: &Path, hwaccel: bool, cancel: Cancel) -> std::result::Result<(), String> {
    // `increase,crop` (the proven pattern in infra::image) fills the tile with no
    // letterbox and an exact, even output size.
    let vf = format!(
        "scale={TILE_W}:{TILE_H}:force_original_aspect_ratio=increase,crop={TILE_W}:{TILE_H}"
    );
    let mut cmd = Command::new("ffmpeg");
    // `-threads 2` (input option): tile workers already run one process per core,
    // so per-process decoder thread pools only multiply into oversubscription.
    cmd.args(["-v", "error", "-nostdin", "-threads", "2"]);
    if hwaccel {
        // Input option: must precede `-i`. `nokey` + hwaccel can be incompatible on
        // some devices, so let the decoder emit every frame and just take the first.
        cmd.args(["-hwaccel", "auto"]);
    } else {
        cmd.args(["-skip_frame", "nokey"]);
    }
    cmd.args(["-ss", &t_secs.to_string(), "-noaccurate_seek", "-i"])
        .arg(abs)
        .args(["-an", "-sn", "-dn", "-frames:v", "1", "-vf", &vf, "-y"])
        .arg(out);
    run_capturing_cancellable(cmd, TILE_TIMEOUT, cancel)?;
    if out.exists() {
        Ok(())
    } else {
        Err("ffmpeg reported success but produced no frame".to_string())
    }
}

/// Backfill any missing `px_<NNNN>.png` (a failed seek) with a black tile so the
/// image2 montage never truncates at a gap (trailing gaps the `tile` filter pads
/// on its own; only interior holes need this). Returns how many REAL tiles landed
/// so an all-empty extraction can be reported as a hard failure.
fn fill_gaps(scratch: &Path, count: u32) -> u32 {
    let missing: Vec<u32> = (0..count)
        .filter(|i| !scratch.join(format!("px_{i:04}.png")).exists())
        .collect();
    let present = count - missing.len() as u32;
    if present == 0 || missing.is_empty() {
        return present;
    }
    let black = scratch.join("black.png");
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i",
        &format!("color=c=black:s={TILE_W}x{TILE_H}"), "-frames:v", "1",
    ])
    .arg(&black);
    if run_capturing(cmd, TILE_TIMEOUT).is_ok() && black.exists() {
        for i in missing {
            let _ = std::fs::copy(&black, scratch.join(format!("px_{i:04}.png")));
        }
    }
    present
}
