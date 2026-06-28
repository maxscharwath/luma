//! On-demand HLS **audio-transcode** sessions.
//!
//! LUMA's streaming policy is direct-play: [`crate::stream`] serves original
//! bytes and the server never re-encodes *video*. The one exception is audio.
//! HEVC files routinely carry AC3/EAC3/DTS/TrueHD tracks that browsers
//! (Chrome/Firefox) refuse to decode for licensing reasons, which yields
//! video-but-no-sound. For those clients we expose an HLS variant that *copies*
//! the video stream untouched and transcodes only the audio to stereo AAC —
//! cheap (no video re-encode, runs many× realtime) and surgical.
//!
//! A session is one running `ffmpeg` writing fragmented-MP4 HLS segments into a
//! per-item directory under `<data>/transcode/`. The playlist is served as it
//! grows (`event` type); idle sessions are reaped after [`IDLE_TIMEOUT`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{info, warn};

/// HLS target segment duration handed to ffmpeg.
const SEGMENT_SECONDS: &str = "6";
/// Tear a session down after this long without a request.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the reaper sweeps for idle sessions.
const REAP_INTERVAL: Duration = Duration::from_secs(30);
/// Give ffmpeg this long to emit a playlist with a first playable segment.
const PLAYLIST_WAIT: Duration = Duration::from_secs(15);
/// A freshly-requested segment may not be flushed yet; poll for this long.
const SEGMENT_WAIT: Duration = Duration::from_secs(8);

/// One live transcode: the working directory plus the ffmpeg child to kill.
struct Session {
    dir: PathBuf,
    child: Mutex<Child>,
    last_access: Mutex<Instant>,
}

impl Session {
    async fn touch(&self) {
        *self.last_access.lock().await = Instant::now();
    }
}

/// Process-wide registry of HLS audio-transcode sessions, keyed by item id.
#[derive(Clone)]
pub struct Sessions {
    root: PathBuf,
    inner: Arc<Mutex<HashMap<String, Arc<Session>>>>,
}

impl Sessions {
    /// Create the registry, wiping any stale `<data>/transcode/` left by a crash.
    pub fn new(data_dir: &Path) -> Self {
        let root = data_dir.join("transcode");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        Sessions {
            root,
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Number of live transcode sessions (for the concurrent-transcode cap).
    pub async fn active_count(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Whether a session for `key` already exists (a reused session doesn't count
    /// against the cap).
    pub async fn has(&self, key: &str) -> bool {
        self.inner.lock().await.contains_key(key)
    }

    /// Start (or reuse) a session keyed by `key` and return the live playlist
    /// bytes. `audio_idx` selects which audio track to map (`0:a:<idx>`) and
    /// `copy` stream-copies that track instead of re-encoding it to stereo AAC.
    /// Waits up to [`PLAYLIST_WAIT`] for ffmpeg to list a first segment so the
    /// client can begin playback immediately. `None` means ffmpeg never produced
    /// output (missing binary, unreadable input, …).
    pub async fn playlist(&self, key: &str, input: &Path, audio_idx: u32, copy: bool) -> Option<Vec<u8>> {
        let session = match self.ensure(key, input, audio_idx, copy).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, session = %key, "failed to start audio transcode");
                return None;
            }
        };
        let path = session.dir.join("index.m3u8");
        let deadline = Instant::now() + PLAYLIST_WAIT;
        loop {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                // Wait until at least one segment is listed (`#EXTINF`), otherwise
                // hls.js would load an empty playlist and stall.
                if contains(&bytes, b"#EXTINF") {
                    return Some(bytes);
                }
            }
            if Instant::now() >= deadline {
                // Return whatever exists; a header-only playlist is better than a
                // hard error and the client will refresh.
                return tokio::fs::read(&path).await.ok();
            }
            sleep(Duration::from_millis(120)).await;
        }
    }

    /// Serve a file (init fragment, segment, or refreshed playlist) from a live
    /// session. Returns the bytes plus a content-type. `None` if the session is
    /// gone, the name is unsafe, or the file never appears.
    pub async fn file(&self, key: &str, name: &str) -> Option<(Vec<u8>, &'static str)> {
        if !is_safe_name(name) {
            return None;
        }
        let session = {
            let map = self.inner.lock().await;
            map.get(key).cloned()
        }?;
        session.touch().await;

        let path = session.dir.join(name);
        let deadline = Instant::now() + SEGMENT_WAIT;
        loop {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Some((bytes, content_type(name)));
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    /// Start (or reuse) a single-stream HLS **master** session: copy the video
    /// once and expose every audio track as an alternate rendition, then return
    /// the master playlist bytes. The player switches language in place (no
    /// reload, position preserved). Waits up to [`PLAYLIST_WAIT`] for ffmpeg to
    /// write the master playlist. `None` if ffmpeg never produced output.
    pub async fn master(&self, key: &str, input: &Path, tracks: &[MasterTrack], aac: bool, start_secs: f64) -> Option<Vec<u8>> {
        let session = match self.ensure_master(key, input, tracks, aac, start_secs).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, session = %key, "failed to start audio master remux");
                return None;
            }
        };
        let path = session.dir.join("master.m3u8");
        let deadline = Instant::now() + PLAYLIST_WAIT;
        loop {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                // ffmpeg writes the master up front (it knows every variant), so a
                // STREAM-INF line means it's ready to hand to the player.
                if contains(&bytes, b"#EXT-X-STREAM-INF") {
                    return Some(bytes);
                }
            }
            if Instant::now() >= deadline {
                return tokio::fs::read(&path).await.ok();
            }
            sleep(Duration::from_millis(120)).await;
        }
    }

    /// Look up an existing master session or spawn ffmpeg for a new one.
    async fn ensure_master(&self, key: &str, input: &Path, tracks: &[MasterTrack], aac: bool, start_secs: f64) -> std::io::Result<Arc<Session>> {
        let mut map = self.inner.lock().await;
        if let Some(s) = map.get(key) {
            s.touch().await;
            return Ok(s.clone());
        }
        let dir = self.root.join(safe_dir(key));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let child = spawn_ffmpeg_master(input, &dir, tracks, aac, start_secs)?;
        info!(session = %key, renditions = tracks.len(), dir = %dir.display(), "started HLS master remux (video copy + alt-audio renditions)");
        let session = Arc::new(Session {
            dir,
            child: Mutex::new(child),
            last_access: Mutex::new(Instant::now()),
        });
        map.insert(key.to_string(), session.clone());
        Ok(session)
    }

    /// Look up an existing session or spawn ffmpeg for a new one.
    async fn ensure(&self, key: &str, input: &Path, audio_idx: u32, copy: bool) -> std::io::Result<Arc<Session>> {
        let mut map = self.inner.lock().await;
        if let Some(s) = map.get(key) {
            s.touch().await;
            return Ok(s.clone());
        }
        let dir = self.root.join(safe_dir(key));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let child = spawn_ffmpeg(input, &dir, audio_idx, copy)?;
        let mode = if copy { "stream-copy" } else { "AAC stereo" };
        info!(session = %key, audio = audio_idx, dir = %dir.display(), "started HLS remux (video copy + {mode} audio)");
        let session = Arc::new(Session {
            dir,
            child: Mutex::new(child),
            last_access: Mutex::new(Instant::now()),
        });
        map.insert(key.to_string(), session.clone());
        Ok(session)
    }

    /// Background task: kill + clean up sessions idle longer than [`IDLE_TIMEOUT`].
    pub fn spawn_reaper(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                sleep(REAP_INTERVAL).await;
                let now = Instant::now();
                let mut map = inner.lock().await;
                let mut dead = Vec::new();
                for (id, s) in map.iter() {
                    if now.duration_since(*s.last_access.lock().await) > IDLE_TIMEOUT {
                        dead.push(id.clone());
                    }
                }
                for id in dead {
                    if let Some(s) = map.remove(&id) {
                        let _ = s.child.lock().await.start_kill();
                        let _ = std::fs::remove_dir_all(&s.dir);
                        info!(item = %id, "reaped idle transcode session");
                    }
                }
            }
        });
    }
}

/// One audio rendition in a master remux: which source audio track to map and
/// how to label it. v1 stream-copies every track (so the runtime must natively
/// decode them — gated client-side by `canSeamlessAudioSwitch`).
pub struct MasterTrack {
    /// Audio-relative source index (`-map 0:a:<index>`).
    pub index: u32,
    /// BCP-47-ish language tag for the rendition (sanitised before use).
    pub language: Option<String>,
    /// Marks the rendition the player selects by default (exactly one should be).
    pub default: bool,
}

/// Build the ffmpeg **master**-playlist command: copy the video once and copy
/// every listed audio track as an alternate HLS rendition (audio group `aud`), so
/// the player switches language in place. Emits master.m3u8 + per-variant
/// playlists/segments (fMP4): `stream_%v.m3u8`, `init_%v.mp4`, `seg_%v_*.m4s`.
fn spawn_ffmpeg_master(input: &Path, dir: &Path, tracks: &[MasterTrack], aac: bool, start_secs: f64) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    // Input seeking: start the remux at `start_secs` so the requested position is
    // available immediately (no waiting for a from-zero remux to reach it).
    let seeking = start_secs > 0.5;
    if seeking {
        cmd.arg("-ss").arg(format!("{start_secs:.3}"));
    }
    cmd.arg("-i").arg(input);
    // `-copyts` keeps the ORIGINAL timestamps so every rendition (the copied video
    // + each audio rendition, which are separate outputs) stays on one shared
    // timeline and the player aligns them by PTS — without it each output is zeroed
    // independently and the keyframe-snapped video drifts out of sync with the
    // audio. The player still normalises the visible start to 0, so the client's
    // baseSec offset (added back for the bar/subtitles/progress) is unaffected.
    if seeking {
        cmd.arg("-copyts");
    }
    cmd.args(["-map", "0:v:0"]);
    for t in tracks {
        cmd.arg("-map").arg(format!("0:a:{}", t.index));
    }
    // Video is always stream-copied. Audio is copied (surround preserved, for
    // runtimes that decode it) or transcoded to stereo AAC (so browsers that
    // can't decode AC3/EAC3/DTS via MSE can still play — and switch — every track).
    cmd.args(["-c:v", "copy"]);
    if aac {
        cmd.args(["-c:a", "aac", "-ac", "2", "-b:a", "192k"]);
    } else {
        cmd.args(["-c:a", "copy"]);
    }

    // var_stream_map: one video variant + one variant per audio rendition, all in
    // the `aud` group so they're alternates of the same program.
    let mut map = String::from("v:0,agroup:aud");
    for (i, t) in tracks.iter().enumerate() {
        map.push_str(&format!(" a:{i},agroup:aud"));
        let lang: String = t
            .language
            .as_deref()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8)
            .collect();
        if !lang.is_empty() {
            map.push_str(&format!(",language:{lang}"));
        }
        if t.default {
            map.push_str(",default:yes");
        }
    }

    cmd.args(["-f", "hls", "-hls_time", SEGMENT_SECONDS])
        .args(["-hls_playlist_type", "event"])
        .args(["-hls_segment_type", "fmp4"])
        .args(["-hls_fmp4_init_filename", "init_%v.mp4"])
        .arg("-hls_segment_filename")
        .arg(dir.join("seg_%v_%05d.m4s"))
        .args(["-hls_flags", "independent_segments+temp_file"])
        .args(["-master_pl_name", "master.m3u8"])
        .arg("-var_stream_map")
        .arg(map)
        .arg(dir.join("stream_%v.m3u8"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build the ffmpeg HLS command: copy the video stream verbatim, select audio
/// track `audio_idx`, and either stream-copy it (`copy`, preserving surround
/// with no re-encode) or transcode it to stereo AAC for runtimes that can't
/// decode the source codec. Emits fragmented-MP4 segments.
fn spawn_ffmpeg(input: &Path, dir: &Path, audio_idx: u32, copy: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(input)
        // First video + the chosen audio track; ignore extra streams (subs/data)
        // the HLS muxer can't carry in fMP4.
        .args(["-map", "0:v:0"])
        .arg("-map")
        .arg(format!("0:a:{audio_idx}"))
        .args(["-c:v", "copy"]);
    if copy {
        cmd.args(["-c:a", "copy"]);
    } else {
        cmd.args(["-c:a", "aac", "-ac", "2", "-b:a", "192k"]);
    }
    cmd.args(["-f", "hls", "-hls_time", SEGMENT_SECONDS])
        .args(["-hls_playlist_type", "event"])
        .args(["-hls_segment_type", "fmp4"])
        .args(["-hls_fmp4_init_filename", "init.mp4"])
        .arg("-hls_segment_filename")
        .arg(dir.join("seg_%05d.m4s"))
        // `temp_file` → write to `.tmp` then atomically rename, so we never serve
        // a half-written segment/playlist.
        .args(["-hls_flags", "independent_segments+temp_file"])
        .arg(dir.join("index.m3u8"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Map an HLS file name to its content-type.
fn content_type(name: &str) -> &'static str {
    if name.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else if name.ends_with(".mp4") {
        "video/mp4"
    } else {
        // fMP4 media segments (.m4s)
        "video/iso.segment"
    }
}

/// Reject path traversal and anything but a plain segment/playlist file name.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A filesystem-safe directory name derived from an item id.
fn safe_dir(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' })
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
