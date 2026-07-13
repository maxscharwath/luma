//! Download wire types: the download queue + client config views, the naming /
//! organize shapes, and the VPN kill-switch status. Pure data (serde); relocated
//! here from the core `luma-domain` crate so the module that owns them also owns
//! their contract. The acquisition search / grab / analyze DTOs live in the
//! `luma-acquisition` crate now.

use serde::{Deserialize, Serialize};

use luma_module_sdk::ports::VpnStatusView;

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

/// The kill switch's view of the tunnel. Cross-boundary within the acquisition
/// stack: the downloads kill switch produces it, and both the VPN admin view
// VpnStatusView moved to luma-contracts (the download manager's VPN surface is a
// port now); re-exported at this crate's root for the module's own callers.

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
