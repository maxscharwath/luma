//! Download / acquisition wire types: the interactive-search scoring shape,
//! the download queue + client config views, manual search/add bodies, and the
//! VPN kill-switch status. Pure data (serde); relocated here from the core
//! `luma-domain` crate so the module that owns them also owns their contract.

use serde::{Deserialize, Serialize};

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

/// The kill switch's view of the tunnel. Cross-boundary within the acquisition
/// stack: the downloads kill switch produces it, and both the VPN admin view
/// (`luma_vpn::VpnAdminView`) and the downloads-queue view embed it. It lives
/// here because `luma-vpn` depends on `luma-torrent` (not the reverse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatusView {
    pub connected: bool,
    pub exit_ip: Option<String>,
    /// Downloads are currently held by the kill switch.
    pub paused: bool,
}

// --- Library rename tool (organize) wire types ---
// Relocated from the core `luma-domain` crate: the organize engine lives in
// `crate::organize`, so the module that owns it owns its contract too.

/// The five naming templates (Sonarr/Radarr-style token strings) plus the
/// global case transform applied to every rendered filename.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingTemplatesView {
    pub movie_folder: String,
    pub movie_file: String,
    pub series_folder: String,
    pub season_folder: String,
    pub episode_file: String,
    /// `default` | `upper` | `lower`.
    pub case: String,
}

/// `GET /api/admin/organize/naming` current templates + a rendered sample.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingView {
    pub templates: NamingTemplatesView,
    pub sample: SampleNames,
}

/// Example rendered names for the live preview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleNames {
    /// e.g. `The Matrix (1999)/The Matrix (1999) Bluray-1080p.mkv`
    pub movie: String,
    /// e.g. `Breaking Bad (2008)/Season 01/Breaking Bad - S01E02 - ... .mkv`
    pub episode: String,
}

/// `POST /api/admin/organize/sample` body (render as the admin types).
pub type SampleBody = NamingTemplatesView;

/// One file the rename tool would move.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeMove {
    pub title: String,
    /// `movie` | `episode`.
    pub kind: String,
    /// Current path, relative to its library folder.
    pub from: String,
    /// Expected path, relative to its library folder.
    pub to: String,
}

/// `GET /api/admin/organize/preview`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub moves: Vec<OrganizeMove>,
    /// Total library files considered.
    pub total_files: u32,
    /// Files already matching the templates.
    pub matching: u32,
}

/// `POST /api/admin/organize/apply` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeResult {
    pub moved: u32,
    pub failed: u32,
    pub errors: Vec<String>,
}
