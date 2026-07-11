//! Torrent download engines behind one [`DownloadClient`] trait, so the
//! server's DownloadManager is engine-agnostic: the embedded librqbit session
//! (feature `rqbit`), a Transmission daemon (RPC over curl) and a qBittorrent
//! WebUI (cookie auth over curl) all look the same. Mirrors the shape of the
//! server's LLM `Provider` trait + `provider_for` dispatch.
//!
//! The trait is synchronous by design: every caller sits on a blocking thread
//! (jobs, the API's `blocking` combinator, the downloads monitor), and the
//! rqbit impl bridges into tokio with a captured runtime `Handle`. There is no
//! engine-wide pause: the kill switch pauses per torrent through the manager's
//! own ledger, so user-paused torrents and foreign torrents in a shared
//! external client are never touched.


use anyhow::bail;
use serde::{Deserialize, Serialize};

#[cfg(feature = "rqbit")]
mod announce;
// The acquisition + organize verticals, moved out of the core luma-engine crate
// so the core depends on ZERO module crates. They orchestrate the app state
// (luma-engine) over the scene/indexer/torznab engines this module already deps.
pub mod acquisition;
pub mod downloads;
pub mod dtos;
pub mod module;
pub mod organize;
pub mod proxycheck;
pub mod routes;
#[cfg(feature = "rqbit")]
mod rqbit;
#[cfg(not(feature = "rqbit"))]
#[path = "rqbit_stub.rs"]
mod rqbit;

pub use dtos::*;
pub use module::MODULE;
pub use rqbit::{RqbitConfig, RqbitEngine};
// The download manager + monitor (merged in from the former luma-downloads crate),
// re-exported at the crate root so `luma_torrent::DownloadManager` etc. keep working.
pub use downloads::{active_proxy_url, DownloadManager, GrabSpec, LABEL};

/// Whether the embedded engine is compiled into this build.
pub const RQBIT_COMPILED: bool = cfg!(feature = "rqbit");

/// The acquisition background jobs this module contributes to the app's job
/// registry (search / import / match). The binary passes this to
/// `AppState::new` so the core registers them without naming the module.
pub const JOBS: &[luma_engine::services::jobs::Builtin] = &[
    acquisition::jobs::import::SPEC,
    acquisition::jobs::search::SPEC,
    acquisition::jobs::match_::SPEC,
];

/// This module's id, shared with `module.json` and the frontend package. The one
/// place callers (route gate, job guards, monitor, lifecycle) name the module.
pub const MODULE_ID: &str = "dev.luma.torrents";

/// The Downloads module's backend behavior: it serves the queue / download-client
/// / organize admin routes (behind its enabled-gate) and drives the librqbit
/// engine lifecycle, so disabling it 404s those routes and stops the running
/// engine. Its download sub-engines (rqbit / transmission / qBittorrent) plug into
/// the `DownloadClientRegistry`; VPN is a separate module this one
/// `optionalDependsOn`. It reaches its [`DownloadManager`] through the host service
/// registry.
///
/// Unlike the vpn / indexer modules it is NOT generic over the host state: its
/// routes orchestrate the acquisition + organize verticals, which run against
/// `luma-engine`'s concrete `AppState`, so it is a `ServerModule<SharedState>`.
pub struct DownloadsModule;

#[luma_module_host::async_trait]
impl luma_module_host::ServerModule<luma_engine::state::SharedState> for DownloadsModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn admin_routes(
        &self,
        _host: &luma_engine::state::SharedState,
    ) -> Option<axum::Router<luma_engine::state::SharedState>> {
        Some(routes::routes())
    }

    async fn on_enable(&self, host: std::sync::Arc<dyn luma_module_host::HostCtx>) {
        // Everything the Downloads module needs at (re)enable lives here, so the
        // binary shell never seeds rows or spawns the monitor: seed the embedded
        // client row, start the engine, flip disable-paused rows back to active,
        // and ensure the resident monitor is running (spawned once). The VPN bridge
        // is its own module (ordered first by the dependency graph), so its SOCKS5
        // is already up. Awaited (not detached) so a following disable cannot race.
        if let Some(downloads) = luma_module_host::service::<DownloadManager>(host.as_ref()) {
            downloads.seed_embedded_client(host.as_ref());
            downloads.start_rqbit(host.as_ref()).await;
            downloads.resume_after_enable(host.as_ref());
            downloads.ensure_monitor(host.clone());
        }
    }

    async fn on_disable(&self, host: std::sync::Arc<dyn luma_module_host::HostCtx>) {
        // Tear the engine down entirely (session stopped, active downloads paused)
        // so nothing is left transferring or seeding while disabled.
        if let Some(downloads) = luma_module_host::service::<DownloadManager>(host.as_ref()) {
            downloads.disable_embedded(host.as_ref());
        }
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module() -> Box<dyn luma_module_host::ServerModule<luma_engine::state::SharedState>> {
    Box::new(DownloadsModule)
}

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
    pub rqbit: Option<std::sync::Arc<RqbitEngine>>,
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

/// Register the built-in factory for ONE client `kind` (returns false for an
/// unknown kind). This is the single-kind entry point the download-engine
/// sub-modules use to add their kind when toggled on (`rqbit` stays part of the
/// Downloads module; `transmission` / `qbittorrent` are their own modules).
/// `rqbit` registers a real factory when compiled in and a clear "not compiled"
/// stub otherwise (so the error is actionable).
pub fn register_client_kind(reg: &mut DownloadClientRegistry, kind: &str) -> bool {
    match kind {
        "rqbit" => {
            #[cfg(feature = "rqbit")]
            reg.register("rqbit", |_def, ctx| match &ctx.rqbit {
                Some(engine) => Ok(engine.client()),
                None => bail!("embedded engine not started"),
            });
            #[cfg(not(feature = "rqbit"))]
            reg.register("rqbit", |_def, _ctx| {
                bail!("embedded engine not compiled (torrent-rqbit feature off)")
            });
            true
        }
        _ => false,
    }
}

/// Register the core download engine (embedded librqbit). Transmission and
/// qBittorrent are their own crates now (`luma-transmission` / `luma-qbittorrent`)
/// that register their own kind; the binary wires them at boot + on toggle.
pub fn register_download_clients(reg: &mut DownloadClientRegistry) {
    register_client_kind(reg, "rqbit");
}

/// A registry with the core (rqbit) engine registered. The binary layers the
/// external engine crates on top.
pub fn builtin_download_clients() -> DownloadClientRegistry {
    let mut reg = DownloadClientRegistry::default();
    register_download_clients(&mut reg);
    reg
}

/// Best-effort info-hash extraction from a magnet URI (`xt=urn:btih:HEX`). Public
/// so the external engine crates (qBittorrent) can reuse it.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_magnet_info_hash() {
        let m = "magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&dn=Thing&tr=udp://x";
        assert_eq!(
            magnet_info_hash(m).as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        assert_eq!(magnet_info_hash("magnet:?xt=urn:btih:short"), None);
        assert_eq!(magnet_info_hash("https://example.com/file.torrent"), None);
    }

    #[test]
    fn cookie_jars_are_stable_and_distinct() {
        let a = ClientDef { kind: "qbittorrent".into(), url: "http://a:8080".into(), username: "u".into(), password: String::new() };
        let b = ClientDef { url: "http://b:8080".into(), ..a.clone() };
        let dir = std::path::Path::new("/tmp");
        assert_eq!(cookie_jar_path(dir, &a), cookie_jar_path(dir, &a));
        assert_ne!(cookie_jar_path(dir, &a), cookie_jar_path(dir, &b));
    }
}
