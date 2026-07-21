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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{info, warn};

use super::{same_program, StreamMode};

const SEGMENT_SECONDS: &str = "6";
const IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const REAP_INTERVAL: Duration = Duration::from_secs(30);
/// Give ffmpeg this long to write the master / a requested segment.
const FILE_WAIT: Duration = Duration::from_secs(20);
/// Read-ahead cap: ffmpeg reads the input at most this multiple of realtime, so N
/// concurrent sessions don't thrash the disk / network mount racing to buffer the
/// whole file. Also bounds the on-disk footprint, which then grows at ~playback
/// rate rather than all-at-once.
/// 2.0 (not 1.0) so the produced edge pulls AHEAD of the playhead over time,
/// letting clients build a deep forward buffer (the web engines now target ~120s;
/// see FORWARD_BUFFER_SEC in video-engine.ts) instead of riding the live edge.
const READRATE: &str = "2.0";
/// Seconds of stream read at FULL speed before [`READRATE`] throttling kicks in,
/// so playback starts instantly AND a chunk of head-start buffer lands up front.
/// Needs ffmpeg >= 6.1 (see [`detect_burst`]).
const READRATE_BURST: &str = "60";
/// A session whose last access is more recent than this is treated as actively
/// playing: it is NEVER evicted to reclaim disk (that would stall a live stream)
/// and is dropped under the concurrency cap only as a last resort.
/// Superseded anchors / finished sessions go idle at once, so they free quickly.
const BUDGET_GRACE: Duration = Duration::from_secs(45);
/// Per-session sliding window: keep this many segments BEHIND the furthest one the
/// client has requested (its playhead + read-ahead), delete older ones. ~45 x 6s =
/// 270s. The furthest requested segment sits ~forward-buffer ahead of the playhead
/// (the web engines now target ~120s), so this must exceed forward+back buffer in
/// segments (~30) with margin, or it would prune a segment the client still holds
/// in its back-buffer and stall a backward seek.
/// Safe with no client change: the player NATIVE-seeks only into already-buffered
/// ranges and re-anchors (fresh session) otherwise, so a pruned segment is never
/// re-fetched (see `seekTo` in the web `useVideoPlayback`).
const KEEP_BEHIND_SEGS: u64 = 45;

struct Session {
    dir: PathBuf,
    child: Mutex<Child>,
    last_access: Mutex<Instant>,
    /// Highest segment index the client has requested (its playhead + read-ahead),
    /// the anchor for behind-pruning. See [`KEEP_BEHIND_SEGS`].
    max_seg: AtomicU64,
    /// Prune watermark: the exclusive cutoff below which segments have been deleted
    /// by [`prune_behind`] and will NEVER be reproduced (the remux is forward-only).
    /// `file()` 404s such indices immediately instead of blocking [`FILE_WAIT`].
    pruned: AtomicU64,
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

/// Registry of continuous HLS remux sessions, keyed per program + anchor (see
/// `session_key`).
pub struct Sessions {
    root: PathBuf,
    cap: usize,
    /// On-disk byte budget for the whole cache (0 = unlimited). Enforced by
    /// evicting idle sessions oldest-first; live sessions are never touched.
    /// Atomic so an admin can retune it live (see [`Self::set_budget`]).
    budget: AtomicU64,
    /// Whether this ffmpeg supports `-readrate_initial_burst` (>= 6.1).
    burst: bool,
    inner: Mutex<HashMap<String, Arc<Session>>>,
}

impl Sessions {
    pub fn new(data_dir: &Path, cap: usize, budget: u64) -> Self {
        let root = data_dir.join("hls");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        Sessions { root, cap: cap.max(1), budget: AtomicU64::new(budget), burst: detect_burst(), inner: Mutex::new(HashMap::new()) }
    }

    /// Retune the disk budget at runtime (0 = unlimited); applied on the next
    /// `make_room` / reaper sweep.
    pub fn set_budget(&self, bytes: u64) {
        self.budget.store(bytes, Ordering::Relaxed);
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
    pub async fn master(&self, key: &str, input: &Path, audio: u32, mode: StreamMode, start_secs: f64) -> Option<(Vec<u8>, f64)> {
        let session = match self.ensure(key, input, audio, mode, start_secs).await {
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
        // Track the playhead (furthest requested segment) so the reaper can prune
        // everything well behind it (see prune_behind).
        if let Some(idx) = seg_index(name) {
            session.max_seg.fetch_max(idx, Ordering::Relaxed);
            // A segment below the prune watermark was deleted and will never be
            // reproduced (the remux only moves forward), so a poll would just burn
            // FILE_WAIT (20s) then 404. Return 404 NOW so the client re-anchors fast.
            if idx < session.pruned.load(Ordering::Relaxed) {
                return None;
            }
        }
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

    async fn ensure(&self, key: &str, input: &Path, audio: u32, mode: StreamMode, start_secs: f64) -> std::io::Result<Arc<Session>> {
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
        // A seek or an audio-filter toggle mints a NEW key for the same program,
        // so the client's previous session is dead weight the moment it stops
        // being read: reclaim it here instead of holding an ffmpeg (the filtered
        // modes really do transcode) plus its segments for the full IDLE_TIMEOUT.
        self.reap_superseded(&mut map, key).await;
        self.make_room(&mut map, key).await;
        let dir = self.root.join(safe_dir(key));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let child = spawn_stream(input, &dir, audio, mode, start_secs, self.burst)?;
        info!(session = %key, audio, mode = ?mode, anchor = start_secs, start, "started HLS remux");
        let session = Arc::new(Session { dir, child: Mutex::new(child), last_access: Mutex::new(Instant::now()), max_seg: AtomicU64::new(0), pruned: AtomicU64::new(0), start });
        map.insert(key.to_string(), session.clone());
        Ok(session)
    }

    /// Free a slot for the incoming `key`: enforce the hard concurrency cap,
    /// then the soft disk budget.
    ///
    /// The cap picks its victim in this order (one eviction per pass, so the
    /// loop always terminates):
    /// 1. the least-recently-used session when it has gone quiet (untouched for
    ///    [`BUDGET_GRACE`]). It is the coldest one by construction, so nothing
    ///    live is dropped while a colder session exists;
    /// 2. otherwise EVERY session is live and killing one stalls a viewer
    ///    mid-play (its next segment 404s), so prefer a sibling of `key` - same
    ///    program, different mode / anchor - which is almost certainly the
    ///    arriving client's OWN superseded stream (see [`same_program`]);
    /// 3. only failing that, the plain LRU: a new stream must be able to start
    ///    even when every session is genuinely live.
    async fn make_room(&self, map: &mut HashMap<String, Arc<Session>>, key: &str) {
        while map.len() >= self.cap {
            let Some((oldest, la)) = lru(map.iter()).await else { break };
            let victim = if Instant::now().duration_since(la) >= BUDGET_GRACE {
                oldest
            } else {
                lru_sibling(map, key).await.unwrap_or(oldest)
            };
            self.evict(map, &victim).await;
        }
        // Soft disk budget: reclaim idle bloat before adding another session.
        self.enforce_budget(map).await;
    }

    /// Drop the sessions superseded by `key` - same program (title + audio
    /// track), different mode / anchor - that have already gone quiet
    /// ([`BUDGET_GRACE`], the same liveness test the disk budget uses). A
    /// re-anchor mints such a sibling and never reads the old one again, so this
    /// returns its ffmpeg + disk in seconds instead of [`IDLE_TIMEOUT`].
    /// Sibling sessions still being read are left alone: the HLS routes are
    /// anonymous (no bearer, no device id), so a warm sibling could equally be a
    /// second viewer on the same title and we must not cut them off.
    async fn reap_superseded(&self, map: &mut HashMap<String, Arc<Session>>, key: &str) {
        let now = Instant::now();
        let mut stale = Vec::new();
        for (k, s) in map.iter() {
            let quiet = now.duration_since(*s.last_access.lock().await) >= BUDGET_GRACE;
            if k != key && quiet && same_program(k, key) {
                stale.push(k.clone());
            }
        }
        for k in stale {
            self.evict(map, &k).await;
        }
    }

    /// Evict idle / superseded sessions oldest-first until the on-disk cache is
    /// under [`Self::budget`]. A session touched within [`BUDGET_GRACE`] is treated
    /// as actively playing and left alone (even if that means briefly exceeding the
    /// budget) - dropping a live stream's segments mid-play would stall it. The
    /// oldest entry gates the loop, so once it is "active" the rest are too.
    /// `budget == 0` disables trimming.
    async fn enforce_budget(&self, map: &mut HashMap<String, Arc<Session>>) {
        let budget = self.budget.load(Ordering::Relaxed);
        if budget == 0 {
            return;
        }
        let mut total = self.bytes();
        while total > budget && map.len() > 1 {
            let Some((k, la)) = lru(map.iter()).await else { break };
            if Instant::now().duration_since(la) < BUDGET_GRACE {
                break; // the least-recent session is still live - keep it and the rest
            }
            if let Some(s) = map.get(&k) {
                total = total.saturating_sub(dir_bytes(&s.dir));
            }
            self.evict(map, &k).await;
        }
    }

    /// Kill a session's ffmpeg and delete its segment directory.
    async fn evict(&self, map: &mut HashMap<String, Arc<Session>>, key: &str) {
        if let Some(s) = map.remove(key) {
            let _ = s.child.lock().await.start_kill();
            let _ = std::fs::remove_dir_all(&s.dir);
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
                    this.evict(&mut map, &id).await;
                }
                // Bound each LIVE session: drop segments far behind its playhead so
                // a single long stream's footprint stays flat (the budget alone
                // can't trim an active session). Safe with no client change - the
                // player never re-fetches an un-buffered segment (see seekTo).
                for s in map.values() {
                    prune_behind(s);
                }
                // Then reclaim whole idle / superseded sessions still over budget.
                this.enforce_budget(&mut map).await;
            }
        });
    }
}

/// The (key, last_access) of the least-recently-used session in `sessions`, if
/// any. Takes an iterator so a subset (e.g. one program's sessions) can be
/// ranked with the same pass.
async fn lru<'a>(sessions: impl Iterator<Item = (&'a String, &'a Arc<Session>)>) -> Option<(String, Instant)> {
    let mut victim: Option<(String, Instant)> = None;
    for (k, s) in sessions {
        let la = *s.last_access.lock().await;
        match &victim {
            Some((_, t)) if *t <= la => {}
            _ => victim = Some((k.clone(), la)),
        }
    }
    victim
}

/// The key of the least-recently-used session that plays the same program as
/// `key` (another mode / anchor of the same title + audio track), if any. `key`
/// itself is never returned. See [`Sessions::make_room`] for why this is the
/// preferred victim once every session is live.
async fn lru_sibling(map: &HashMap<String, Arc<Session>>, key: &str) -> Option<String> {
    lru(map.iter().filter(|(k, _)| k.as_str() != key && same_program(k, key))).await.map(|(k, _)| k)
}

/// Delete this session's media segments more than [`KEEP_BEHIND_SEGS`] behind the
/// furthest one the client has requested. The init fragment and playlist are never
/// touched (they are not `seg_*.m4s`); the playlist keeps listing pruned entries,
/// but the player never re-fetches them - a seek to an un-buffered position
/// re-anchors a fresh session instead (see web `seekTo`).
fn prune_behind(s: &Session) {
    let max = s.max_seg.load(Ordering::Relaxed);
    if max <= KEEP_BEHIND_SEGS {
        return; // nothing far enough behind yet
    }
    let cutoff = max - KEEP_BEHIND_SEGS;
    // Publish the watermark so file() can 404 pruned indices without polling.
    s.pruned.fetch_max(cutoff, Ordering::Relaxed);
    let Ok(entries) = std::fs::read_dir(&s.dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Some(idx) = entry.file_name().to_str().and_then(seg_index) {
            if idx < cutoff {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// The index of a media segment filename (`seg_00042.m4s` → 42); `None` for the
/// init fragment, playlists, or anything else.
fn seg_index(name: &str) -> Option<u64> {
    name.strip_prefix("seg_")?.strip_suffix(".m4s")?.parse().ok()
}

/// Recursive byte size of one session's segment directory.
fn dir_bytes(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Whether the installed ffmpeg understands `-readrate_initial_burst` (added in
/// 6.1). Probed once at startup; on older builds we fall back to a plain
/// `-readrate` (universally supported) and accept a slightly slower first segment.
fn detect_burst() -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-h", "full"])
        .output()
        .map(|o| {
            let mut s = o.stdout;
            s.extend_from_slice(&o.stderr);
            contains(&s, b"readrate_initial_burst")
        })
        .unwrap_or(false)
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
fn spawn_stream(input: &Path, dir: &Path, audio: u32, mode: StreamMode, start_secs: f64, burst: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    // Remux never decodes video (`-c:v copy`); at most it decodes ONE audio
    // stream for the aac fallback. `-threads 1` stops ffmpeg from standing up a
    // per-core decoder pool it can't use, keeping a session ~free CPU-wise.
    cmd.args(["-v", "error", "-nostdin", "-threads", "1"]);
    if start_secs > 0.5 {
        // `-noaccurate_seek` is CRITICAL for A/V sync: the default accurate seek
        // backs the video up to a keyframe but decodes-and-DISCARDS audio up to
        // the exact -ss point, so audio starts ~1 GOP after video (desync).
        // `-noaccurate_seek` starts BOTH streams at the keyframe together; the
        // client learns the real start via the X-Hls-Start header → baseSec.
        cmd.arg("-noaccurate_seek").arg("-ss").arg(format!("{start_secs:.3}"));
    }
    // Read-ahead throttle (input option, before `-i`): cap reading at READRATE x
    // realtime so concurrent sessions don't saturate the disk / network mount, and
    // so segments are produced at ~playback rate instead of the whole file at once.
    // The initial burst (when the ffmpeg supports it) reads the first chunk at full
    // speed so playback still starts immediately. Throttling only caps the UPPER
    // bound, so it never slows a mount that already can't keep up.
    cmd.args(["-readrate", READRATE]);
    if burst {
        cmd.args(["-readrate_initial_burst", READRATE_BURST]);
    }
    cmd.arg("-i").arg(input);
    if start_secs > 0.5 {
        cmd.arg("-copyts"); // keep source timestamps so video + audio stay on one timeline
    }
    cmd.args(["-map", "0:v:0"]).arg("-map").arg(format!("0:a:{audio}"));
    cmd.args(["-c:v", "copy"]);
    if mode.transcode() {
        // The loudness filter (volume leveling) rides the decode the transcode
        // already pays for; `-ac 2` downmixes after the filter graph.
        if let Some(af) = mode.filter_chain() {
            cmd.args(["-af", af]);
        }
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
    fn seg_indices() {
        assert_eq!(seg_index("seg_00042.m4s"), Some(42));
        assert_eq!(seg_index("seg_00000.m4s"), Some(0));
        assert_eq!(seg_index("init.mp4"), None);
        assert_eq!(seg_index("index.m3u8"), None);
        assert_eq!(seg_index("seg_.m4s"), None);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("master.m3u8"), "application/vnd.apple.mpegurl");
        assert_eq!(content_type("init_0.mp4"), "video/mp4");
        assert_eq!(content_type("seg_0_00001.m4s"), "video/iso.segment");
    }

    // ---- eviction policy -----------------------------------------------------
    // The registry is driven directly (no ffmpeg): each fake session holds a
    // harmless child process and an explicit last-access age, which is all the
    // cap / supersede rules look at.

    /// Age of a session the client is still reading (well inside [`BUDGET_GRACE`]).
    const LIVE: Duration = Duration::from_secs(1);
    /// Age of a session nobody has read for longer than [`BUDGET_GRACE`].
    const QUIET: Duration = Duration::from_secs(BUDGET_GRACE.as_secs() + 5);

    fn fake_session(dir: PathBuf, age: Duration) -> Arc<Session> {
        let child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn the stand-in child");
        let last = Instant::now().checked_sub(age).expect("monotonic clock older than the test window");
        Arc::new(Session {
            dir,
            child: Mutex::new(child),
            last_access: Mutex::new(last),
            max_seg: AtomicU64::new(0),
            pruned: AtomicU64::new(0),
            start: 0.0,
        })
    }

    /// A registry over its own temp root, pre-populated with `(key, age)` fakes.
    /// `budget = 0` so only the concurrency cap is exercised.
    async fn registry(name: &str, cap: usize, sessions: &[(&str, Duration)]) -> Sessions {
        let data = std::env::temp_dir().join(format!("kroma-hls-test-{}-{name}", std::process::id()));
        let s = Sessions::new(&data, cap, 0);
        let mut map = s.inner.lock().await;
        for (key, age) in sessions {
            let dir = s.root.join(safe_dir(key));
            std::fs::create_dir_all(&dir).expect("session dir");
            map.insert((*key).to_string(), fake_session(dir, *age));
        }
        drop(map);
        s
    }

    /// The registry's session keys, sorted.
    async fn keys(s: &Sessions) -> Vec<String> {
        let mut keys: Vec<String> = s.inner.lock().await.keys().cloned().collect();
        keys.sort();
        keys
    }

    #[tokio::test]
    async fn hard_cap_prefers_the_arriving_clients_own_superseded_sibling() {
        // Two viewers mid-stream: itA started first (so it is the LRU) while itB
        // toggles the audio filter, which mints a third key for its own program.
        let s = registry(
            "sibling",
            2,
            &[("itA:copy:0:a0", Duration::from_secs(3)), ("itB:aac:0:a0", LIVE)],
        )
        .await;
        {
            let mut map = s.inner.lock().await;
            s.make_room(&mut map, "itB:aac-night:0:a0").await;
        }
        // The plain-LRU rule used to kill itA here: another viewer's LIVE stream.
        assert_eq!(keys(&s).await, ["itA:copy:0:a0"]);
        assert!(!s.root.join(safe_dir("itB:aac:0:a0")).exists());
    }

    #[tokio::test]
    async fn hard_cap_evicts_a_quiet_session_before_a_live_sibling() {
        let s = registry("quiet", 2, &[("itA:copy:0:a0", QUIET), ("itB:aac:0:a0", LIVE)]).await;
        {
            let mut map = s.inner.lock().await;
            s.make_room(&mut map, "itB:aac-night:0:a0").await;
        }
        assert_eq!(keys(&s).await, ["itB:aac:0:a0"]);
    }

    #[tokio::test]
    async fn hard_cap_still_frees_a_slot_when_every_session_is_live_and_unrelated() {
        let s = registry(
            "fallback",
            2,
            &[("itA:copy:0:a0", Duration::from_secs(3)), ("itB:aac:0:a0", LIVE)],
        )
        .await;
        {
            let mut map = s.inner.lock().await;
            s.make_room(&mut map, "itC:copy:0:a0").await;
        }
        // Nothing is quiet and nothing is related, so the LRU still goes: a new
        // stream must always be able to start.
        assert_eq!(keys(&s).await, ["itB:aac:0:a0"]);
    }

    #[tokio::test]
    async fn superseded_siblings_are_reclaimed_only_once_quiet() {
        let s = registry(
            "supersede",
            8,
            &[
                ("itA:copy:0:a0", QUIET),  // the same program, gone quiet: superseded
                ("itA:aac:600:a0", LIVE),  // the same program but still being read
                ("itA:copy:0:a1", QUIET),  // another language track = another program
                ("itB:copy:0:a0", QUIET),  // another title
            ],
        )
        .await;
        {
            let mut map = s.inner.lock().await;
            s.reap_superseded(&mut map, "itA:aac-night:900:a0").await;
        }
        assert_eq!(keys(&s).await, ["itA:aac:600:a0", "itA:copy:0:a1", "itB:copy:0:a0"]);
        assert!(!s.root.join(safe_dir("itA:copy:0:a0")).exists());
    }
}
