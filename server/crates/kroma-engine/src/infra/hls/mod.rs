//! On-demand HLS delivery.
//!
//! Direct-play (raw byte range, [`crate::infra::stream`]) is the default; this
//! covers what a browser can't direct-play: MKV→fMP4 repackaging, audio it can't
//! decode (AC3/EAC3/DTS → AAC), and seamless language switching. It is a thin
//! wrapper over a [`session`] registry: one continuous ffmpeg per (item,
//! audio-mode, ANCHOR) produces a complete-program HLS master (video + alternate
//! audio renditions); see [`session`] for why continuous (correct A/V, no desync,
//! no holes) and how the client seeks.
//!
//! The anchor (resume / seek position, input `-ss`) is part of the key AND the
//! URL path (`/hls/:mode/:anchor/...`). So each seek gets its OWN session with
//! UNIQUE child URLs - a re-anchor never reuses another anchor's segment names
//! (which would replay stale content) and never thrashes a shared session. Old
//! anchors are LRU- / idle-reaped.

mod session;

use std::path::Path;
use std::sync::Arc;

use session::Sessions;

/// `{item_id}:{copy|aac}:{anchor_secs}:a{audio}` session key. The audio track is
/// part of the key because it is muxed into the stream (one program per language).
fn session_key(item_id: &str, aac: bool, anchor: u64, audio: u32) -> String {
    format!("{item_id}:{}:{anchor}:a{audio}", if aac { "aac" } else { "copy" })
}

pub struct HlsEngine {
    sessions: Arc<Sessions>,
}

impl HlsEngine {
    /// `max_concurrent` hard-caps live sessions; `cache_budget` is the on-disk
    /// byte budget that trims idle / superseded sessions (0 = unlimited).
    pub fn new(data_dir: &Path, max_concurrent: usize, cache_budget: u64) -> Self {
        HlsEngine { sessions: Arc::new(Sessions::new(data_dir, max_concurrent, cache_budget)) }
    }

    pub fn spawn_reaper(&self) {
        self.sessions.spawn_reaper();
    }

    pub fn cache_bytes(&self) -> u64 {
        self.sessions.bytes()
    }

    /// Retune the on-disk cache byte budget at runtime (0 = unlimited).
    pub fn set_cache_budget(&self, bytes: u64) {
        self.sessions.set_budget(bytes);
    }

    /// The media playlist for `item_id` in `copy`/`aac` mode, anchored at `anchor`
    /// seconds (input `-ss`, 0 = start), muxing the `audio`-th audio track. Returns
    /// the playlist text + the REAL stream start (s) - the keyframe at-or-before
    /// `anchor` - for the client's `baseSec`.
    pub async fn master(&self, item_id: &str, input: &str, audio: u32, aac: bool, anchor: u64) -> Option<(String, f64)> {
        let key = session_key(item_id, aac, anchor, audio);
        let (bytes, start) = self.sessions.master(&key, Path::new(input), audio, aac, anchor as f64).await?;
        Some((String::from_utf8(bytes).ok()?, start))
    }

    /// A child file (init or segment) of the `(mode, anchor, audio)` session.
    pub async fn file(&self, item_id: &str, aac: bool, anchor: u64, audio: u32, name: &str) -> Option<(Vec<u8>, &'static str)> {
        let key = session_key(item_id, aac, anchor, audio);
        self.sessions.file(&key, name).await
    }
}
