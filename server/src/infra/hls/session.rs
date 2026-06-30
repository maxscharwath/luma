//! One continuous ffmpeg per (item, audio-mode): copies the video once and
//! exposes every audio track as an alternate rendition (the player switches
//! language in place), writing fMP4 segment files as it goes. The server serves
//! those files as ffmpeg produces them.
//!
//! Why continuous (not per-segment cuts): independent `-ss … -c:v copy -t <dur>`
//! cuts are unreliable on MKV (the cue index is a keyframe subset, so the seek
//! lands earlier and the copy over-runs), which desyncs audio/video and stalls
//! hls.js. One continuous process splits at real keyframes and decodes audio+video
//! together, so they are always aligned and gapless, and ffmpeg owns the playlist
//! (the only source of truth for the segment boundaries). With `-copyts` the
//! timeline is absolute, so the client seeks natively with no `baseSec`.
//!
//! A resume / far seek the client can't reach within the produced range reloads
//! the master at `?t=<secs>`, which RE-ANCHORS: ffmpeg restarts at that input
//! `-ss` so the position is available in ~1 s even over a network mount.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{info, warn};

const SEGMENT_SECONDS: &str = "6";
const IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const REAP_INTERVAL: Duration = Duration::from_secs(30);
/// Give ffmpeg this long to write the master / a requested segment.
const FILE_WAIT: Duration = Duration::from_secs(20);

struct Session {
    dir: PathBuf,
    child: Mutex<Child>,
    last_access: Mutex<Instant>,
    /// The real stream start (s): the keyframe at-or-before the requested anchor
    /// (where `-noaccurate_seek` puts BOTH video and audio). The client uses this
    /// as `baseSec` so the clock / subtitles stay aligned with A/V.
    start: f64,
}

impl Session {
    async fn touch(&self) {
        *self.last_access.lock().await = Instant::now();
    }
    async fn finished(&self) -> bool {
        matches!(self.child.lock().await.try_wait(), Ok(Some(_)))
    }
}

/// Registry of continuous HLS remux sessions, keyed by `{item_id}:{copy|aac}`.
pub struct Sessions {
    root: PathBuf,
    cap: usize,
    inner: Mutex<HashMap<String, Arc<Session>>>,
}

impl Sessions {
    pub fn new(data_dir: &Path, cap: usize) -> Self {
        let root = data_dir.join("hls");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        Sessions { root, cap: cap.max(1), inner: Mutex::new(HashMap::new()) }
    }

    pub fn bytes(&self) -> u64 {
        walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    /// Start (or reuse) the session for `key` (one muxed video+audio program) and
    /// return the media playlist bytes + the real stream start (s) for `baseSec`.
    pub async fn master(&self, key: &str, input: &Path, audio: u32, aac: bool, start_secs: f64) -> Option<(Vec<u8>, f64)> {
        let session = match self.ensure(key, input, audio, aac, start_secs).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, session = %key, "failed to start HLS remux");
                return None;
            }
        };
        let start = session.start;
        let path = session.dir.join("index.m3u8");
        let deadline = Instant::now() + FILE_WAIT;
        loop {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                // The media playlist has a target-duration header once it is valid.
                if contains(&bytes, b"#EXT-X-TARGETDURATION") {
                    return Some((bytes, start));
                }
            }
            if Instant::now() >= deadline {
                return tokio::fs::read(&path).await.ok().map(|b| (b, start));
            }
            sleep(Duration::from_millis(80)).await;
        }
    }

    /// Serve a child file (variant playlist, init fragment, or media segment). A
    /// variant playlist gets `#EXT-X-ENDLIST` once the remux has finished, so the
    /// completed stream becomes seekable VOD. A not-yet-produced segment is polled
    /// for until ffmpeg flushes it.
    pub async fn file(&self, key: &str, name: &str) -> Option<(Vec<u8>, &'static str)> {
        if !is_safe_name(name) {
            return None;
        }
        let session = { self.inner.lock().await.get(key).cloned() }?;
        session.touch().await;
        let path = session.dir.join(name);
        let deadline = Instant::now() + FILE_WAIT;
        loop {
            if let Ok(mut bytes) = tokio::fs::read(&path).await {
                if name.ends_with(".m3u8") && session.finished().await && !contains(&bytes, b"#EXT-X-ENDLIST") {
                    bytes.extend_from_slice(b"#EXT-X-ENDLIST\n");
                }
                return Some((bytes, content_type(name)));
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(Duration::from_millis(80)).await;
        }
    }

    async fn ensure(&self, key: &str, input: &Path, audio: u32, aac: bool, start_secs: f64) -> std::io::Result<Arc<Session>> {
        // The anchor is part of the key, so an existing session is always the
        // right one - just reuse it (no in-place re-anchor).
        {
            let map = self.inner.lock().await;
            if let Some(s) = map.get(key) {
                s.touch().await;
                return Ok(s.clone());
            }
        }
        // Probe the real start (keyframe at-or-before the anchor) WITHOUT holding
        // the lock - it shells out to ffprobe and must not stall other sessions.
        let start = keyframe_before(input, start_secs).await;

        let mut map = self.inner.lock().await;
        if let Some(s) = map.get(key) {
            s.touch().await; // another task created it while we probed
            return Ok(s.clone());
        }
        self.make_room(&mut map).await;
        let dir = self.root.join(safe_dir(key));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let child = spawn_stream(input, &dir, audio, aac, start_secs)?;
        info!(session = %key, audio, aac, anchor = start_secs, start, "started HLS remux");
        let session = Arc::new(Session { dir, child: Mutex::new(child), last_access: Mutex::new(Instant::now()), start });
        map.insert(key.to_string(), session.clone());
        Ok(session)
    }

    async fn make_room(&self, map: &mut HashMap<String, Arc<Session>>) {
        while map.len() >= self.cap {
            let mut victim: Option<(String, Instant)> = None;
            for (k, s) in map.iter() {
                let la = *s.last_access.lock().await;
                match &victim {
                    Some((_, t)) if *t <= la => {}
                    _ => victim = Some((k.clone(), la)),
                }
            }
            let Some((k, _)) = victim else { break };
            if let Some(s) = map.remove(&k) {
                let _ = s.child.lock().await.start_kill();
                let _ = std::fs::remove_dir_all(&s.dir);
            }
        }
    }

    pub fn spawn_reaper(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(REAP_INTERVAL).await;
                let now = Instant::now();
                let mut map = this.inner.lock().await;
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
                    }
                }
            }
        });
    }
}

/// The largest video keyframe PTS at-or-before `anchor` - where `-noaccurate_seek
/// -ss anchor` actually starts BOTH video and audio. 0 for `anchor <= 0.5`. Reads
/// only a short interval ending at `anchor`, keyframes only, so it is fast even
/// over a network mount. Falls back to `anchor` if ffprobe finds nothing.
async fn keyframe_before(input: &Path, anchor: f64) -> f64 {
    if anchor <= 0.5 {
        return 0.0;
    }
    let from = (anchor - 30.0).max(0.0);
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey"])
        .arg("-read_intervals")
        .arg(format!("{from:.3}%{anchor:.3}"))
        .args(["-show_entries", "frame=pts_time", "-of", "csv=p=0"])
        .arg(input)
        .output()
        .await;
    let Ok(out) = out else {
        return anchor;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut best: Option<f64> = None;
    for line in text.lines() {
        if let Ok(t) = line.trim().trim_end_matches(',').parse::<f64>() {
            if t <= anchor + 0.01 {
                best = Some(best.map_or(t, |b| b.max(t)));
            }
        }
    }
    best.unwrap_or(anchor)
}

/// Build the ffmpeg command for ONE program: copy the video + the SELECTED audio
/// track (`0:a:<audio>`), MUXED into a single media playlist (`index.m3u8`). We
/// mux the chosen language rather than expose alternate audio renditions because
/// hls.js's alternate-audio switching was unreliable (it kept playing rendition 0
/// regardless of selection); muxing makes the chosen language play unconditionally.
/// Language switch = the client reloads with a different `audio` (a fresh session).
fn spawn_stream(input: &Path, dir: &Path, audio: u32, aac: bool, start_secs: f64) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    if start_secs > 0.5 {
        // `-noaccurate_seek` is CRITICAL for A/V sync: the default accurate seek
        // backs the video up to a keyframe but decodes-and-DISCARDS audio up to
        // the exact -ss point, so audio starts ~1 GOP after video (desync).
        // `-noaccurate_seek` starts BOTH streams at the keyframe together; the
        // client learns the real start via the X-Hls-Start header → baseSec.
        cmd.arg("-noaccurate_seek").arg("-ss").arg(format!("{start_secs:.3}"));
    }
    cmd.arg("-i").arg(input);
    if start_secs > 0.5 {
        cmd.arg("-copyts"); // keep source timestamps so video + audio stay on one timeline
    }
    cmd.args(["-map", "0:v:0"]).arg("-map").arg(format!("0:a:{audio}"));
    cmd.args(["-c:v", "copy"]);
    if aac {
        cmd.args(["-c:a", "aac", "-ac", "2", "-b:a", "192k"]);
    } else {
        cmd.args(["-c:a", "copy"]);
    }
    cmd.args(["-f", "hls", "-hls_time", SEGMENT_SECONDS])
        .args(["-hls_playlist_type", "event"])
        .args(["-hls_segment_type", "fmp4"])
        .args(["-hls_fmp4_init_filename", "init.mp4"])
        .arg("-hls_segment_filename")
        .arg(dir.join("seg_%05d.m4s"))
        .args(["-hls_flags", "independent_segments+temp_file"])
        .arg(dir.join("index.m3u8"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .kill_on_drop(true);
    // Capture ffmpeg stderr to the session dir so remux failures are diagnosable.
    match std::fs::File::create(dir.join("ffmpeg.log")) {
        Ok(f) => {
            cmd.stderr(Stdio::from(f));
        }
        Err(_) => {
            cmd.stderr(Stdio::null());
        }
    }
    cmd.spawn()
}

fn content_type(name: &str) -> &'static str {
    if name.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else if name.ends_with(".mp4") {
        "video/mp4"
    } else {
        "video/iso.segment"
    }
}

fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains("..") && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn safe_dir(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' }).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name() {
        assert!(is_safe_name("seg_0_00001.m4s"));
        assert!(is_safe_name("init_0.mp4"));
        assert!(!is_safe_name("../x"));
        assert!(!is_safe_name("a/b"));
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("master.m3u8"), "application/vnd.apple.mpegurl");
        assert_eq!(content_type("init_0.mp4"), "video/mp4");
        assert_eq!(content_type("seg_0_00001.m4s"), "video/iso.segment");
    }
}
