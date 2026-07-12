//! Acquisition wire types: the interactive-search scoring shape, the manual
//! search / add bodies, and the torrent-analysis views. Pure data (serde); the
//! download-queue / client / organize / VPN views stay in `luma-torrent`.

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

/// `POST /api/admin/acquisition/search`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSearchView {
    pub releases: Vec<ManualReleaseView>,
    pub indexer_errors: Vec<String>,
}

/// `POST /api/admin/acquisition/search` body.
#[derive(Debug, Clone, Deserialize)]
pub struct ManualSearchBody {
    pub query: String,
}

/// `POST /api/admin/acquisition/add` body: grab a magnet / `.torrent` URL (pasted
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

/// `POST /api/admin/acquisition/analyze` the torrent's file list + what it holds,
/// so the admin can pick episodes / confirm the kind before grabbing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentAnalysis {
    /// `movie` | `episode` | `season` | `series` | `unknown`.
    pub kind: String,
    pub seasons: Vec<u32>,
    pub files: Vec<TorrentFileView>,
}

/// `POST /api/admin/acquisition/analyze` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeBody {
    pub magnet_or_url: String,
}
