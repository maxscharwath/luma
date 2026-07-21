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

/// One program's audio treatment: stream-copy, plain stereo-AAC transcode, or an
/// AAC transcode with a loudness filter (night-mode volume leveling for clients
/// with no local audio DSP, e.g. Tizen AVPlay). A filter always transcodes - a
/// stream copy cannot be filtered - so the filtered variants subsume `Aac`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StreamMode {
    Copy,
    Aac,
    AacStandard,
    AacNight,
}

impl StreamMode {
    /// Parse the `{mode}` URL path segment (also the token used in session keys).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "copy" => Some(Self::Copy),
            "aac" => Some(Self::Aac),
            "aac-standard" => Some(Self::AacStandard),
            "aac-night" => Some(Self::AacNight),
            _ => None,
        }
    }

    /// The `{mode}` path/key token (the inverse of [`Self::parse`], emitted by the
    /// client URL builder in packages/client `media.ts`).
    fn token(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Aac => "aac",
            Self::AacStandard => "aac-standard",
            Self::AacNight => "aac-night",
        }
    }

    /// Whether the audio is decoded + re-encoded (vs stream-copied).
    fn transcode(self) -> bool {
        !matches!(self, Self::Copy)
    }

    /// The ffmpeg `-af` chain for the filtered modes, tuned to MATCH the client
    /// Web Audio compressor (packages/ui `audio-filter.ts`) so every engine
    /// sounds the same: standard = gentle 4:1 leveling with make-up gain
    /// (threshold -24 dB = 0.063), night = 8:1 peak clamping (threshold -28 dB =
    /// 0.04) with below-unity make-up so it is never louder than off/standard.
    fn filter_chain(self) -> Option<&'static str> {
        match self {
            Self::Copy | Self::Aac => None,
            Self::AacStandard => {
                Some("acompressor=threshold=0.063:ratio=4:attack=10:release=250:knee=6:makeup=1.4")
            }
            Self::AacNight => {
                Some("acompressor=threshold=0.04:ratio=8:attack=4:release=250:knee=5,volume=0.9")
            }
        }
    }
}

/// `{item_id}:{mode}:{anchor_secs}:a{audio}` session key. The audio track is
/// part of the key because it is muxed into the stream (one program per
/// language); the mode is part of it because filtered and clean programs must
/// never share segment URLs (segments are cached immutably per URL).
fn session_key(item_id: &str, mode: StreamMode, anchor: u64, audio: u32) -> String {
    format!("{item_id}:{}:{anchor}:a{audio}", mode.token())
}

/// The `(item_id, a{audio})` PROGRAM of a session key: what it identifies apart
/// from the mode and the anchor. `None` for a malformed key. Split from the
/// RIGHT because an item id may itself contain a colon.
fn program_of(key: &str) -> Option<(&str, &str)> {
    let (head, audio) = key.rsplit_once(':')?; // "{item}:{mode}:{anchor}" + "a{audio}"
    let (head, _anchor) = head.rsplit_once(':')?;
    let (item, _mode) = head.rsplit_once(':')?;
    Some((item, audio))
}

/// Whether two session keys play the SAME program (same title, same muxed audio
/// track) and differ only in mode / anchor - i.e. one supersedes the other for
/// whichever client re-anchored (a seek) or toggled the audio filter. A
/// malformed key matches nothing. Used by the session registry to pick a victim
/// under the concurrency cap (see `Sessions::make_room`).
fn same_program(a: &str, b: &str) -> bool {
    match (program_of(a), program_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

pub struct HlsEngine {
    sessions: Arc<Sessions>,
    /// Cache of `abs_path -> true media duration (ms)`, so the on-demand ffprobe
    /// fallback (for catalog rows that were never probed) runs at most once per
    /// file. `None` = probed but no readable duration.
    durations: std::sync::Mutex<std::collections::HashMap<String, Option<u64>>>,
}

impl HlsEngine {
    /// `max_concurrent` hard-caps live sessions; `cache_budget` is the on-disk
    /// byte budget that trims idle / superseded sessions (0 = unlimited).
    pub fn new(data_dir: &Path, max_concurrent: usize, cache_budget: u64) -> Self {
        HlsEngine {
            sessions: Arc::new(Sessions::new(data_dir, max_concurrent, cache_budget)),
            durations: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// True media duration (ms) of the input file, ffprobed once (duration-only,
    /// header read) and cached. Lets the player show the real length when the
    /// catalog row was never probed - otherwise the growing EVENT playlist's
    /// duration is all the client can see (it reads the live edge as the total).
    pub async fn input_duration_ms(&self, input: &str) -> Option<u64> {
        if let Some(v) = self.durations.lock().unwrap().get(input).copied() {
            return v;
        }
        let path = input.to_string();
        let dur = tokio::task::spawn_blocking(move || {
            crate::infra::probe::probe_duration_ms(Path::new(&path))
        })
        .await
        .ok()
        .flatten();
        self.durations.lock().unwrap().insert(input.to_string(), dur);
        dur
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

    /// The media playlist for `item_id` in `mode`, anchored at `anchor` seconds
    /// (input `-ss`, 0 = start), muxing the `audio`-th audio track. Returns the
    /// playlist text + the REAL stream start (s) - the keyframe at-or-before
    /// `anchor` - for the client's `baseSec`.
    pub async fn master(&self, item_id: &str, input: &str, audio: u32, mode: StreamMode, anchor: u64) -> Option<(String, f64)> {
        let key = session_key(item_id, mode, anchor, audio);
        let (bytes, start) = self.sessions.master(&key, Path::new(input), audio, mode, anchor as f64).await?;
        Some((String::from_utf8(bytes).ok()?, start))
    }

    /// A child file (init or segment) of the `(mode, anchor, audio)` session.
    pub async fn file(&self, item_id: &str, mode: StreamMode, anchor: u64, audio: u32, name: &str) -> Option<(Vec<u8>, &'static str)> {
        let key = session_key(item_id, mode, anchor, audio);
        self.sessions.file(&key, name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_tokens_round_trip() {
        for mode in [StreamMode::Copy, StreamMode::Aac, StreamMode::AacStandard, StreamMode::AacNight] {
            assert_eq!(StreamMode::parse(mode.token()), Some(mode));
        }
        assert_eq!(StreamMode::parse("bogus"), None);
    }

    #[test]
    fn filtered_modes_transcode_with_a_chain() {
        assert!(!StreamMode::Copy.transcode());
        assert!(StreamMode::Aac.transcode());
        assert!(StreamMode::Aac.filter_chain().is_none());
        assert!(StreamMode::AacStandard.transcode());
        assert!(StreamMode::AacStandard.filter_chain().unwrap().contains("ratio=4"));
        assert!(StreamMode::AacNight.filter_chain().unwrap().contains("ratio=8"));
    }

    #[test]
    fn session_keys_keep_filtered_programs_apart() {
        let clean = session_key("it1", StreamMode::Aac, 30, 1);
        let night = session_key("it1", StreamMode::AacNight, 30, 1);
        assert_eq!(clean, "it1:aac:30:a1");
        assert_eq!(night, "it1:aac-night:30:a1");
        assert_ne!(clean, night);
    }

    #[test]
    fn same_program_spans_anchors_and_modes_only() {
        let key = session_key("it1", StreamMode::Aac, 30, 1);
        // A seek and an audio-filter toggle both mint a sibling of the SAME program.
        assert!(same_program(&key, &session_key("it1", StreamMode::Aac, 900, 1)));
        assert!(same_program(&key, &session_key("it1", StreamMode::AacNight, 30, 1)));
        // Another language track, another title and junk are all different programs.
        assert!(!same_program(&key, &session_key("it1", StreamMode::Aac, 30, 2)));
        assert!(!same_program(&key, &session_key("it2", StreamMode::Aac, 30, 1)));
        assert!(!same_program(&key, "nonsense"));
        assert!(!same_program("nonsense", "nonsense"));
    }

    #[test]
    fn program_of_tolerates_a_colon_in_the_item_id() {
        assert_eq!(program_of("tv:s1e2:aac-night:30:a1"), Some(("tv:s1e2", "a1")));
        assert_eq!(program_of("it1:copy:0:a0"), Some(("it1", "a0")));
        assert_eq!(program_of("bogus"), None);
    }
}
