//! Core data model. The JSON shape here is a public contract — web/TV clients
//! depend on it, so field names and casing must not drift.

use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;

/// What sort of thing a media item is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Movie,
    Episode,
    Video,
}

/// Library classification, derived from the kinds of items it holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LibraryKind {
    Movies,
    Shows,
    Mixed,
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
    /// `short_hash(abs_path)` — stable per physical file.
    pub id: String,
    #[serde(rename = "relPath")]
    pub rel_path: Option<String>,
    pub container: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<u64>,
    pub video: Option<VideoStream>,
    /// Representative (first/default) audio track — kept for codec badges and
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
    /// Absolute path on disk. Internal only — never serialized to clients.
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
    /// Representative (first/default) audio track — kept for badges and
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
    /// Absolute path on disk. Internal only — never serialized to clients.
    /// Mirrors the default/representative file's path so `/stream` keeps working.
    #[serde(skip)]
    pub abs_path: Option<String>,
    /// Every physical file backing this logical item.
    #[serde(default)]
    pub files: Vec<MediaFile>,
    /// Id of the representative ("default") file — the one `/stream` serves and
    /// whose stream info populates the top-level fields. `None` until at least
    /// one file exists.
    #[serde(rename = "defaultFileId", default, skip_serializing_if = "Option::is_none")]
    pub default_file_id: Option<String>,
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
}

/// One season's worth of episodes, sorted by episode number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub number: u32,
    pub episodes: Vec<MediaItem>,
}

/// `GET /api/shows/:id` payload: a show plus its seasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowDetail {
    pub show: Show,
    pub seasons: Vec<Season>,
}

/// A user account. `password_hash` lives only in the DB layer and is never part
/// of this (serialized) shape, so a `User` is always safe to send to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Preferred UI locale (`"fr"` | `"en"`), synced across this account's
    /// devices. `None` → clients fall back to the device/browser locale. See the
    /// shared i18n catalogs (`packages/core/src/locales`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Granted permissions (capability-based, no roles). The first registered
    /// account (owner) gets every permission; the rest default to `[Playback]`.
    /// Clients unlock pages/actions from this set (admin panel, future stats…).
    pub permissions: Vec<Permission>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

impl User {
    /// Whether this user holds a given permission. Gates the invite/admin
    /// endpoints via [`crate::api::users`]'s `require`.
    pub fn can(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }
}

/// A granular capability. Stored on each user as a JSON array of the string keys
/// below. Extend this enum (and the TS mirror in `@luma/core`) to add more —
/// e.g. a `stats.view` for the upcoming stats pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// Manage user accounts (the admin panel).
    #[serde(rename = "users.manage")]
    UsersManage,
    /// Manage libraries, scans and metadata.
    #[serde(rename = "library.manage")]
    LibraryManage,
    /// Manage server settings.
    #[serde(rename = "settings.manage")]
    SettingsManage,
    /// Watch the catalogue and save playback progress (default for everyone).
    #[serde(rename = "playback")]
    Playback,
}

impl Permission {
    /// Parse a stored key; `None` for unknown keys (tolerant forward-compat).
    pub fn parse(s: &str) -> Option<Permission> {
        match s {
            "users.manage" => Some(Permission::UsersManage),
            "library.manage" => Some(Permission::LibraryManage),
            "settings.manage" => Some(Permission::SettingsManage),
            "playback" => Some(Permission::Playback),
            _ => None,
        }
    }

    /// Every permission — granted to the owner account.
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::UsersManage,
            Permission::LibraryManage,
            Permission::SettingsManage,
            Permission::Playback,
        ]
    }
}

/// The publicly-listable subset of a user, surfaced by `GET /api/users` to
/// populate the "Qui regarde ?" profile picker (no email).
#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// A registration invitation created by a user with `users.manage`. After the
/// bootstrap owner, an invite is the only way to create an account.
#[derive(Debug, Clone, Serialize)]
pub struct Invite {
    pub token: String,
    /// Permissions the invited account will be granted.
    pub permissions: Vec<Permission>,
    #[serde(rename = "createdBy", skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Unix-seconds expiry.
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    pub used: bool,
}

/// One row of a user's playback progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// A "continue watching" entry: the resumable item plus where to resume from.
#[derive(Debug, Clone, Serialize)]
pub struct ContinueItem {
    pub item: MediaItem,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// A scanned library root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub kind: LibraryKind,
    pub path: String,
    #[serde(rename = "itemCount")]
    pub item_count: usize,
}

/// Derive a display role label from a capability set. The backend is
/// capability-based; this is purely for the admin UI's "Rôle" badge.
pub fn role_label(perms: &[Permission]) -> &'static str {
    if perms.contains(&Permission::UsersManage) && perms.contains(&Permission::SettingsManage) {
        "Propriétaire"
    } else if perms.contains(&Permission::Playback) {
        "Membre"
    } else {
        "Restreint"
    }
}

/// One account as surfaced to the admin "Membres & partage" table. Unlike
/// [`User`] this carries the email, a derived role, last-activity and a live
/// `online` flag (set at request time from the playback registry).
#[derive(Debug, Clone, Serialize)]
pub struct AdminUser {
    pub id: String,
    pub email: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub permissions: Vec<Permission>,
    /// Derived display role: "Propriétaire" | "Membre" | "Restreint".
    pub role: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastSeen", skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// Whether the user is currently streaming (filled from the playback registry).
    pub online: bool,
}

/// Aggregated per-user watch stats over a window (the dashboard "Top des
/// utilisateurs" cards).
#[derive(Debug, Clone, Serialize)]
pub struct TopUser {
    pub username: String,
    pub plays: i64,
    #[serde(rename = "watchedMs")]
    pub watched_ms: i64,
    #[serde(rename = "filmsMs")]
    pub films_ms: i64,
    #[serde(rename = "tvMs")]
    pub tv_ms: i64,
}

/// One raw play-history record (used to bucket the weekly "Historique de
/// lecture" chart server-side).
#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub ended_at: i64,
    pub kind: Kind,
    pub watched_ms: i64,
}

/// Per-library aggregate (item count + total bytes on disk) for the storage and
/// libraries admin pages.
#[derive(Debug, Clone)]
pub struct LibraryStat {
    pub id: String,
    pub item_count: i64,
    pub total_bytes: i64,
}
