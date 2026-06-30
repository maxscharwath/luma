//! The running-process side: the per-item session registry that spawns/reuses
//! the ffmpeg children, serves the live HLS playlist/segments as they grow, and
//! reaps idle sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{info, warn};

use super::ffmpeg::{spawn_ffmpeg, spawn_ffmpeg_master, MasterTrack};

/// Tear a session down after this long without a request.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// Under cap pressure we only evict a *cross-title* session this idle, long
/// enough that an actively-buffering player (which fetches ahead, then coasts on
/// its buffer) is never mistaken for abandoned.
const EVICT_GRACE: Duration = Duration::from_secs(90);
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

    /// Whether a session for `key` already exists (a reused session doesn't count
    /// against the cap).
    pub async fn has(&self, key: &str) -> bool {
        self.inner.lock().await.contains_key(key)
    }

    /// On-disk directory names of every live session. A disk sweep (the
    /// `cache.cleanup` job) consults this so it never deletes the working dir of a
    /// stream that's still playing (evicted/reaped sessions already remove their
    /// own dirs, so anything live here is in active use).
    pub async fn live_dir_names(&self) -> std::collections::HashSet<String> {
        self.inner
            .lock()
            .await
            .values()
            .filter_map(|s| s.dir.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            .collect()
    }

    /// Make room for a new session (`new_key`) under `cap` without ever stuttering
    /// a live viewer:
    ///   1. Drop this *same title*'s other sessions: seeking/re-opening spawns a
    ///      fresh `master.<…>.<startMs>` per position, and the old ones are pure
    ///      seek-orphans the client will never fetch again. This alone fixes the
    ///      single-user-seeking pile-up that used to 503.
    ///   2. If still at the cap (many *different* titles in flight, e.g. lots of
    ///      concurrent viewers), only evict a session idle beyond [`EVICT_GRACE`]
    ///      (paused/abandoned). If every other stream is live, allow a soft
    ///      overflow: an extra cheap audio-remux beats interrupting someone.
    pub async fn make_room(&self, cap: usize, new_key: &str) {
        let item = item_of(new_key);
        let mut map = self.inner.lock().await;
        let now = Instant::now();

        // Reclaim this title's OTHER sessions, but only ones gone idle past the
        // grace window. A same-title session keys in its `-ss` startMs, so it's
        // EITHER this user's abandoned seek-orphan (idle, last_access frozen at
        // creation) OR a *concurrent viewer* at a different offset (still
        // fetching). Evicting the latter kills a live stream, and its player then
        // re-requests its playlist -> make_room -> evicts us back: a mutual
        // eviction ping-pong that stutters both. So gate on EVICT_GRACE like the
        // cross-title path; true orphans are reclaimed here or by the reaper.
        let orphans: Vec<String> = {
            let mut out = Vec::new();
            for (k, s) in map.iter() {
                if k.as_str() == new_key || item_of(k) != item {
                    continue;
                }
                if now.duration_since(*s.last_access.lock().await) >= EVICT_GRACE {
                    out.push(k.clone());
                }
            }
            out
        };
        for k in orphans {
            evict_session(&mut map, &k).await;
        }

        while map.len() >= cap.max(1) {
            let now = Instant::now();
            let mut victim: Option<(String, Instant)> = None;
            for (k, s) in map.iter() {
                let la = *s.last_access.lock().await;
                if now.duration_since(la) < EVICT_GRACE {
                    continue; // actively buffering, never evict
                }
                match &victim {
                    Some((_, t)) if *t <= la => {}
                    _ => victim = Some((k.clone(), la)),
                }
            }
            let Some((key, _)) = victim else { break }; // all live → soft overflow
            evict_session(&mut map, &key).await;
        }
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

/// The item id portion of a `{id}:{variant}` session key.
fn item_of(key: &str) -> &str {
    key.split(':').next().unwrap_or(key)
}

/// Remove a session from `map`, kill its ffmpeg child, and delete its temp dir.
async fn evict_session(map: &mut HashMap<String, Arc<Session>>, key: &str) {
    if let Some(s) = map.remove(key) {
        let _ = s.child.lock().await.start_kill();
        let _ = std::fs::remove_dir_all(&s.dir);
        info!(session = %key, "evicted transcode session");
    }
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
