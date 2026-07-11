//! Acquisition wire types: indexer config views, and the scored-release shape
//! interactive search returns. Pure data (serde + ts-rs); the engines live in
//! the `luma-torznab` / `luma-scene` workspace crates, orchestration in
//! `crate::services::acquisition`.

use serde::{Deserialize, Serialize};

/// One configured Torznab indexer, as listed to admins. The API key is
/// write-only (mirroring the remote-access token convention): clients only
/// learn whether one is set.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub has_api_key: bool,
    pub categories: Vec<u32>,
    pub enabled: bool,
    /// Flat score bonus in the decision engine (tiebreak between indexers).
    pub priority: i32,
    /// `torznab` (external Jackett/Prowlarr) or `builtin` (native Cardigann).
    pub kind: String,
    /// The Cardigann definition id (built-in indexers only).
    pub definition_id: Option<String>,
    /// Names of the settings that currently have a value (secrets never leave
    /// the server; the edit form re-renders the schema and blanks secrets).
    pub configured_settings: Vec<String>,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

/// `GET /api/admin/indexers`.
#[derive(Debug, Clone, Serialize)]
pub struct IndexersView {
    pub indexers: Vec<IndexerView>,
}

/// `POST /api/admin/indexers` / `PUT /api/admin/indexers/:id` body. Omitted
/// fields keep their current value on update; an omitted `api_key` keeps the
/// stored secret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveIndexerBody {
    pub name: Option<String>,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub categories: Option<Vec<u32>>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    /// `builtin` to create a native-Cardigann indexer (default `torznab`).
    #[serde(default)]
    pub kind: Option<String>,
    /// The Cardigann definition id (built-in create).
    #[serde(default)]
    pub definition_id: Option<String>,
    /// Per-indexer settings (credentials + toggles). Merged into the stored
    /// map on update; an omitted secret keeps its stored value.
    #[serde(default)]
    pub settings: Option<std::collections::HashMap<String, String>>,
}

// ----- built-in definition catalog ------------------------------------------------

/// One Cardigann definition in the admin's browse list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionView {
    pub id: String,
    pub name: String,
    /// `public` | `private` | `semi-private`.
    pub kind: String,
    pub description: String,
    pub links: Vec<String>,
}

/// `GET /api/admin/indexers/definitions`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionsView {
    pub definitions: Vec<IndexerDefinitionView>,
    /// Whether the definition set has been fetched yet.
    pub synced: bool,
}

/// One configurable setting of a definition, for rendering the add form.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionSettingView {
    pub name: String,
    /// `text` | `password` | `checkbox` | `select` | `info`.
    pub kind: String,
    pub label: String,
    pub default: Option<String>,
    /// For `select`: ordered (value, label) pairs.
    pub options: Vec<(String, String)>,
}

/// `GET /api/admin/indexers/definitions/:id` — the schema needed to add it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerDefinitionDetailView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub description: String,
    pub links: Vec<String>,
    pub settings: Vec<IndexerDefinitionSettingView>,
}

/// `POST /api/admin/indexers/definitions/sync` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDefinitionsResult {
    pub count: usize,
    pub version: String,
}

/// `POST /api/admin/indexers/:id/test` result (a `t=caps` round-trip).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub server_title: Option<String>,
    /// Whether the indexer resolves TMDB ids (movie / tv search).
    pub supports_tmdb: bool,
    pub error: Option<String>,
}

/// One score-explanation line (mirrors `luma_scene::ScoreLine` on the wire).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreLineView {
    pub rule: String,
    pub delta: i32,
    pub note: String,
}

/// One release from an interactive search, scored (or rejected with the rule
/// that fired). Sorted accepted-first, best score first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredReleaseView {
    pub title: String,
    pub guid: String,
    pub indexer_id: String,
    pub indexer_name: String,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub published_at: Option<String>,
    /// What the grab would target (`movie` | `episode` | `season`).
    pub target: String,
    pub season: Option<u32>,
    /// Episode numbers a season-pack grab would cover.
    pub episodes: Option<Vec<u32>>,
    pub score: Option<i32>,
    pub breakdown: Vec<ScoreLineView>,
    /// Rejection rule + note when the decision engine refused it.
    pub rejected: Option<String>,
    /// Whether the release carries something grabbable (magnet or .torrent URL).
    pub grabbable: bool,
    /// The tracker's torrent page (for a "view on the tracker" link).
    pub details_url: Option<String>,
}

/// `GET /api/requests/:id/search`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveSearchView {
    pub releases: Vec<ScoredReleaseView>,
    /// Indexers that errored during the sweep (name -> message), so an empty
    /// list is distinguishable from a broken indexer.
    pub indexer_errors: Vec<String>,
}

/// `POST /api/requests/:id/grab` body: pick one release from the last
/// interactive search (identified the way the search listed it).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrabBody {
    pub guid: String,
    pub indexer_id: String,
}

/// One configured download client, as listed to admins (password write-only).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadClientView {
    pub id: String,
    /// `rqbit` | `transmission` | `qbittorrent`.
    pub kind: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub has_password: bool,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: i64,
    /// The embedded engine row cannot be deleted (it is seeded by the build).
    pub builtin: bool,
}

/// `GET /api/admin/download-clients`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadClientsView {
    pub clients: Vec<DownloadClientView>,
    /// Whether the embedded engine is compiled into this build.
    pub rqbit_compiled: bool,
}

/// Create/update body for a download client.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDownloadClientBody {
    pub kind: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub username: Option<String>,
    /// Omitted/empty keeps the stored secret.
    pub password: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

/// `POST /api/admin/download-clients/:id/test` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientTestResult {
    pub ok: bool,
    /// Human version string ("Transmission 4.0.5").
    pub version: Option<String>,
    pub error: Option<String>,
}

/// One download (grab), as listed in the admin queue. Live speed/ETA ride the
/// `download.progress` WS event; this is the durable row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub request_id: Option<String>,
    /// `movie` | `episode` | `season`.
    pub kind: String,
    pub title: String,
    pub release_title: String,
    pub season: Option<u32>,
    pub episodes: Option<Vec<u32>>,
    /// `queued` | `downloading` | `seeding` | `completed` | `imported` |
    /// `failed` | `removed` | `paused`.
    pub status: String,
    pub progress: f64,
    pub size_bytes: Option<u64>,
    pub score: Option<i32>,
    pub error: Option<String>,
    pub grabbed_at: i64,
    pub completed_at: Option<i64>,
    pub imported_at: Option<i64>,
    /// Which indexer this was grabbed from (display name), when known.
    pub indexer_name: Option<String>,
    /// The tracker's torrent page, for a "view on the tracker" link.
    pub details_url: Option<String>,
    /// The release's info hash (identifies the exact torrent).
    pub info_hash: Option<String>,
    /// Poster art (from the linked request), for the queue thumbnail.
    pub poster_url: Option<String>,
    /// The catalog item id when the title is already in the library (imported),
    /// so the queue can link to its LUMA detail page. `None` until imported.
    pub local_id: Option<String>,
}

/// `GET /api/admin/downloads`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsView {
    pub downloads: Vec<DownloadView>,
    /// VPN seal status (None until a proxy is configured). Fleshed out with
    /// the kill-switch milestone.
    pub vpn: Option<VpnStatusView>,
}

/// One release from a free-text manual indexer search. Not scored against a
/// specific target (the admin picks); carries parsed quality + the link to
/// grab it, and the parse hints so the add form can pre-fill.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualReleaseView {
    pub title: String,
    pub guid: String,
    pub indexer_name: String,
    /// Magnet or `.torrent` link to hand to the add endpoint.
    pub download_url: Option<String>,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub published_at: Option<String>,
    /// Parsed hints (for display + pre-filling the add form).
    pub resolution: Option<String>,
    pub codec: Option<String>,
    pub source: Option<String>,
    pub parsed_title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub full_season: bool,
    /// The tracker's torrent page (for a "view on the tracker" link).
    pub details_url: Option<String>,
}

/// `POST /api/admin/downloads/search`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSearchView {
    pub releases: Vec<ManualReleaseView>,
    pub indexer_errors: Vec<String>,
}

/// `POST /api/admin/downloads/search` body.
#[derive(Debug, Clone, Deserialize)]
pub struct ManualSearchBody {
    pub query: String,
}

/// `POST /api/admin/downloads/add` body: grab a magnet / `.torrent` URL (pasted
/// or from a manual search) and import it as `kind` into the right library.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualAddBody {
    pub magnet_or_url: String,
    /// `movie` | `episode` | `season`.
    pub kind: String,
    /// Import title (movie or show title). Required for correct naming; when
    /// empty the release name is parsed at import time.
    pub title: Option<String>,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub tmdb_id: Option<u64>,
    /// Download only these torrent file indices (from an analysis). `None`/empty
    /// = the whole torrent.
    #[serde(default)]
    pub only_files: Option<Vec<usize>>,
    /// The tracker's torrent page (carried from a manual-search pick).
    #[serde(default)]
    pub details_url: Option<String>,
}

/// One file inside an analyzed torrent, with its detected season/episode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFileView {
    pub index: usize,
    pub path: String,
    pub size_bytes: u64,
    pub is_video: bool,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// `POST /api/admin/downloads/analyze` the torrent's file list + what it holds,
/// so the admin can pick episodes / confirm the kind before grabbing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentAnalysis {
    /// `movie` | `episode` | `season` | `series` | `unknown`.
    pub kind: String,
    pub seasons: Vec<u32>,
    pub files: Vec<TorrentFileView>,
}

/// `POST /api/admin/downloads/analyze` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeBody {
    pub magnet_or_url: String,
}

/// The kill switch's view of the tunnel. Cross-boundary (the downloads kill
/// switch produces it; the VPN admin view + the downloads-queue view embed it),
/// so it stays here until the download DTOs leave core, then it moves to
/// luma-contracts. VpnAdminView / SaveVpnBody / VpnTestResult moved to luma-vpn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatusView {
    pub connected: bool,
    pub exit_ip: Option<String>,
    /// Downloads are currently held by the kill switch.
    pub paused: bool,
}
