//! The embedded BitTorrent engine: a librqbit `Session` wrapped so the sync
//! [`DownloadClient`] trait can drive it (callers sit on blocking threads; a
//! captured tokio `Handle` bridges into the async session). Torrents are
//! identified by their info-hash hex, stable across restarts (the session
//! persists to JSON and fastresumes on boot).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use librqbit::api::TorrentIdOrHash;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ConnectionOptions, DhtSessionConfig,
    ListenerMode, ListenerOptions, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig,
};

/// `librqbit::torrent_state::ManagedTorrentHandle`, not re-exported at the root.
type ManagedTorrentHandle = Arc<ManagedTorrent>;

use crate::{AddTorrentReq, DownloadClient, TorrentState, TorrentStatus};

/// Everything the engine needs at start (mapped from server settings).
#[derive(Debug, Clone, Default)]
pub struct RqbitConfig {
    /// Session state folder (fastresume JSON).
    pub session_dir: PathBuf,
    /// Default output folder (each torrent normally overrides per download).
    pub download_dir: PathBuf,
    /// `socks5://[user:pass@]host:port`; every peer connection routes through
    /// it when set (the VPN seal).
    pub socks_proxy_url: Option<String>,
    /// Fixed listen port; `None`/0 = ephemeral.
    pub listen_port: Option<u16>,
    pub download_bps: Option<u32>,
    pub upload_bps: Option<u32>,
}

/// A running librqbit session + the runtime handle to drive it from sync code.
pub struct RqbitEngine {
    session: Arc<Session>,
    handle: tokio::runtime::Handle,
    /// The VPN socks5 the session dials through (if any), reused for our own
    /// tracker announces so recovered peers are reachable from the same network.
    socks_proxy: Option<String>,
}

impl RqbitEngine {
    /// Start (or restore, via fastresume) the embedded session. Must be called
    /// from within a tokio runtime.
    pub async fn start(cfg: &RqbitConfig) -> Result<Arc<RqbitEngine>> {
        std::fs::create_dir_all(&cfg.download_dir).ok();
        std::fs::create_dir_all(&cfg.session_dir).ok();
        let opts = SessionOptions {
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(cfg.session_dir.clone()),
            }),
            fastresume: true,
            // 9.x moved the SOCKS proxy under `connect`. Peer traffic dials
            // through it when set (the VPN seal); `None` = direct.
            connect: Some(ConnectionOptions {
                proxy_url: cfg.socks_proxy_url.clone(),
                ..Default::default()
            }),
            // 9.x moved the fixed listen port + UPnP under `listen`. Keep it
            // TCP-only (the default): uTP is UDP and can't traverse a TCP
            // SOCKS5 tunnel, so a VPN session must stay TCP-only regardless.
            // `listen_port` 0/None = OS-assigned.
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: (std::net::Ipv6Addr::UNSPECIFIED, cfg.listen_port.unwrap_or(0)).into(),
                // A NAS behind the user's control: no surprise router reconfig.
                enable_upnp_port_forwarding: false,
                ..Default::default()
            }),
            ratelimits: LimitsConfig {
                download_bps: cfg.download_bps.and_then(std::num::NonZeroU32::new),
                upload_bps: cfg.upload_bps.and_then(std::num::NonZeroU32::new),
            },
            // DHT on, but ephemeral port + NO persistence: on a restart (e.g. a
            // VPN config change) a fixed persisted port collides with the
            // session still shutting down ("address already in use"). An
            // ephemeral port re-bootstraps in a few seconds. (Private torrents
            // disable DHT per-torrent regardless, so this only helps public ones.)
            dht: Some(DhtSessionConfig { bootstrap_addrs: None, port: None, persistence: None }),
            ..Default::default()
        };
        let session = Session::new_with_opts(cfg.download_dir.clone(), opts)
            .await
            .context("start embedded torrent session")?;
        Ok(Arc::new(RqbitEngine {
            session,
            handle: tokio::runtime::Handle::current(),
            socks_proxy: cfg.socks_proxy_url.clone(),
        }))
    }

    /// Drain the session (stops all torrent activity). Used before a restart
    /// when proxy/port settings change. Async shutdown runs detached; the
    /// replacement session is safe to start immediately (fresh sockets).
    pub fn stop(&self) {
        let session = self.session.clone();
        self.handle.spawn(async move { session.stop().await });
    }

    pub fn client(self: &Arc<Self>) -> Box<dyn DownloadClient> {
        Box::new(RqbitClient { engine: self.clone() })
    }

    fn find(&self, client_ref: &str) -> Result<ManagedTorrentHandle> {
        let id = TorrentIdOrHash::parse(client_ref)
            .map_err(|e| anyhow!("bad torrent ref {client_ref:?}: {e:#}"))?;
        self.session.get(id).ok_or_else(|| anyhow!("torrent {client_ref} not in session"))
    }

    /// Re-seed a stalled torrent with a fresh peer set (from our own proxied
    /// tracker announce). librqbit has no public API to push peers into a live
    /// torrent, and `initial_peers` is only honored on the FIRST add, so this
    /// removes the torrent (KEEPING its data on disk) and re-adds it seeded with
    /// the peers. Fastresume makes the re-add near-instant (the bitfield is
    /// trusted, not re-hashed). Callers must serialize this against status polls
    /// (the monitor runs both on its one thread) so the brief remove window is
    /// never observed as "torrent disappeared".
    pub fn reseed(
        &self,
        client_ref: &str,
        torrent_bytes: Vec<u8>,
        output_folder: Option<&str>,
        peers: &[std::net::SocketAddr],
    ) -> Result<()> {
        let id = TorrentIdOrHash::parse(client_ref)
            .map_err(|e| anyhow!("bad torrent ref {client_ref:?}: {e:#}"))?;
        let output_folder = output_folder.map(str::to_string);
        let peers = peers.to_vec();
        self.handle.block_on(async {
            // Keep the files (delete_files = false); we re-add against them.
            self.session.delete(id, false).await.ok();
            let opts = AddTorrentOptions {
                output_folder,
                // NOT overwrite: resume from whatever is already on disk (fastresume)
                // instead of truncating it - a reseed must never reset progress.
                overwrite: false,
                initial_peers: Some(peers),
                ..Default::default()
            };
            self.session
                .add_torrent(AddTorrent::from_bytes(torrent_bytes), Some(opts))
                .await
                .map(|_| ())
        })
    }
}

/// The `DownloadClient` face of the engine (thin clone-able wrapper).
pub(crate) struct RqbitClient {
    engine: Arc<RqbitEngine>,
}

/// librqbit `Speed.mbps` is MiB/s (bytes / 1024 / 1024).
fn mib_to_bytes(mbps: f64) -> u64 {
    (mbps * 1024.0 * 1024.0).max(0.0) as u64
}

fn status_of(handle: &ManagedTorrentHandle) -> TorrentStatus {
    use librqbit::TorrentStatsState as S;
    let stats = handle.stats();
    let progress = if stats.total_bytes > 0 {
        stats.progress_bytes as f64 / stats.total_bytes as f64
    } else {
        0.0
    };
    let state = match stats.state {
        S::Initializing => TorrentState::Queued,
        S::Paused if stats.finished => TorrentState::Completed,
        S::Paused => TorrentState::Paused,
        S::Error => TorrentState::Error,
        S::Live if stats.finished => TorrentState::Seeding,
        S::Live => TorrentState::Downloading,
    };
    let (down_bps, up_bps) = stats
        .live
        .as_ref()
        .map(|l| (mib_to_bytes(l.download_speed.mbps), mib_to_bytes(l.upload_speed.mbps)))
        .unwrap_or((0, 0));
    let (peers, peers_seen) = stats
        .live
        .as_ref()
        .map(|l| (l.snapshot.peer_stats.live, l.snapshot.peer_stats.seen))
        .unwrap_or((0, 0));
    let metadata = handle.metadata.load();
    let files = metadata
        .as_ref()
        .map(|m| {
            m.file_infos
                .iter()
                .map(|f| f.relative_filename.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    TorrentStatus {
        client_ref: handle.info_hash().as_string(),
        name: handle.name().unwrap_or_default(),
        info_hash: Some(handle.info_hash().as_string()),
        progress,
        state,
        down_bps,
        up_bps,
        peers,
        peers_seen,
        size_bytes: stats.total_bytes,
        // The manager sets an explicit per-download output folder at add time
        // and remembers it; the engine reports none.
        save_path: None,
        files,
        error: stats.error,
    }
}

impl DownloadClient for RqbitClient {
    fn kind(&self) -> &'static str {
        "rqbit"
    }

    fn test(&self) -> Result<String> {
        Ok(format!("librqbit {} (embedded)", "9"))
    }

    fn add(&self, req: &AddTorrentReq) -> Result<String> {
        let engine = &self.engine;
        // Seed the swarm ourselves: behind the VPN, librqbit's reqwest tracker
        // client can't traverse the SOCKS bridge, so its own announce finds
        // nothing. When we have the `.torrent` bytes, announce over the bridge
        // ourselves (curl) and hand the session the peers (incl. IPv6) as
        // `initial_peers`. The periodic reseed keeps this fresh; this is the
        // head start at add time. See `announce.rs` / `reseed_stalled`.
        let initial_peers = req.torrent_bytes.map(|bytes| {
            let peers = crate::announce::tracker_peers(bytes, engine.socks_proxy.as_deref());
            if !peers.is_empty() {
                let v6 = peers.iter().filter(|p| p.is_ipv6()).count();
                tracing::info!(total = peers.len(), ipv6 = v6, "seeded torrent with peers from our proxied tracker announce");
            }
            peers
        });
        let opts = AddTorrentOptions {
            output_folder: req.download_dir.map(str::to_string),
            overwrite: true,
            only_files: req.only_files.map(<[usize]>::to_vec),
            initial_peers: initial_peers.filter(|p| !p.is_empty()),
            ..Default::default()
        };
        // Pre-fetched `.torrent` bytes (fetched by the caller outside the VPN)
        // add instantly; otherwise hand librqbit the magnet/URL to resolve.
        let magnet_or_url = req.magnet_or_url.to_string();
        let add = match req.torrent_bytes {
            Some(bytes) => AddTorrent::from_bytes(bytes.to_vec()),
            None => AddTorrent::from_url(&magnet_or_url),
        };
        let response = engine.handle.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(120),
                engine.session.add_torrent(add, Some(opts)),
            )
            .await
            .map_err(|_| anyhow!("timed out adding torrent (magnet resolve slow?)"))?
        })?;
        match response {
            AddTorrentResponse::Added(_, handle) | AddTorrentResponse::AlreadyManaged(_, handle) => {
                Ok(handle.info_hash().as_string())
            }
            AddTorrentResponse::ListOnly(_) => bail!("unexpected list-only add response"),
        }
    }

    fn list_files(
        &self,
        magnet_or_url: &str,
        torrent_bytes: Option<&[u8]>,
    ) -> Result<Vec<crate::TorrentFileEntry>> {
        let engine = &self.engine;
        // list_only fetches the metadata + file list but downloads nothing.
        let opts = AddTorrentOptions { list_only: true, ..Default::default() };
        let url = magnet_or_url.to_string();
        let add = match torrent_bytes {
            Some(bytes) => AddTorrent::from_bytes(bytes.to_vec()),
            None => AddTorrent::from_url(&url),
        };
        let response = engine.handle.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(90),
                engine.session.add_torrent(add, Some(opts)),
            )
            .await
            .map_err(|_| anyhow!("timed out fetching torrent metadata"))?
        })?;
        let info = match response {
            AddTorrentResponse::ListOnly(resp) => resp.info,
            // Already in the session: read its metadata instead.
            AddTorrentResponse::Added(_, h) | AddTorrentResponse::AlreadyManaged(_, h) => {
                h.metadata.load().as_ref().map(|m| m.info.clone()).ok_or_else(|| {
                    anyhow!("torrent already added but its metadata is not resolved yet")
                })?
            }
        };
        let mut out = Vec::new();
        // 9.x: iter_file_details() and to_pathbuf() return plain values (no Result).
        for (index, file) in info.iter_file_details().enumerate() {
            out.push(crate::TorrentFileEntry {
                index,
                path: file.filename.to_pathbuf().to_string_lossy().into_owned(),
                size_bytes: file.len,
            });
        }
        Ok(out)
    }

    fn status(&self, client_ref: &str) -> Result<Option<TorrentStatus>> {
        match TorrentIdOrHash::parse(client_ref).ok().and_then(|id| self.engine.session.get(id)) {
            Some(handle) => Ok(Some(status_of(&handle))),
            None => Ok(None),
        }
    }

    fn pause(&self, client_ref: &str) -> Result<()> {
        let handle = self.engine.find(client_ref)?;
        self.engine.handle.block_on(self.engine.session.pause(&handle))
    }

    fn resume(&self, client_ref: &str) -> Result<()> {
        let handle = self.engine.find(client_ref)?;
        self.engine.handle.block_on(self.engine.session.unpause(&handle))
    }

    fn reannounce(&self, client_ref: &str) -> Result<()> {
        // librqbit exposes no force-announce; a pause->unpause cycle rebuilds
        // the live task, which re-announces to every tracker immediately (the
        // recovery lever for a peer-starved torrent stuck at 0%).
        let handle = self.engine.find(client_ref)?;
        if matches!(handle.stats().state, librqbit::TorrentStatsState::Paused) {
            return Ok(()); // deliberately paused: don't resurrect it
        }
        self.engine.handle.block_on(async {
            self.engine.session.pause(&handle).await?;
            self.engine.session.unpause(&handle).await
        })
    }

    fn remove(&self, client_ref: &str, delete_data: bool) -> Result<()> {
        let id = TorrentIdOrHash::parse(client_ref)
            .map_err(|e| anyhow!("bad torrent ref {client_ref:?}: {e:#}"))?;
        self.engine.handle.block_on(self.engine.session.delete(id, delete_data))
    }
}
