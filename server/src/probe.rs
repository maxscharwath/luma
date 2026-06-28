//! Metadata extraction via the `ffprobe` CLI.
//!
//! We never transcode. ffprobe is invoked purely to read stream metadata. If it
//! is missing or fails on a given file we degrade gracefully: codec is inferred
//! from the container extension and unknown fields are left null.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::activity::{self, Shared as Activity};
use crate::db::{self, Pool};
use crate::events::{Bus, ServerEvent};
use crate::model::{AudioStream, SubtitleTrack, VideoStream};

/// Max concurrent ffprobe processes in the phase-2 background pass. Tuned to
/// saturate an SMB mount without thrashing it.
const PROBE_WORKERS: usize = 10;

/// Result of probing a file. All fields are best-effort.
#[derive(Debug, Default)]
pub struct ProbeResult {
    pub duration_ms: Option<u64>,
    pub video: Option<VideoStream>,
    /// Representative (first) audio track, for badges. `audio_tracks.first()`.
    pub audio: Option<AudioStream>,
    /// Every audio track, in container order (audio-relative index = position).
    pub audio_tracks: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleTrack>,
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
/// [`crate::enrich`]. A no-op when there are no unprobed files.
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
        let worker_count = PROBE_WORKERS.min(total.max(1));
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

                // Whether this is the item's first probe → emit ItemUpdated so
                // the client shows the codec badge appear.
                let first_for_item = db::item_has_probed_file(&pool, &job.item_id)
                    .map(|has| !has)
                    .unwrap_or(true);

                let result = probe_file(Path::new(&job.abs_path), ffprobe_present);
                match db::set_file_probe(
                    &pool,
                    &job.file_id,
                    result.duration_ms,
                    result.video.as_ref(),
                    result.audio.as_ref(),
                    &result.audio_tracks,
                    &result.subtitles,
                ) {
                    Ok(()) => {
                        if first_for_item {
                            bus.publish(ServerEvent::ItemUpdated { id: job.item_id.clone() });
                        }
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        activity::probe_progress(&activity, n);
                        if n % 25 == 0 {
                            bus.publish(ServerEvent::ProbeProgress { done: n, total });
                        }
                    }
                    Err(e) => warn!(file = %job.file_id, error = %e, "failed to store probe result"),
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

/// Run ffprobe and parse its JSON. Returns `None` (→ extension-guess fallback)
/// if anything goes wrong, logging the cause at DEBUG. It's DEBUG, not WARN,
/// because 10 workers probing every file would flood the default log; a wholesale
/// degradation still shows up as `probed` ≪ `total` in the phase-2 summary, and
/// `RUST_LOG=luma_server=debug` surfaces the per-file detail. `-v error` (vs the
/// old `-v quiet`) lets ffmpeg's own diagnostic reach stderr.
fn run_ffprobe(path: &Path) -> Option<ProbeResult> {
    let output = match Command::new("ffprobe")
        .args(["-v", "error", "-show_format", "-show_streams", "-of", "json"])
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

/// Build our model from raw ffprobe output.
fn build_result(raw: FfprobeOutput) -> ProbeResult {
    // Every audio stream, in container order. The audio-relative index (0-based
    // among audio streams) is the position here — exactly ffmpeg's `0:a:<n>`.
    let audio_tracks: Vec<AudioStream> = raw
        .streams
        .iter()
        .filter(|&s| s.codec_type.as_deref() == Some("audio"))
        .enumerate()
        .map(|(i, s)| build_audio(s, i as u32))
        .collect();
    let audio = audio_tracks.first().cloned();

    ProbeResult {
        duration_ms: raw.format.as_ref().and_then(|fmt| {
            fmt.duration
                .as_deref()
                .and_then(|d| d.parse::<f64>().ok())
                .map(|secs| (secs * 1000.0) as u64)
        }),
        // First real video stream, skipping embedded cover-art (mjpeg posters):
        // the cover-art test must stay in the predicate so a leading poster
        // stream doesn't win and null out the actual video.
        video: raw
            .streams
            .iter()
            .find(|&s| s.codec_type.as_deref() == Some("video") && !is_probably_cover_art(s))
            .map(build_video),
        audio,
        audio_tracks,
        subtitles: raw
            .streams
            .iter()
            .filter(|&s| s.codec_type.as_deref() == Some("subtitle"))
            .map(|s| SubtitleTrack {
                language: s.language(),
                codec: normalize_codec(s.codec_name.as_deref()),
            })
            .collect(),
    }
}

fn is_probably_cover_art(stream: &FfStream) -> bool {
    matches!(stream.codec_name.as_deref(), Some("mjpeg") | Some("png"))
        && stream.width.unwrap_or(0) <= 1000
        && stream.height.unwrap_or(0) <= 1000
}

fn build_video(stream: &FfStream) -> VideoStream {
    let bit_depth = stream
        .bits_per_raw_sample
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .or_else(|| pixel_format_bit_depth(stream.pix_fmt.as_deref()));

    let hdr = is_hdr(stream, bit_depth);

    VideoStream {
        codec: normalize_codec(stream.codec_name.as_deref()),
        width: stream.width,
        height: stream.height,
        hdr,
        bit_depth,
    }
}

fn build_audio(stream: &FfStream, index: u32) -> AudioStream {
    AudioStream {
        index,
        codec: normalize_codec(stream.codec_name.as_deref()),
        channels: stream.channels,
        language: stream.language(),
        title: stream.title(),
        default: stream.disposition.as_ref().is_some_and(|d| d.default == Some(1)),
    }
}

/// HDR heuristic: PQ / HLG transfer, or 10-bit+ with a wide-gamut primary.
fn is_hdr(stream: &FfStream, bit_depth: Option<u32>) -> bool {
    let transfer = stream.color_transfer.as_deref().unwrap_or("");
    if matches!(transfer, "smpte2084" | "arib-std-b67") {
        return true;
    }
    let wide_gamut = matches!(
        stream.color_primaries.as_deref().unwrap_or(""),
        "bt2020"
    );
    bit_depth.map(|b| b >= 10).unwrap_or(false) && wide_gamut
}

/// Map common pixel formats to a bit depth when `bits_per_raw_sample` is absent.
fn pixel_format_bit_depth(pix_fmt: Option<&str>) -> Option<u32> {
    let pix_fmt = pix_fmt?;
    if pix_fmt.contains("p10") || pix_fmt.contains("10le") || pix_fmt.contains("10be") {
        Some(10)
    } else if pix_fmt.contains("p12") || pix_fmt.contains("12le") || pix_fmt.contains("12be") {
        Some(12)
    } else if !pix_fmt.is_empty() {
        Some(8)
    } else {
        None
    }
}

/// Normalize a codec name to the lowercase canonical form clients expect.
pub fn normalize_codec(name: Option<&str>) -> String {
    let raw = name.unwrap_or("unknown").to_ascii_lowercase();
    match raw.as_str() {
        "h265" | "hevc" => "hevc",
        "h264" | "avc" => "h264",
        "av01" | "av1" => "av1",
        "vp09" | "vp9" => "vp9",
        "vp08" | "vp8" => "vp8",
        "mpeg4" => "mpeg4",
        "eac3" | "e-ac-3" => "eac3",
        "ac3" | "ac-3" => "ac3",
        "dca" | "dts" => "dts",
        "truehd" => "truehd",
        "mp4a" | "aac" => "aac",
        "mp3" | "mp3float" => "mp3",
        "flac" => "flac",
        "opus" => "opus",
        "vorbis" => "vorbis",
        "subrip" | "srt" => "subrip",
        "ass" | "ssa" => "ass",
        "hdmv_pgs_subtitle" | "pgs" => "pgs",
        "mov_text" => "mov_text",
        // Unknown codec — hand back the owned, already-lowercased string rather
        // than re-allocating a copy of it.
        _ => return raw,
    }
    .to_string()
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
    }
}

// ----- Raw ffprobe JSON shapes -------------------------------------------------

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfStream>,
    #[serde(default)]
    format: Option<FfFormat>,
}

#[derive(Debug, Deserialize)]
struct FfFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    channels: Option<u32>,
    pix_fmt: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    bits_per_raw_sample: Option<String>,
    #[serde(default)]
    tags: Option<FfTags>,
    #[serde(default)]
    disposition: Option<FfDisposition>,
}

#[derive(Debug, Deserialize)]
struct FfTags {
    language: Option<String>,
    title: Option<String>,
}

/// ffprobe stream `disposition` flags — we only read `default`.
#[derive(Debug, Deserialize)]
struct FfDisposition {
    default: Option<u8>,
}

impl FfStream {
    fn language(&self) -> Option<String> {
        self.tags
            .as_ref()
            .and_then(|t| t.language.clone())
            .filter(|l| !l.is_empty() && l != "und")
    }

    fn title(&self) -> Option<String> {
        self.tags
            .as_ref()
            .and_then(|t| t.title.clone())
            .filter(|t| !t.trim().is_empty())
    }
}
