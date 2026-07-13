//! The download-client contract: the `DownloadClient` engine trait, the data
//! types torrent engines exchange, and the `DownloadClientHost` port a download
//! engine module resolves to register/unregister its kind. It lives here (not in
//! the torrents crate) so an engine module (transmission / qBittorrent) depends
//! only on the SDK. The embedded-engine handle in `DownloadClientCtx` is kept
//! opaque (`dyn Any`) so this crate carries no torrent-internal type.

use serde::{Deserialize, Serialize};

/// A torrent to hand to an engine.
#[derive(Debug, Clone)]
pub struct AddTorrentReq<'a> {
    /// `magnet:` URI or an `http(s)` `.torrent` link (Jackett proxy links).
    pub magnet_or_url: &'a str,
    /// Download directory. The embedded engine always honors it (the importer
    /// depends on knowing exactly where data lands); external engines treat it
    /// as a hint and may fall back to their own default.
    pub download_dir: Option<&'a str>,
    /// Category/label where the engine supports one ("luma"), so LUMA's
    /// torrents are recognizable inside a shared external client.
    pub label: &'a str,
    /// Download only these file indices (Sonarr/Radarr-style selection, e.g.
    /// one episode from a season pack). `None` = the whole torrent. Honored by
    /// the embedded engine; ignored by external clients.
    pub only_files: Option<&'a [usize]>,
    /// Pre-fetched `.torrent` file bytes. When set, the embedded engine adds
    /// these directly instead of fetching `magnet_or_url` itself. The caller
    /// fetches the `.torrent` from the indexer OUTSIDE the VPN tunnel (the
    /// indexer/Jackett is typically on the LAN, unreachable through a
    /// `0.0.0.0/0` tunnel), so only peer traffic rides the VPN.
    pub torrent_bytes: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TorrentState {
    Queued,
    Downloading,
    Seeding,
    Paused,
    Completed,
    Error,
}

/// A point-in-time view of one torrent inside an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentStatus {
    /// The engine's own identifier (info-hash hex for every shipped engine).
    pub client_ref: String,
    pub name: String,
    pub info_hash: Option<String>,
    /// 0..=1.
    pub progress: f64,
    pub state: TorrentState,
    pub down_bps: u64,
    pub up_bps: u64,
    /// Connected (live) peers, when the engine reports them.
    pub peers: u32,
    /// Peers discovered from the tracker / DHT (whether or not connected). If
    /// this is 0 while downloading, the tracker returned nothing (dead torrent
    /// or the announce failed / was blocked); if it's >0 but `peers` is 0, it's
    /// a connectivity problem (firewall / proxy).
    pub peers_seen: u32,
    pub size_bytes: u64,
    /// Directory the torrent's data lives under.
    pub save_path: Option<String>,
    /// Relative file paths inside `save_path` (what the importer walks).
    pub files: Vec<String>,
    pub error: Option<String>,
}

/// One file inside a torrent, from a metadata-only listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFileEntry {
    pub index: usize,
    pub path: String,
    pub size_bytes: u64,
}

/// One torrent engine.
pub trait DownloadClient: Send + Sync {
    fn kind(&self) -> &'static str;
    /// Reachability probe; returns a human-readable version string.
    fn test(&self) -> anyhow::Result<String>;
    /// Returns the engine's identifier for the new torrent (`client_ref`).
    fn add(&self, req: &AddTorrentReq) -> anyhow::Result<String>;
    /// Fetch the torrent's file list WITHOUT downloading (metadata-only), so
    /// the caller can analyze/select before committing. `torrent_bytes` are the
    /// pre-fetched `.torrent` file (fetched outside the VPN); when `None` the
    /// engine resolves `magnet_or_url` itself. Not every engine can do this
    /// (external clients don't expose a list-only add) - the default reports it
    /// as unsupported.
    fn list_files(
        &self,
        _magnet_or_url: &str,
        _torrent_bytes: Option<&[u8]>,
    ) -> anyhow::Result<Vec<TorrentFileEntry>> {
        anyhow::bail!("this download client cannot list a torrent's files before adding it")
    }
    /// `Ok(None)` = the engine no longer knows this torrent.
    fn status(&self, client_ref: &str) -> anyhow::Result<Option<TorrentStatus>>;
    fn pause(&self, client_ref: &str) -> anyhow::Result<()>;
    fn resume(&self, client_ref: &str) -> anyhow::Result<()>;
    /// Force a tracker / DHT re-announce now ("ask more peers"). Best-effort;
    /// the default is a no-op (the embedded engine already announces to trackers
    /// and DHT continuously, so there's nothing to force), overridden by the
    /// external clients that expose a manual reannounce.
    fn reannounce(&self, _client_ref: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn remove(&self, client_ref: &str, delete_data: bool) -> anyhow::Result<()>;
}

/// A configured engine, crate-owned mirror of the server's client row.
#[derive(Debug, Clone)]
pub struct ClientDef {
    /// `rqbit` | `transmission` | `qbittorrent`.
    pub kind: String,
    pub url: String,
    pub username: String,
    pub password: String,
}

/// Runtime context a download-client factory needs to build its engine: the
/// embedded librqbit handle (None when off / not compiled) and a scratch dir for
/// per-client state (qBittorrent cookie jars).
pub struct DownloadClientCtx<'a> {
    /// Opaque embedded-engine handle (only the torrents crate downcasts it).
    pub rqbit: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
    pub state_dir: &'a std::path::Path,
}

type DownloadClientFactory = std::sync::Arc<
    dyn Fn(&ClientDef, &DownloadClientCtx) -> anyhow::Result<Box<dyn DownloadClient>> + Send + Sync,
>;

/// The download-client sub-engine registry: maps a client `kind` ("rqbit",
/// "transmission", ...) to a factory that builds it. This is the extension point
/// a download sub-engine plugs into -- adding a new backend is registering one
/// factory, not editing a central `match`. It is the runtime constructor half of
/// the `download-client` capability the module declares in its `module.json`.
#[derive(Clone, Default)]
pub struct DownloadClientRegistry {
    factories: std::collections::HashMap<String, DownloadClientFactory>,
}

impl DownloadClientRegistry {
    /// Register (or replace) the factory for a client `kind`.
    pub fn register(
        &mut self,
        kind: impl Into<String>,
        factory: impl Fn(&ClientDef, &DownloadClientCtx) -> anyhow::Result<Box<dyn DownloadClient>>
            + Send
            + Sync
            + 'static,
    ) -> &mut Self {
        self.factories.insert(kind.into(), std::sync::Arc::new(factory));
        self
    }

    /// The kinds with a registered factory (diagnostics / capability checks).
    pub fn kinds(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// Remove a client `kind`'s factory (a download sub-engine module was
    /// disabled), so that kind is no longer offered / buildable.
    pub fn unregister(&mut self, kind: &str) {
        self.factories.remove(kind);
    }

    /// Build the engine for a client definition, or error if its kind has no
    /// registered sub-engine.
    pub fn build(
        &self,
        def: &ClientDef,
        ctx: &DownloadClientCtx,
    ) -> anyhow::Result<Box<dyn DownloadClient>> {
        let factory = self
            .factories
            .get(&def.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown download client kind {:?}", def.kind))?;
        factory(def, ctx)
    }
}

pub fn magnet_info_hash(uri: &str) -> Option<String> {
    let lower = uri.to_ascii_lowercase();
    let idx = lower.find("xt=urn:btih:")?;
    let hash: String = lower[idx + "xt=urn:btih:".len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    // 40-char hex (v1) or 32-char base32.
    (hash.len() == 40 || hash.len() == 32).then_some(hash)
}

/// The download manager's registration surface, exposed as a port so a download
/// engine module plugs its client `kind` in on enable without depending on the
/// torrents crate. Implemented by the Downloads module's `DownloadManager` and
/// resolved via `luma_module_host::resolve_port`.
pub trait DownloadClientHost: Send + Sync {
    /// Add a download sub-engine by running its `register` fn against the shared
    /// client registry.
    fn register_engine(&self, register: fn(&mut DownloadClientRegistry));
    /// Remove a download sub-engine `kind` (its module was disabled).
    fn unregister_engine(&self, kind: &str);
}
