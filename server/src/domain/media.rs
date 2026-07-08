//! Media catalog types: kinds, stream descriptions, files, items and shows.
//!
//! The JSON shape here is a public contract web/TV clients depend on it, so
//! field names and casing must not drift.

use serde::{Deserialize, Serialize};

use crate::domain::metadata::{CastMember, Metadata};

/// What sort of thing a media item is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Movie,
    Episode,
    Video,
}

/// Video stream description (best-effort; fields may be null when unknown).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStream {
    pub codec: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub hdr: bool,
    #[serde(rename = "bitDepth")]
    pub bit_depth: Option<u32>,
}

/// One audio stream/track. An item can carry several (e.g. EN + FR, or a
/// director's commentary); `index` is the **audio-relative** position (0-based
/// among audio streams only), which is exactly what ffmpeg's `-map 0:a:<index>`
/// selector expects when remuxing a chosen track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    /// Audio-relative stream index (0 = first audio track). Drives track
    /// selection (`-map 0:a:<index>`) on the server's per-track HLS remux.
    #[serde(default)]
    pub index: u32,
    pub codec: String,
    pub channels: Option<u32>,
    pub language: Option<String>,
    /// Human label from the stream's `title` tag ("Commentary", "Director's
    /// Cut", …), when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Whether the container marks this as the default audio track.
    #[serde(default)]
    pub default: bool,
}

/// A subtitle track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub language: Option<String>,
    pub codec: String,
}

/// One physical file backing a logical [`MediaItem`]. A single item can have
/// several of these (Director's Cut + Theatrical, 1080p + 4K, …); they all share
/// the same logical item id but each maps to a distinct file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    /// `short_hash(abs_path)` stable per physical file.
    pub id: String,
    #[serde(rename = "relPath")]
    pub rel_path: Option<String>,
    pub container: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<u64>,
    pub video: Option<VideoStream>,
    /// Representative (first/default) audio track kept for codec badges and
    /// backward compatibility with clients that read `audio.codec` directly.
    pub audio: Option<AudioStream>,
    /// Every audio track on this file, in container order. Drives the player's
    /// audio-track picker. Empty when unprobed or for pre-`audioTracks` rows.
    #[serde(rename = "audioTracks", default)]
    pub audio_tracks: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleTrack>,
    pub size: Option<u64>,
    /// Best-effort label parsed from the filename ("Director's Cut", "Extended",
    /// "Remux", "4K", "1080p", …). `None` when nothing notable is detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    /// Whether ffprobe has run on this file yet (phase 2). Until then the stream
    /// fields above are null.
    pub probed: bool,
    /// Absolute path on disk. Internal only never serialized to clients.
    #[serde(skip)]
    pub abs_path: Option<String>,
}

/// A single playable media item.
///
/// `rel_path` is relative to the owning media directory. Demo/seed items have
/// `rel_path == None` and cannot be streamed.
///
/// An item can be backed by multiple physical [`MediaFile`]s. The top-level
/// `video`/`audio`/`duration_ms`/`container`/`subtitles`/`abs_path` fields mirror
/// the **representative file** (the highest-resolution probed file) for backward
/// compatibility with clients that read `item.video.codec` directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub year: Option<u32>,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<u64>,
    pub container: String,
    pub video: Option<VideoStream>,
    /// Representative (first/default) audio track kept for badges and
    /// backward compatibility. The full list is `audio_tracks`.
    pub audio: Option<AudioStream>,
    /// Every audio track of the representative file, for the audio-track picker.
    #[serde(rename = "audioTracks", default)]
    pub audio_tracks: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleTrack>,
    pub library: String,
    // --- show / episode grouping (null for movies) ---
    #[serde(rename = "showId")]
    pub show_id: Option<String>,
    #[serde(rename = "showTitle")]
    pub show_title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// Last episode number for multi-episode files (`S01E02-E03`).
    #[serde(rename = "episodeEnd")]
    pub episode_end: Option<u32>,
    #[serde(rename = "episodeTitle")]
    pub episode_title: Option<String>,
    #[serde(rename = "relPath")]
    pub rel_path: Option<String>,
    #[serde(rename = "addedAt")]
    pub added_at: String,
    /// TMDB catalog metadata (poster/backdrop/overview/IDs). `None` until the
    /// background enrichment pass resolves it. Movies only; episodes inherit
    /// their show's metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Absolute path on disk. Internal only never serialized to clients.
    /// Mirrors the default/representative file's path so `/stream` keeps working.
    #[serde(skip)]
    pub abs_path: Option<String>,
    /// Every physical file backing this logical item.
    #[serde(default)]
    pub files: Vec<MediaFile>,
    /// Id of the representative ("default") file the one `/stream` serves and
    /// whose stream info populates the top-level fields. `None` until at least
    /// one file exists.
    #[serde(rename = "defaultFileId", default, skip_serializing_if = "Option::is_none")]
    pub default_file_id: Option<String>,
    /// Intro / credits segment markers (episodes only). Drives the "skip intro"
    /// button and the credits-triggered "next episode" card. Empty until resolved
    /// from chapters or the audio-fingerprint job.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
}

/// What a [`Marker`] segment is. Serialized lowercase (`"intro"` / `"credits"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerKind {
    Intro,
    Credits,
}

/// One timed segment of an episode (intro / credits), in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub kind: MarkerKind,
    #[serde(rename = "startMs")]
    pub start_ms: u64,
    #[serde(rename = "endMs")]
    pub end_ms: u64,
}

/// A TV show aggregate (not a file). Built by grouping episodes during a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Show {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub library: String,
    #[serde(rename = "seasonCount")]
    pub season_count: u32,
    #[serde(rename = "episodeCount")]
    pub episode_count: u32,
    /// Representative video info (from an episode) for quality badges.
    pub video: Option<VideoStream>,
    #[serde(rename = "addedAt")]
    pub added_at: String,
    /// TMDB catalog metadata (poster/backdrop/overview/IDs). `None` until the
    /// background enrichment pass resolves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Per-user series-completion percent (0–100), filled by the catalogue
    /// endpoints when the request is authenticated. `None` for anonymous requests
    /// or shows with no progress drives the progress bar on show cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
}

/// One season's worth of episodes, sorted by episode number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub number: u32,
    pub episodes: Vec<MediaItem>,
    /// Season-specific cast (TMDB season credits), resolved during enrichment.
    /// Empty until enriched or when the provider returned none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<CastMember>,
}

/// `GET /api/shows/:id` payload: a show plus its seasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowDetail {
    pub show: Show,
    pub seasons: Vec<Season>,
}
