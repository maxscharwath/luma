//! Invoke the `ffprobe` CLI: availability check, the phase-2 background probing
//! pass, the per-file run, and the extension-guess fallback.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use tracing::{debug, info, warn};

use crate::db::{self, Pool};
use crate::infra::events::{Bus, ServerEvent};
use crate::model::VideoStream;
use crate::services::activity::{self, Shared as Activity};

use super::parse::build_result;
use super::ProbeResult;

/// Max concurrent ffprobe processes in the phase-2 background pass: half the
/// cores, clamped to 2..4. Each ffprobe is a real process doing header reads +
/// a few frame decodes; ten at once starved interactive work on a 4-core NAS.
/// `KROMA_PROBE_WORKERS` overrides (e.g. bump it on a big box / remote mount).
fn probe_workers() -> usize {
    if let Some(n) = std::env::var("KROMA_PROBE_WORKERS").ok().and_then(|s| s.parse().ok()) {
        return n;
    }
    let cores = std::thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(4);
    (cores / 2).clamp(2, 4)
}

/// Detect whether `ffprobe` is callable. Done once at startup.
pub fn ffprobe_available() -> bool {
    Command::new("ffprobe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Probe a single file. Returns a best-effort [`ProbeResult`]; on any failure
/// it falls back to a container-extension guess for the video codec.
pub fn probe_file(path: &Path, ffprobe_present: bool) -> ProbeResult {
    if ffprobe_present {
        if let Some(result) = run_ffprobe(path) {
            return result;
        }
    }
    fallback_from_extension(path)
}

/// One file awaiting a probe.
struct ProbeJob {
    file_id: String,
    abs_path: String,
    item_id: String,
}

/// Spawn the background phase-2 probing pass: ffprobe every file with `probed=0`,
/// write the result, and emit live events so clients fill in codec/HDR badges.
///
/// Returns immediately; work runs on a small pool of detached threads, mirroring
/// [`crate::services::enrich`]. A no-op when there are no unprobed files.
pub fn spawn_probe_pass(pool: Pool, ffprobe_present: bool, bus: Bus, activity: Activity) {
    let unprobed = match db::unprobed_files(&pool) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to list unprobed files; skipping probe pass");
            return;
        }
    };
    if unprobed.is_empty() {
        info!("phase-2 probe: nothing to probe (mtime cache hit)");
        return;
    }

    let total = unprobed.len();
    info!(files = total, "starting phase-2 background probing");
    activity::probe_started(&activity, total);

    let jobs: Vec<ProbeJob> = unprobed
        .into_iter()
        .map(|(file_id, abs_path, item_id)| ProbeJob { file_id, abs_path, item_id })
        .collect();
    let queue = Arc::new(Mutex::new(jobs));
    let done = Arc::new(AtomicUsize::new(0));

    thread::spawn(move || {
        let worker_count = probe_workers().min(total.max(1));
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let pool = pool.clone();
            let queue = queue.clone();
            let done = done.clone();
            let bus = bus.clone();
            let activity = activity.clone();
            handles.push(thread::spawn(move || loop {
                let job = match queue.lock().unwrap().pop() {
                    Some(j) => j,
                    None => break,
                };
                if let Err(e) =
                    probe_one(&pool, ffprobe_present, &bus, &job.file_id, &job.abs_path, &job.item_id)
                {
                    warn!(file = %job.file_id, error = %e, "failed to store probe result");
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                activity::probe_progress(&activity, n);
                if n.is_multiple_of(25) {
                    bus.publish(ServerEvent::ProbeProgress { done: n, total });
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let done = done.load(Ordering::Relaxed);
        activity::probe_completed(&activity);
        info!(probed = done, total, "phase-2 probing complete");
        bus.publish(ServerEvent::ProbeProgress { done, total });
        bus.publish(ServerEvent::ProbeCompleted { total });
        bus.publish(ServerEvent::LibraryUpdated);
    });
}

/// Probe one file and persist it: run ffprobe, store the stream columns (+
/// `probed=1`), derive intro/credits markers from any embedded chapters, and emit
/// `ItemUpdated` when this is the item's first probed file. Shared by the
/// background probe pass and the `pipeline.probe` stage.
pub fn probe_one(
    pool: &Pool,
    ffprobe: bool,
    bus: &Bus,
    file_id: &str,
    abs_path: &str,
    item_id: &str,
) -> anyhow::Result<()> {
    // Whether this is the item's first probe → emit ItemUpdated so the client
    // shows the codec badge appear.
    let first_for_item = db::item_has_probed_file(pool, item_id).map(|has| !has).unwrap_or(true);
    let result = probe_file(Path::new(abs_path), ffprobe);
    db::set_file_probe(
        pool,
        file_id,
        result.duration_ms,
        result.video.as_ref(),
        result.audio.as_ref(),
        &result.audio_tracks,
        &result.subtitles,
    )?;
    // Intro/credits markers from embedded chapters (free since we already probed).
    for (kind, start, end) in super::markers_from_chapters(&result.chapters, result.duration_ms) {
        let _ = db::set_marker(pool, item_id, kind, start, end, "chapters");
    }
    if first_for_item {
        bus.publish(ServerEvent::ItemUpdated { id: item_id.to_string() });
    }
    Ok(())
}

/// Run ffprobe and parse its JSON. Returns `None` (→ extension-guess fallback)
/// if anything goes wrong, logging the cause at DEBUG. It's DEBUG, not WARN,
/// because 10 workers probing every file would flood the default log; a wholesale
/// degradation still shows up as `probed` ≪ `total` in the phase-2 summary, and
/// `RUST_LOG=kroma_server=debug` surfaces the per-file detail. `-v error` (vs the
/// old `-v quiet`) lets ffmpeg's own diagnostic reach stderr.
fn run_ffprobe(path: &Path) -> Option<ProbeResult> {
    let output = match Command::new("ffprobe")
        .args([
            "-v", "error", "-show_format", "-show_streams", "-show_chapters", "-of", "json",
        ])
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            debug!(file = %path.display(), error = %e, "ffprobe failed to spawn; using extension guess");
            return None;
        }
    };

    if !output.status.success() {
        debug!(
            file = %path.display(),
            code = output.status.code().unwrap_or(-1),
            detail = %String::from_utf8_lossy(&output.stderr).trim(),
            "ffprobe errored; using extension guess",
        );
        return None;
    }

    match serde_json::from_slice(&output.stdout) {
        Ok(parsed) => Some(build_result(parsed)),
        Err(e) => {
            debug!(file = %path.display(), error = %e, "failed to parse ffprobe JSON; using extension guess");
            None
        }
    }
}

/// When ffprobe is unavailable, guess the video codec from the file extension.
fn fallback_from_extension(path: &Path) -> ProbeResult {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let codec = match ext.as_str() {
        "webm" => "vp9",
        "avi" => "mpeg4",
        // Modern containers commonly carry h264; leave it as a soft guess.
        "mp4" | "m4v" | "mov" | "mkv" | "ts" => "h264",
        _ => "unknown",
    };

    ProbeResult {
        duration_ms: None,
        video: Some(VideoStream {
            codec: codec.to_string(),
            width: None,
            height: None,
            hdr: false,
            bit_depth: None,
        }),
        audio: None,
        audio_tracks: Vec::new(),
        subtitles: Vec::new(),
        chapters: Vec::new(),
    }
}
