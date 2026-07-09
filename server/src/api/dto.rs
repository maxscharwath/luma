//! Typed response DTOs for endpoints whose JSON was previously assembled ad-hoc
//! with `serde_json::json!`. Modeling them as structs (a) makes the wire contract
//! a single source of truth (the TS clients mirror it via the zod schemas in
//! packages/core), and (b) removes a whole class of bug a mistyped JSON key that
//! silently breaks a client. `#[serde(rename_all = "camelCase")]` maps the
//! snake_case Rust fields to the camelCase the clients expect.

use serde::Serialize;

use crate::infra::metrics::DiskInfo;
use crate::model::{AdminUser, MediaItem, Permission, Show, User};
use crate::services::settings::SettingGroup;

/// `GET /api/health`.
#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub ffprobe: bool,
    pub libraries: usize,
    pub items: usize,
    pub shows: usize,
}

/// `{ token, accessToken, user }` returned by register/login. `token` is the
/// short-lived session bearer; `accessToken` is the long-lived device credential
/// the client stores and later exchanges (see `/auth/token`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResult {
    pub token: String,
    pub access_token: String,
    pub user: User,
}

/// `{ token, user }` from `/auth/token` a fresh session minted from an access
/// token (the access token itself is unchanged, so it isn't echoed back).
#[derive(Serialize)]
pub struct SessionResult {
    pub token: String,
    pub user: User,
}

/// One signed-in device in `GET /auth/me/sessions`. `id` is a non-secret handle
/// (a short hash of the device's access token) used to revoke it; `current`
/// marks the device making the request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    /// The device's captured User-Agent, if any (the client derives a label).
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_seen: Option<String>,
    pub current: bool,
}

/// One registered passkey in `GET /auth/me/passkeys`. `id` is the credential's
/// stable handle (used to revoke it).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used: Option<String>,
}

/// `GET /api/auth/config` the public login-gate configuration, read before any
/// credential so the client knows what to render.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    /// Whether the account roster is public (the "Qui regarde ?" profile picker).
    /// Off by default: hiding it means knowing the URL no longer lists accounts.
    pub public_user_list: bool,
    /// Whether any account exists yet. `false` → the first-run owner registration
    /// flow; `true` → sign in.
    pub has_accounts: bool,
}

/// `POST /api/invites` result the invite plus a ready-to-share join URL.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteCreated {
    pub token: String,
    /// `<web>/join?invite=…` when the server knows the web URL, else null.
    pub url: Option<String>,
    pub permissions: Vec<Permission>,
    pub expires_at: i64,
}

/// `POST /api/auth/quickconnect/initiate` a device-pairing request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum QuickPoll {
    Pending,
    Expired,
    #[serde(rename_all = "camelCase")]
    Authorized {
        token: String,
        access_token: String,
        user: User,
    },
}

/// Server identity + uptime for the admin sidebar status card.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub hostname: String,
    pub version: &'static str,
    pub uptime_sec: u64,
    pub online: bool,
    pub sessions: usize,
}

/// Cache directory usage + counts, nested in [`StorageInfo`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub dir: String,
    /// Total on-disk cache (transcode + images).
    pub bytes: u64,
    pub limit: String,
    /// On-disk size of the transcode segment cache.
    pub transcode_bytes: u64,
    /// Byte budget for the transcode cache (the `transcodeCacheLimit` label).
    pub transcode_limit: String,
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub volumes: Vec<DiskInfo>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub media_bytes: u64,
    pub cache: CacheInfo,
}

/// `GET /api/admin/users`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUsers {
    pub users: Vec<AdminUser>,
    pub library_count: usize,
}

/// A named, multi-folder library (`GET /api/admin/libraries`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBucket {
    pub label: String,
    pub films_ms: i64,
    pub tv_ms: i64,
}

/// `GET /api/admin/stats/history`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStats {
    pub buckets: Vec<HistoryBucket>,
    pub total_films_ms: i64,
    pub total_tv_ms: i64,
}

/// `GET /api/admin/stats/overview`.
#[derive(Serialize)]
pub struct AdminOverview {
    pub users: usize,
    pub online: usize,
    pub invites: usize,
    pub items: usize,
    pub shows: usize,
    pub libraries: usize,
}

/// `GET /api/admin/settings?view=…`.
#[derive(Serialize)]
pub struct SettingsView {
    pub view: String,
    pub groups: Vec<SettingGroup>,
}

/// `GET /api/admin/llm` the multi-provider LLM configuration for the IA admin
/// page: the global enable flag, the id of the default provider used for
/// generation, and every configured provider (API keys never returned).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmAdminConfig {
    pub enabled: bool,
    /// Id of the provider used for generation (falls back to the first).
    pub default_id: String,
    pub providers: Vec<LlmProviderView>,
}

/// One configured provider as shown to the admin the API key itself is never
/// returned, only whether one is set (`has_api_key`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SearchHit {
    Movie { item: MediaItem },
    Show { show: Show },
    Episode { item: MediaItem },
}

/// `GET /api/search?q=…` the echoed query plus hits in descending relevance.
#[derive(Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchHit>,
}

/// `GET /api/people?name=…` every movie + show one person is credited in (cast
/// or key crew), best-known work first. Reuses [`SearchHit`] so clients render the
/// results with their existing card UI.
#[derive(Serialize)]
pub struct PersonResponse {
    /// The matched person's name (echoed; original casing from the request).
    pub name: String,
    pub results: Vec<SearchHit>,
}
