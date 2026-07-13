//! The download-ledger contract: the grab spec + stored download row, plus the
//! DownloadGrabPort (grab / gate / activate / drop) and DownloadDbPort (the
//! ledger reads/writes acquisition's import needs), so acquisition doesn't depend
//! on the torrents crate.

use luma_module_host::HostCtx;

use super::TorrentFileEntry;

/// Everything needed to grab a torrent + import it. Built from a scored release
/// (auto / interactive) or from admin-provided fields (manual add / magnet).
#[derive(Debug, Clone, Default)]
pub struct GrabSpec {
    pub magnet_or_url: String,
    /// `movie` | `episode` | `season`.
    pub kind: String,
    pub tmdb_id: u64,
    /// Import title (`None` => derive from the release name at import time).
    pub title: Option<String>,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episodes: Option<Vec<u32>>,
    pub release_title: String,
    pub indexer_id: Option<String>,
    pub size_bytes: Option<u64>,
    pub score: Option<i32>,
    pub score_breakdown: Option<String>,
    pub request_id: Option<String>,
    /// Wanted rows this grab covers (flip to `grabbed`); empty for manual adds.
    pub wanted_ids: Vec<String>,
    /// Download only these file indices (Sonarr/Radarr-style selection). `None`
    /// = the whole torrent.
    pub only_files: Option<Vec<usize>>,
    /// The tracker's torrent page, stored so the queue can link back to it.
    pub details_url: Option<String>,
}

/// A stored download row.
#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub id: String,
    pub client_id: String,
    /// The engine's identifier (info-hash hex).
    pub client_ref: String,
    pub request_id: Option<String>,
    /// `movie` | `episode` | `season`.
    pub kind: String,
    pub tmdb_id: u64,
    /// Display / import title (denormalized so a manual grab imports without a
    /// request). `None` = fall back to parsing the release title.
    pub title: Option<String>,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episodes: Option<Vec<u32>>,
    pub release_title: String,
    pub indexer_id: Option<String>,
    pub info_hash: Option<String>,
    pub magnet_or_url: String,
    pub size_bytes: Option<u64>,
    pub score: Option<i32>,
    pub score_breakdown: Option<String>,
    pub status: String,
    pub progress: f64,
    pub save_path: Option<String>,
    /// Library files written by the import (persisted for the record / future
    /// "reveal in library"; not surfaced in a view yet).
    #[allow(dead_code)]
    pub imported_paths: Option<Vec<String>>,
    pub error: Option<String>,
    pub grabbed_at: i64,
    pub completed_at: Option<i64>,
    pub imported_at: Option<i64>,
    /// The tracker's human-viewable torrent page (Sonarr/Radarr's info link).
    pub details_url: Option<String>,
    /// Selected torrent file indices for a partial grab (`None` = whole torrent).
    /// Persisted so the background add (`crate::downloads`) keeps the selection
    /// even though it runs after the request returned.
    pub only_files: Option<Vec<usize>>,
}

/// The download manager's grab + lifecycle surface, resolved by acquisition.
pub trait DownloadGrabPort: Send + Sync {
    /// Grab a release: record the ledger row and hand it to the engine.
    fn grab(&self, host: &dyn HostCtx, spec: GrabSpec) -> anyhow::Result<DownloadRow>;
    /// List a torrent's files (metadata only, no download) so the admin can
    /// analyze + select before grabbing.
    fn list_files(
        &self,
        host: &dyn HostCtx,
        magnet_or_url: &str,
    ) -> anyhow::Result<Vec<TorrentFileEntry>>;
    /// Whether the kill switch currently allows new grabs.
    fn gate_open(&self) -> bool;
    /// Kick a freshly-recorded row into the engine (background add).
    fn activate(&self, host: &dyn HostCtx, row: &DownloadRow);
    /// Free a download's data + stop seeding (post-import cleanup).
    fn drop_data(&self, host: &dyn HostCtx, row: &DownloadRow);
}

/// The downloads-ledger reads/writes acquisition's import pass needs.
pub trait DownloadDbPort: Send + Sync {
    fn completed_downloads(&self, host: &dyn HostCtx) -> anyhow::Result<Vec<DownloadRow>>;
    fn mark_download_imported(
        &self,
        host: &dyn HostCtx,
        id: &str,
        paths: &[String],
        now_ms: i64,
    ) -> anyhow::Result<()>;
    fn set_download_status(
        &self,
        host: &dyn HostCtx,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> anyhow::Result<bool>;
}
