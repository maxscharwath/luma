//! Typed response DTOs for endpoints whose JSON was previously assembled ad-hoc
//! with `serde_json::json!`. Modeling them as structs (a) makes the wire contract
//! a single source of truth shared with the TS clients via `#[derive(TS)]`, and
//! (b) removes a whole class of bug a mistyped JSON key that silently breaks a
//! client. `#[serde(rename_all = "camelCase")]` maps the snake_case Rust fields to
//! the camelCase the clients expect.

use serde::Serialize;
use ts_rs::TS;

use crate::infra::metrics::DiskInfo;
use crate::model::{AdminUser, MediaItem, Permission, Show, User};
use crate::services::settings::SettingGroup;

/// `GET /api/health`.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct Health {
    #[ts(type = "string")]
    pub status: &'static str,
    #[ts(type = "string")]
    pub version: &'static str,
    pub ffprobe: bool,
    pub libraries: usize,
    pub items: usize,
    pub shows: usize,
}

/// `POST /api/scan` result.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct ScanResult {
    pub scanned: usize,
    pub libraries: usize,
    pub shows: usize,
}

/// `{ token, user }` returned by register/login.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthResult {
    pub token: String,
    pub user: User,
}

/// `POST /api/invites` result the invite plus a ready-to-share join URL.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InviteCreated {
    pub token: String,
    /// `<web>/join?invite=…` when the server knows the web URL, else null.
    pub url: Option<String>,
    pub permissions: Vec<Permission>,
    pub expires_at: i64,
}

/// `POST /api/auth/quickconnect/initiate` a device-pairing request.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "QuickConnectInit")]
pub struct QuickConnectInit {
    /// Short numeric code shown on the device.
    pub code: String,
    /// Private handle the device polls with.
    pub secret: String,
    pub expires_in_sec: i64,
    /// Web URL to approve the code (for a QR), when the server knows it.
    pub authorize_url: Option<String>,
}

/// `GET /api/auth/quickconnect/poll` result a status-tagged union.
#[derive(Serialize, TS)]
#[serde(tag = "status", rename_all = "lowercase")]
#[ts(export, rename = "QuickConnectStatus")]
pub enum QuickPoll {
    Pending,
    Expired,
    Authorized { token: String, user: User },
}

/// Server identity + uptime for the admin sidebar status card.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServerInfo {
    pub name: String,
    pub hostname: String,
    #[ts(type = "string")]
    pub version: &'static str,
    pub uptime_sec: u64,
    pub online: bool,
    pub sessions: usize,
}

/// Cache directory usage + counts, nested in [`StorageInfo`].
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CacheInfo {
    pub dir: String,
    /// Total on-disk cache (transcode + images).
    pub bytes: u64,
    pub limit: String,
    /// On-disk size of the transcode segment cache.
    pub transcode_bytes: u64,
    /// On-disk size of the downloaded poster/backdrop/logo cache.
    pub images_bytes: u64,
    /// Number of cached image files (posters, backdrops, logos, stills).
    pub images_count: u64,
    /// Movies/loose videos that carry resolved TMDB metadata.
    pub enriched_items: u64,
    /// Shows that carry resolved TMDB metadata.
    pub enriched_shows: u64,
    /// Title embeddings stored for similar / themed / "For You" rows.
    pub embeddings: u64,
}

/// `GET /api/admin/storage`.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageInfo {
    pub volumes: Vec<DiskInfo>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub media_bytes: u64,
    pub cache: CacheInfo,
}

/// `GET /api/admin/users`.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminUsers {
    pub users: Vec<AdminUser>,
    pub library_count: usize,
}

/// A named, multi-folder library (`GET /api/admin/libraries`).
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminLibrary {
    pub id: String,
    pub name: String,
    /// `film` | `tv` | `music` | `photo`.
    pub kind: String,
    pub folders: Vec<String>,
    pub item_count: i64,
    pub size_bytes: i64,
    pub last_scan: Option<String>,
    pub auto_scan: bool,
}

/// One weekly bucket of the play-history chart.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryBucket {
    pub label: String,
    pub films_ms: i64,
    pub tv_ms: i64,
}

/// `GET /api/admin/stats/history`.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryStats {
    pub buckets: Vec<HistoryBucket>,
    pub total_films_ms: i64,
    pub total_tv_ms: i64,
}

/// `GET /api/admin/stats/overview`.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct AdminOverview {
    pub users: usize,
    pub online: usize,
    pub invites: usize,
    pub items: usize,
    pub shows: usize,
    pub libraries: usize,
}

/// `GET /api/admin/settings?view=…`.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct SettingsView {
    pub view: String,
    pub groups: Vec<SettingGroup>,
}

/// `GET /api/admin/llm` the multi-provider LLM configuration for the IA admin
/// page: the global enable flag, the id of the default provider used for
/// generation, and every configured provider (API keys never returned).
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LlmAdminConfig {
    pub enabled: bool,
    /// Id of the provider used for generation (falls back to the first).
    pub default_id: String,
    pub providers: Vec<LlmProviderView>,
}

/// One configured provider as shown to the admin the API key itself is never
/// returned, only whether one is set (`has_api_key`).
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LlmProviderView {
    pub id: String,
    pub name: String,
    /// `"openai"` (OpenAI-compatible / Ollama) | `"anthropic"` | `"openrouter"`.
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
    pub temperature: f32,
    pub max_tokens: i64,
    /// Anthropic adaptive thinking (Claude 4.6+).
    pub reasoning: bool,
}

/// One ranked result of `GET /api/search` a `type`-tagged union so the client
/// can switch on it (`movie`/`episode` carry a `MediaItem`, `show` a `Show`).
#[derive(Serialize, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
#[ts(export)]
pub enum SearchHit {
    Movie { item: MediaItem },
    Show { show: Show },
    Episode { item: MediaItem },
}

/// `GET /api/search?q=…` the echoed query plus hits in descending relevance.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchHit>,
}

/// `GET /api/people?name=…` every movie + show one person is credited in (cast
/// or key crew), best-known work first. Reuses [`SearchHit`] so clients render the
/// results with their existing card UI.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct PersonResponse {
    /// The matched person's name (echoed; original casing from the request).
    pub name: String,
    pub results: Vec<SearchHit>,
}
