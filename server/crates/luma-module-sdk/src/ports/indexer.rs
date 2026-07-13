//! The indexer data + built-in-search contract: the shared indexer row and the
//! ports the downloads / acquisition modules resolve so they don't depend on the
//! indexer crate. The search query/result types are the Torznab ones (see
//! `torznab`), which the indexer's native engine mirrors 1:1.

use luma_module_host::HostCtx;

use super::{Query, Release};

/// A stored indexer row (full, including the secret; internal only). Owned by the
/// indexer module's `indexers` table but shared so the downloads queue view and
/// the acquisition search can name it without depending on the indexer crate.
#[derive(Debug, Clone)]
pub struct IndexerRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub categories: Vec<u32>,
    pub enabled: bool,
    pub priority: i32,
    /// `torznab` (external Jackett/Prowlarr) or `builtin` (native Cardigann).
    pub kind: String,
    /// The Cardigann definition id (file stem) for `builtin` rows.
    pub definition_id: Option<String>,
    /// JSON map of per-indexer settings (credentials + toggles) for `builtin`.
    pub settings: String,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

/// Read/update the shared `indexers` table. Implemented by the indexer module and
/// resolved by the downloads module (queue view) and acquisition.
pub trait IndexerDbPort: Send + Sync {
    fn list_indexers(&self, host: &dyn HostCtx) -> anyhow::Result<Vec<IndexerRow>>;
    fn enabled_indexers(&self, host: &dyn HostCtx) -> anyhow::Result<Vec<IndexerRow>>;
    fn get_indexer(&self, host: &dyn HostCtx, id: &str) -> anyhow::Result<Option<IndexerRow>>;
    fn note_indexer_result(
        &self,
        host: &dyn HostCtx,
        id: &str,
        ok: bool,
        error: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()>;
}

/// Where a built-in (native Cardigann) indexer's release can be grabbed from.
pub enum DownloadTarget {
    Magnet(String),
    TorrentUrl(String),
}

/// Outcome of one native-engine search sweep: the releases found plus any
/// per-path errors (a partial error alongside real results is not fatal).
pub struct SearchOutcome {
    pub releases: Vec<Release>,
    pub errors: Vec<String>,
}

/// Run native (built-in Cardigann) indexer searches + resolve a release's grab
/// target. Implemented by the indexer module (it owns the Cardigann sessions) and
/// resolved by acquisition, so acquisition never names the indexer crate.
pub trait IndexerSearchPort: Send + Sync {
    /// Run `query` against the built-in indexer `row` over the given categories.
    fn search_builtin(
        &self,
        host: &dyn HostCtx,
        row: &IndexerRow,
        query: &Query,
        categories: &[u32],
    ) -> anyhow::Result<SearchOutcome>;
    /// Resolve the grabbable target (magnet / .torrent URL) for a built-in
    /// release, following the definition's `download` block when needed.
    fn resolve_download(
        &self,
        host: &dyn HostCtx,
        row: &IndexerRow,
        title: &str,
        details_url: Option<&str>,
        magnet_or_url: &str,
    ) -> anyhow::Result<DownloadTarget>;
}
