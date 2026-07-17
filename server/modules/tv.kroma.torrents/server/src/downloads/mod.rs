//! The download manager: owns the embedded torrent engine's lifecycle, builds
//! engines from client-config rows, records grabs in the downloads ledger and
//! carries the kill-switch gate. The resident polling loop lives in
//! [`monitor`]; everything here is synchronous and called from blocking
//! contexts (jobs, `api::util::blocking`, the monitor's own spawn_blocking).

pub mod monitor;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{anyhow, bail, Result};
use crate::{AddTorrentReq, ClientDef, DownloadClient, RqbitConfig, RqbitEngine};

use crate::db::{self, DownloadClientRow, DownloadRow};
use kroma_module_sdk::domain::RequestStatus;

use crate::VpnStatusView;
use kroma_module_sdk::host::{Event, HostCtx};
use serde_json::json;
use kroma_module_sdk::primitives::now_ms;

/// The KROMA category/label applied inside external clients.
pub const LABEL: &str = "kroma";

pub struct DownloadManager {
    /// The embedded engine, once started (None = failed / not compiled / off).
    rqbit: RwLock<Option<Arc<RqbitEngine>>>,
    /// Kill-switch gate: closed = no new grabs, active torrents paused. Opens
    /// at boot (downloads work out of the box) and stays open unless the admin
    /// explicitly enables the kill switch AND the VPN check keeps failing.
    gate_open: AtomicBool,
    /// Consecutive failed VPN seal checks. The kill switch only closes the gate
    /// after a couple in a row, so a transient blip / bridge still starting up
    /// never slams downloads shut.
    vpn_fail_streak: AtomicU32,
    /// Latest VPN probe outcome, for the admin banner.
    vpn_status: Mutex<Option<VpnStatusView>>,
    /// Download refs the kill switch paused (so recovery resumes exactly
    /// those, never a user-paused torrent).
    paused_by_killswitch: Mutex<Vec<String>>,
    /// Download refs paused because the embedded engine was disabled in the
    /// admin UI (resumed on re-enable; never resumes a user-paused torrent).
    paused_by_disable: Mutex<Vec<String>>,
    /// Scratch dir (qBittorrent cookie jars).
    state_dir: PathBuf,
    /// Root for per-download output folders of the embedded engine.
    downloads_dir: PathBuf,
    /// The download sub-engine registry (kind -> factory). Shared + mutable so
    /// the download-engine sub-modules can register / unregister their kind when
    /// toggled. Adding a new backend is registering a factory here, not a `match`.
    clients: RwLock<crate::DownloadClientRegistry>,
    /// Guards [`Self::ensure_monitor`] so the resident loop spawns at most once
    /// per process even though the module's `on_enable` may fire more than once.
    monitor_started: AtomicBool,
}

/// Failed VPN checks in a row before the kill switch actually closes the gate.
const VPN_FAIL_GRACE: u32 = 2;

impl DownloadManager {
    pub fn new(data_dir: &std::path::Path) -> Arc<Self> {
        let state_dir = data_dir.join("torrents");
        std::fs::create_dir_all(&state_dir).ok();
        Arc::new(Self {
            rqbit: RwLock::new(None),
            gate_open: AtomicBool::new(true),
            vpn_fail_streak: AtomicU32::new(0),
            vpn_status: Mutex::new(None),
            paused_by_killswitch: Mutex::new(Vec::new()),
            paused_by_disable: Mutex::new(Vec::new()),
            downloads_dir: state_dir.join("downloads"),
            state_dir,
            clients: RwLock::new(crate::builtin_download_clients()),
            monitor_started: AtomicBool::new(false),
        })
    }

    /// Seed the embedded engine's download-client row (idempotent; INSERT OR
    /// IGNORE keeps admin edits) so it exists once the engine is (re)enabled. A
    /// no-op when the embedded engine is not compiled in. Owned here so the binary
    /// shell never names the rqbit client row (onion boundary).
    pub fn seed_embedded_client(&self, host: &dyn HostCtx) {
        if !crate::RQBIT_COMPILED {
            return;
        }
        let _ = db::insert_download_client(
            host.db(),
            &DownloadClientRow {
                id: db::EMBEDDED_CLIENT_ID.to_string(),
                kind: "rqbit".into(),
                name: "Moteur intégré".into(),
                url: String::new(),
                username: String::new(),
                password: String::new(),
                enabled: true,
                priority: 100,
                created_at: now_ms(),
            },
        );
    }

    /// Spawn the resident monitor exactly once per process. The module's
    /// `on_enable` may fire more than once (boot + re-enable); the loop self-idles
    /// while the module is disabled, so one long-lived task covers every cycle.
    pub fn ensure_monitor(self: &Arc<Self>, host: Arc<dyn HostCtx>) {
        if self.monitor_started.swap(true, Ordering::SeqCst) {
            return;
        }
        self.spawn_monitor(host);
    }

    /// Start (or restart) the embedded engine from current settings. Errors
    /// are logged, not fatal: external clients keep working without it.
    ///
    /// Robust restart: start the NEW session first (ephemeral DHT/peer ports so
    /// it never collides with the old), swap it in only on success, then stop
    /// the old. A failed restart therefore leaves the previous engine running
    /// instead of killing downloads.
    pub async fn start_rqbit(&self, host: &dyn HostCtx) {
        // Hard-off must survive restarts + setting/VPN changes: never bring the
        // engine up while the embedded client is disabled. (A missing row = first
        // boot before seeding = treated as enabled.)
        if let Ok(conn) = host.db().get() {
            if let Ok(Some(c)) = db::get_download_client(&conn, db::EMBEDDED_CLIENT_ID) {
                if !c.enabled {
                    drop(conn);
                    self.stop_rqbit();
                    return;
                }
            }
        }
        // `None` from the proxy port can mean "no VPN" OR "the VPN sidecar is
        // not answering yet" (it resolves over the port bridge, and sidecars
        // boot concurrently). While a WireGuard config is stored and the VPN
        // module enabled, peer traffic must stay sealed: defer the start (the
        // monitor retries every VPN tick) rather than run on the raw
        // connection with the banner still showing green.
        let proxy = active_proxy_url(host);
        if proxy.is_none() && vpn_sealed_expected(host) {
            tracing::warn!(
                "VPN is configured but its proxy is not resolvable; embedded engine start deferred (never runs unsealed)"
            );
            return;
        }
        let cfg = RqbitConfig {
            session_dir: self.state_dir.join("session"),
            download_dir: self.downloads_dir.clone(),
            socks_proxy_url: proxy,
            listen_port: u16::try_from(host.setting_i64("rqbitPort", 0).max(0)).ok(),
            download_bps: kbps_setting(host, "rqbitDownKbps"),
            upload_bps: kbps_setting(host, "rqbitUpKbps"),
        };
        match RqbitEngine::start(&cfg).await {
            Ok(engine) => {
                tracing::info!(proxy = cfg.socks_proxy_url.is_some(), "embedded torrent engine started");
                let old = self.rqbit.write().unwrap().replace(engine);
                if let Some(old) = old {
                    old.stop();
                }
            }
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "embedded torrent engine restart failed; keeping the previous session");
            }
        }
    }

    pub fn rqbit(&self) -> Option<Arc<RqbitEngine>> {
        self.rqbit.read().unwrap().clone()
    }

    /// Live engine stats (down/up bps, connected/seen peers) per active download
    /// id, queried straight from the engine. The queue endpoint folds these into
    /// its polled response so the panel shows speed + peers without the live
    /// WebSocket event stream (which a tunnel may not carry). Blocking; call off
    /// the runtime.
    pub fn live_stats(&self, host: &dyn HostCtx) -> std::collections::HashMap<String, (u64, u64, u32, u32)> {
        let mut out = std::collections::HashMap::new();
        let Ok(rows) = host.db().get().and_then(|c| Ok(db::active_downloads(&c)?)) else {
            return out;
        };
        for row in rows {
            if row.client_ref.is_empty() {
                continue;
            }
            let client = match host.db().get().and_then(|c| Ok(db::get_download_client(&c, &row.client_id)?)) {
                Ok(Some(c)) => c,
                _ => continue,
            };
            if let Ok(engine) = self.engine_for(&client) {
                if let Ok(Some(s)) = engine.status(&row.client_ref) {
                    out.insert(row.id, (s.down_bps, s.up_bps, s.peers, s.peers_seen));
                }
            }
        }
        out
    }

    /// Re-seed embedded torrents that librqbit's own tracker announce can't feed
    /// while behind the VPN. librqbit dials trackers via reqwest, whose SOCKS
    /// support can't traverse the WireGuard-to-SOCKS bridge (it fails
    /// "host unreachable"), so with a proxy configured its ongoing announce
    /// yields nothing and a torrent whose swarm it hasn't otherwise discovered
    /// sits at 0 peers - the exact case of a private, IPv6-only tracker (no DHT
    /// fallback). We announce ourselves THROUGH the bridge (curl, which does
    /// traverse it), parse peers (incl. IPv6), and inject them. Only runs with a
    /// proxy set (direct connections don't have the problem) and only for
    /// torrents the engine sees no peers for at all (`peers_seen == 0`), so a
    /// healthy or connecting torrent is never disturbed. Blocking (curl +
    /// engine); the monitor calls it on its own thread, serialized with its
    /// status polls so the brief remove/re-add window is never mis-read.
    pub fn reseed_stalled(&self, host: &dyn HostCtx) {
        // No proxy => librqbit's own (direct) announce works; nothing to do.
        let Some(proxy) = active_proxy_url(host) else { return };
        let Some(engine) = self.rqbit() else { return };
        let client = engine.client();
        let session_dir = self.state_dir.join("session");
        let rows = match host.db().get().and_then(|c| Ok(db::active_downloads(&c)?)) {
            Ok(rows) => rows,
            Err(_) => return,
        };
        for row in rows {
            if row.client_id != db::EMBEDDED_CLIENT_ID || row.client_ref.is_empty() {
                continue;
            }
            let Ok(Some(status)) = client.status(&row.client_ref) else { continue };
            // Only reseed a genuinely DEAD grab: no data downloaded, no live peer,
            // and none ever seen. A reseed is a remove/re-add, which RESETS the
            // torrent to 0% - so touching one that has ANY progress or ANY peer
            // would throw away a working download. `progress > 0` is the hard
            // guard (a torrent that has downloaded a single byte is working).
            if status.progress > 0.0 || status.peers > 0 || status.peers_seen > 0 {
                continue;
            }
            // librqbit persists each torrent as `<info_hash>.torrent`, and
            // client_ref IS the info-hash hex.
            let path = session_dir.join(format!("{}.torrent", row.client_ref));
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let peers = crate::announce::tracker_peers(&bytes, Some(&proxy));
            if peers.is_empty() {
                continue;
            }
            match engine.reseed(&row.client_ref, bytes, row.save_path.as_deref(), &peers) {
                Ok(()) => tracing::info!(
                    id = %row.id,
                    peers = peers.len(),
                    "re-seeded a stalled torrent with peers from our proxied tracker announce"
                ),
                Err(e) => {
                    tracing::warn!(id = %row.id, error = %format!("{e:#}"), "torrent re-seed failed")
                }
            }
        }
    }

    /// Fetch a torrent's file list (metadata only, no download) via the
    /// preferred engine, so the admin can analyze + select before grabbing.
    pub fn list_files(&self, host: &dyn HostCtx, magnet_or_url: &str) -> Result<Vec<crate::TorrentFileEntry>> {
        let conn = host.db().get()?;
        let client = db::preferred_download_client(&conn)?
            .ok_or_else(|| anyhow!("no enabled download client"))?;
        drop(conn);
        // Fetch a `.torrent` link direct (bypass the VPN) for the same reason as
        // grabbing: a LAN indexer is unreachable through the tunnel.
        let prefetched: Option<Vec<u8>> = (client.kind == "rqbit"
            && magnet_or_url.starts_with("http"))
        .then(|| fetch_torrent_file(magnet_or_url))
        .transpose()?;
        self.engine_for(&client)?.list_files(magnet_or_url, prefetched.as_deref())
    }

    /// Build the engine for a stored client row via the sub-engine registry.
    pub fn engine_for(&self, row: &DownloadClientRow) -> Result<Box<dyn DownloadClient>> {
        let def = ClientDef {
            kind: row.kind.clone(),
            url: row.url.clone(),
            username: row.username.clone(),
            password: row.password.clone(),
        };
        self.clients.read().expect("download client registry lock").build(
            &def,
            &crate::DownloadClientCtx {
                rqbit: self.rqbit().map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>),
                state_dir: &self.state_dir,
            },
        )
    }

    // ----- kill switch ----------------------------------------------------------

    pub fn gate_open(&self) -> bool {
        self.gate_open.load(Ordering::Relaxed)
    }

    pub fn vpn_status(&self) -> Option<VpnStatusView> {
        self.vpn_status.lock().unwrap().clone()
    }

    /// One VPN probe + gate transition. Called by the monitor (~every 60s)
    /// and by the admin test endpoint. No proxy configured = dormant (gate
    /// open, no status). Blocking (curl); call off the runtime.
    pub fn vpn_check(&self, host: &dyn HostCtx) -> Option<crate::proxycheck::VpnCheck> {
        let Some(proxy) = active_proxy_url(host) else {
            self.gate_open.store(true, Ordering::Relaxed);
            *self.vpn_status.lock().unwrap() = None;
            return None;
        };
        let check_url = host.setting_str("vpnCheckUrl", "https://api.ipify.org");
        let check = crate::proxycheck::check(&proxy, &check_url);
        let sealed = check.sealed();
        // Opt-in: the kill switch does nothing unless the admin turns it on.
        let kill_switch = host.setting_bool("vpnKillSwitch", false);
        let was_open = self.gate_open.load(Ordering::Relaxed);

        // Track a failure streak so one blip (or the bridge still coming up)
        // never blocks downloads; only a sustained failure closes the gate.
        let streak = if sealed {
            self.vpn_fail_streak.store(0, Ordering::Relaxed);
            0
        } else {
            self.vpn_fail_streak.fetch_add(1, Ordering::Relaxed) + 1
        };

        if kill_switch && !sealed && streak >= VPN_FAIL_GRACE && was_open {
            self.close_gate(host);
        } else if (!kill_switch || sealed) && !was_open {
            self.open_gate(host);
        }
        let paused = !self.gate_open.load(Ordering::Relaxed);
        let status = VpnStatusView { connected: sealed, exit_ip: check.proxied_ip.clone(), paused };
        let changed = self.vpn_status.lock().unwrap().replace(status.clone()) != Some(status.clone());
        if changed {
            host.publish(Event::new(
                "vpn.status",
                json!({
                    "connected": status.connected,
                    "exitIp": status.exit_ip,
                    "paused": status.paused,
                }),
            ));
        }
        Some(check)
    }

    /// Close: refuse new grabs, pause every active embedded-engine download
    /// (external clients guard their own tunnel), remember exactly which rows
    /// we paused so recovery never resumes a user-paused torrent.
    fn close_gate(&self, host: &dyn HostCtx) {
        self.gate_open.store(false, Ordering::Relaxed);
        tracing::warn!("VPN kill switch engaged: pausing embedded downloads");
        let mut held: Vec<String> = Vec::new();
        if let Ok(conn) = host.db().get() {
            if let Ok(rows) = db::active_downloads(&conn) {
                drop(conn);
                for row in rows {
                    if row.client_id != db::EMBEDDED_CLIENT_ID || row.status == "paused" {
                        continue;
                    }
                    if self.pause(host, &row.id).is_ok() {
                        held.push(row.id);
                    }
                }
            }
        }
        *self.paused_by_killswitch.lock().unwrap() = held;
    }

    fn open_gate(&self, host: &dyn HostCtx) {
        self.gate_open.store(true, Ordering::Relaxed);
        let held = std::mem::take(&mut *self.paused_by_killswitch.lock().unwrap());
        if !held.is_empty() {
            tracing::info!(count = held.len(), "VPN restored: resuming held downloads");
        }
        for id in held {
            let _ = self.resume(host, &id);
        }
    }

    /// Hard-stop the embedded engine: drop the session so **all** BitTorrent
    /// activity ceases (no download, no upload/seed, no DHT, listen sockets
    /// closed). Idempotent.
    pub fn stop_rqbit(&self) {
        if let Some(engine) = self.rqbit.write().unwrap().take() {
            engine.stop();
            tracing::info!("embedded torrent engine stopped");
        }
    }

    /// Disable the embedded engine (admin toggle): mark its active downloads
    /// paused (for the UI) and tear the session down entirely, so nothing is
    /// left listening or transferring. `start_rqbit` will refuse to come back up
    /// until it is re-enabled, so this survives restarts.
    pub fn disable_embedded(&self, host: &dyn HostCtx) {
        let mut held = Vec::new();
        if let Ok(conn) = host.db().get() {
            if let Ok(rows) = db::active_downloads(&conn) {
                drop(conn);
                for row in rows {
                    if row.client_id == db::EMBEDDED_CLIENT_ID && row.status != "paused" {
                        let _ = db::set_download_status(host.db(), &row.id, "paused", None);
                        held.push(row.id);
                    }
                }
            }
        }
        *self.paused_by_disable.lock().unwrap() = held;
        self.stop_rqbit();
        tracing::warn!("embedded engine disabled: session stopped, downloads paused");
    }

    /// Re-enable after [`disable_embedded`]: the caller has already restarted the
    /// session ([`start_rqbit`], which reloads the persisted torrents), so just
    /// flip the rows we paused back to active - the monitor reconciles the exact
    /// status (downloading vs seeding) from the live engine.
    pub fn resume_after_enable(&self, host: &dyn HostCtx) {
        let held = std::mem::take(&mut *self.paused_by_disable.lock().unwrap());
        for id in held {
            let _ = db::set_download_status(host.db(), &id, "downloading", None);
        }
    }

    // ----- grabbing ---------------------------------------------------------------

    /// Send one accepted release to the preferred engine and record the grab.
    /// `wanted_ids` flip to `grabbed`.
    ///
    /// This does NO torrent network I/O: it inserts a `queued` row (with an
    /// empty `client_ref`) and returns immediately, so the HTTP handler never
    /// blocks on a slow magnet resolve / `.torrent` fetch. Adding it to the
    /// engine happens in the background via [`Self::activate`]; the monitor
    /// then picks the row up once it has a `client_ref`.
    pub fn grab(&self, host: &dyn HostCtx, spec: GrabSpec) -> Result<DownloadRow> {
        if !self.gate_open() {
            bail!("downloads are held by the VPN kill switch");
        }
        if spec.magnet_or_url.trim().is_empty() {
            bail!("no magnet or download link");
        }
        let conn = host.db().get()?;
        // Dedup: refuse a second grab of a torrent already in the queue (same
        // magnet/URL, not failed/removed). Retrying a failed one is still fine.
        if let Some(existing) = db::active_download_by_url(&conn, spec.magnet_or_url.trim())? {
            bail!("this release is already in the queue (\"{}\", status: {})", existing.title.as_deref().unwrap_or(&existing.release_title), existing.status);
        }
        let client = db::preferred_download_client(&conn)?
            .ok_or_else(|| anyhow!("no enabled download client"))?;
        drop(conn);

        let id = kroma_module_sdk::primitives::short_hash(&format!(
            "download|{}|{}",
            spec.release_title,
            kroma_module_sdk::primitives::random_token()
        ));
        // The embedded engine downloads into a per-grab folder we choose, so
        // the importer knows exactly where the data is. External engines use
        // their own default directory and report it back via status().
        let save_path = (client.kind == "rqbit")
            .then(|| self.downloads_dir.join(&id).to_string_lossy().into_owned());

        let row = DownloadRow {
            id,
            client_id: client.id.clone(),
            client_ref: String::new(), // filled in by activate() once added
            request_id: spec.request_id.clone(),
            kind: spec.kind,
            tmdb_id: spec.tmdb_id,
            title: spec.title,
            year: spec.year,
            season: spec.season,
            episodes: spec.episodes,
            release_title: spec.release_title,
            indexer_id: spec.indexer_id,
            info_hash: None,
            magnet_or_url: spec.magnet_or_url,
            size_bytes: spec.size_bytes,
            score: spec.score,
            score_breakdown: spec.score_breakdown,
            status: "queued".into(),
            progress: 0.0,
            save_path,
            imported_paths: None,
            error: None,
            grabbed_at: now_ms(),
            completed_at: None,
            imported_at: None,
            details_url: spec.details_url,
            only_files: spec.only_files,
        };
        db::insert_download(host.db(), &row)?;
        db::set_wanted_status(host.db(), &spec.wanted_ids, "grabbed", now_ms())?;
        if let Some(req_id) = &row.request_id {
            // Do NOT persist a `downloading` status on the request: it's a
            // transient phase derived at read time from the live download
            // relationship (see api::requests overlay), so it self-heals when
            // the grab fails or the torrent is deleted. Just nudge listeners.
            host.publish(Event::new(
                "request.updated",
                json!({ "id": req_id, "status": RequestStatus::Downloading.as_str() }),
            ));
        }
        tracing::info!(release = %row.release_title, client = %client.name, "queued torrent grab");
        Ok(row)
    }

    /// Background phase of a grab: actually hand the torrent to the engine
    /// (slow: magnet resolve / `.torrent` fetch, up to a couple of minutes) and
    /// move the row to `downloading`, or mark it `failed` with the error. Safe
    /// to run detached from the request that queued it.
    pub fn activate(&self, host: &dyn HostCtx, row: &DownloadRow) {
        let client = match host.db().get().and_then(|c| Ok(db::get_download_client(&c, &row.client_id)?)) {
            Ok(Some(c)) => c,
            _ => {
                let _ = db::set_download_status(host.db(), &row.id, "failed", Some("download client unavailable"));
                return;
            }
        };
        let engine = match self.engine_for(&client) {
            Ok(e) => e,
            Err(e) => {
                let _ = db::set_download_status(host.db(), &row.id, "failed", Some(&format!("engine unavailable: {e:#}")));
                return;
            }
        };
        // A `.torrent` HTTP link points at the indexer (often local Jackett/
        // Prowlarr on the LAN). librqbit routes ALL its traffic through the VPN
        // proxy, and a `0.0.0.0/0` tunnel can't reach a private LAN address, so
        // its own fetch hangs until the add times out. Fetch the file OURSELVES,
        // directly (no proxy), and hand the engine the bytes; only peer traffic
        // then rides the VPN. Magnets have no file to fetch - let the engine
        // resolve those (via proxied peers).
        let prefetched: Option<Vec<u8>> =
            if client.kind == "rqbit" && row.magnet_or_url.starts_with("http") {
                match fetch_torrent_for(host, row) {
                    Ok(bytes) => {
                        tracing::info!(id = %row.id, bytes = bytes.len(), "fetched .torrent directly (bypassing VPN)");
                        Some(bytes)
                    }
                    Err(e) => {
                        let msg = format!("could not fetch .torrent from the indexer: {e:#}");
                        tracing::warn!(id = %row.id, error = %msg, "torrent file fetch failed");
                        let _ = db::set_download_status(host.db(), &row.id, "failed", Some(&msg));
                        return;
                    }
                }
            } else {
                None
            };
        let added = engine.add(&AddTorrentReq {
            magnet_or_url: &row.magnet_or_url,
            download_dir: row.save_path.as_deref(),
            label: LABEL,
            only_files: row.only_files.as_deref(),
            torrent_bytes: prefetched.as_deref(),
        });
        match added {
            Ok(client_ref) => {
                // The add can take a while; the admin may have removed or paused
                // the row meanwhile. Honor that instead of resurrecting it.
                let current = host
                    .db()
                    .get()
                    .ok()
                    .and_then(|c| db::get_download(&c, &row.id).ok().flatten())
                    .map(|r| r.status);
                match current.as_deref() {
                    None => {
                        // Removed while adding: drop the orphan torrent.
                        let _ = engine.remove(&client_ref, true);
                        tracing::info!(id = %row.id, "torrent add landed after removal; dropped");
                    }
                    Some("paused") => {
                        let _ = engine.pause(&client_ref);
                        let _ = db::set_download_ref(host.db(), &row.id, &client_ref);
                        tracing::info!(release = %row.release_title, "torrent added then paused (paused while adding)");
                    }
                    _ => {
                        // Dedup by info-hash: the engine returns the SAME ref for
                        // identical content grabbed from a different URL. If another
                        // live row already owns this torrent, don't run two against
                        // one - fail this one (the engine torrent stays for the other).
                        let dup = host
                    .db()
                    .get()
                            .ok()
                            .and_then(|c| db::other_active_download_with_ref(&c, &row.id, &client_ref).ok().flatten());
                        if let Some(other) = dup {
                            let name = other.title.as_deref().unwrap_or(&other.release_title);
                            let _ = db::set_download_status(host.db(), &row.id, "failed", Some(&format!("duplicate of \"{name}\" (same torrent already downloading)")));
                            tracing::info!(id = %row.id, "grab duplicates a live download; marked failed");
                        } else {
                            if let Err(e) = db::activate_download(host.db(), &row.id, &client_ref) {
                                tracing::warn!(id = %row.id, error = %format!("{e:#}"), "failed to record activated torrent");
                            }
                            tracing::info!(release = %row.release_title, hash = %client_ref, "torrent added to engine");
                        }
                    }
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::warn!(id = %row.id, release = %row.release_title, error = %msg, "torrent add failed");
                let _ = db::set_download_status(host.db(), &row.id, "failed", Some(&msg));
            }
        }
    }

    /// Re-attempt a failed (or removed) grab: drop any half-added torrent from
    /// the engine, reset the row to `queued`, and return it so the caller can
    /// re-run [`Self::activate`] in the background. Also re-flips its wanted rows
    /// so a re-grab covers them again.
    pub fn retry(&self, host: &dyn HostCtx, id: &str) -> Result<DownloadRow> {
        let (row, client) = self.row_and_client(host, id)?;
        if !self.gate_open() {
            bail!("downloads are held by the VPN kill switch");
        }
        // Best-effort: remove a stale/half-added torrent before re-adding.
        if !row.client_ref.is_empty() {
            if let Ok(engine) = self.engine_for(&client) {
                let _ = engine.remove(&row.client_ref, false);
            }
        }
        db::reset_download_for_retry(host.db(), id)?;
        let conn = host.db().get()?;
        let row = db::get_download(&conn, id)?.ok_or_else(|| anyhow!("download not found"))?;
        Ok(row)
    }

    /// Remove the torrent from the engine + delete its downloaded data, but KEEP
    /// the ledger row (status stays `imported`). Used by the "delete after import"
    /// option to free the download folder + stop seeding once the file is safely
    /// in the library (the hardlink/copy there survives). Best-effort.
    pub fn drop_data(&self, host: &dyn HostCtx, row: &DownloadRow) {
        if row.client_ref.is_empty() {
            return;
        }
        let client = host.db().get().ok().and_then(|c| db::get_download_client(&c, &row.client_id).ok().flatten());
        if let Some(client) = client {
            if let Ok(engine) = self.engine_for(&client) {
                if let Err(e) = engine.remove(&row.client_ref, true) {
                    tracing::warn!(id = %row.id, error = %format!("{e:#}"), "delete-after-import: engine remove failed");
                    return;
                }
                // The engine no longer tracks it; blank the ref so pause/resume
                // and the monitor don't try to poll a gone torrent.
                let _ = db::set_download_ref(host.db(), &row.id, "");
                tracing::info!(release = %row.release_title, "deleted torrent + data after import");
            }
        }
    }

    /// Pause/resume/remove one download by row id, mirroring engine + ledger.
    /// A row with an empty `client_ref` is still being added in the background
    /// (slow magnet/`.torrent` resolve); we skip the engine call and just move
    /// the ledger, and `activate()` honors that state when the add lands.
    pub fn pause(&self, host: &dyn HostCtx, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(host, id)?;
        if !row.client_ref.is_empty() {
            self.engine_for(&client)?.pause(&row.client_ref)?;
        }
        db::set_download_status(host.db(), id, "paused", None)?;
        Ok(())
    }

    pub fn resume(&self, host: &dyn HostCtx, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(host, id)?;
        if row.client_ref.is_empty() {
            // Not in the engine yet: re-queue so it gets (re)added.
            db::set_download_status(host.db(), id, "queued", None)?;
            return Ok(());
        }
        self.engine_for(&client)?.resume(&row.client_ref)?;
        db::set_download_status(host.db(), id, "downloading", None)?;
        Ok(())
    }

    pub fn remove(&self, host: &dyn HostCtx, id: &str, delete_data: bool) -> Result<()> {
        let (row, client) = self.row_and_client(host, id)?;
        // The engine may already have dropped it (or never had it); removal
        // stays best-effort so the ledger can always be cleaned up.
        if !row.client_ref.is_empty() {
            if let Ok(engine) = self.engine_for(&client) {
                if let Err(e) = engine.remove(&row.client_ref, delete_data) {
                    tracing::warn!(id, error = %format!("{e:#}"), "engine remove failed");
                }
            }
        }
        db::delete_download_row(host.db(), id)?;
        Ok(())
    }

    /// Pause every KROMA-tracked download that is still active (best-effort per
    /// row). Only our ledger's torrents are touched, never foreign torrents in a
    /// shared external client. Returns how many were paused.
    pub fn pause_all(&self, host: &dyn HostCtx) -> Result<usize> {
        let rows = {
            let conn = host.db().get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.status == "paused" {
                continue;
            }
            match self.pause(host, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "pause_all: skipped a download"),
            }
        }
        Ok(n)
    }

    /// Resume every KROMA download we previously paused. Returns the count.
    pub fn resume_all(&self, host: &dyn HostCtx) -> Result<usize> {
        let rows = {
            let conn = host.db().get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.status != "paused" {
                continue;
            }
            match self.resume(host, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "resume_all: skipped a download"),
            }
        }
        Ok(n)
    }

    /// Force a tracker/DHT re-announce ("ask more peers") on one download.
    pub fn reannounce(&self, host: &dyn HostCtx, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(host, id)?;
        if !row.client_ref.is_empty() {
            self.engine_for(&client)?.reannounce(&row.client_ref)?;
        }
        Ok(())
    }

    /// Force a tracker/DHT re-announce ("ask more peers") on every active
    /// download. Best-effort per row. Returns how many were reannounced.
    pub fn reannounce_all(&self, host: &dyn HostCtx) -> Result<usize> {
        let rows = {
            let conn = host.db().get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.client_ref.is_empty() || row.status == "paused" {
                continue;
            }
            match self.reannounce(host, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "reannounce_all: skipped a download"),
            }
        }
        Ok(n)
    }

    fn row_and_client(&self, host: &dyn HostCtx, id: &str) -> Result<(DownloadRow, DownloadClientRow)> {
        let conn = host.db().get()?;
        let row = db::get_download(&conn, id)?.ok_or_else(|| anyhow!("download not found"))?;
        let client = db::get_download_client(&conn, &row.client_id)?
            .ok_or_else(|| anyhow!("download client no longer configured"))?;
        Ok((row, client))
    }
}

// GrabSpec moved to kroma_module_sdk::ports; re-exported for this crate.
pub use kroma_module_sdk::ports::GrabSpec;

// A `GrabSpec` is built from a scored release by the acquisition crate (which
// owns `ScoredReleaseView` now) or field-by-field for a manual add. This crate
// only consumes it, so it names no acquisition type.

/// The local SOCKS5 the embedded engine routes torrent peers through, when a
/// WireGuard config is stored. This is the ONLY VPN path: KROMA runs a
/// wireproxy bridge (WireGuard in, `socks5://127.0.0.1:<port>` out) and hands
/// that local URL to librqbit. The SOCKS5 is an internal implementation detail
/// of routing WireGuard traffic (librqbit only proxies via SOCKS5); it is not
/// a user-facing option. `None` = no VPN, torrent traffic goes out directly.
pub fn active_proxy_url(host: &dyn HostCtx) -> Option<String> {
    // Route torrent traffic through the VPN module's bridge whenever it provides
    // one, resolved by port so downloads never depends on the VPN crate.
    kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::VpnProxyPort>(host)
        .and_then(|p| p.proxy_url(host))
}

/// Whether peer traffic is REQUIRED to ride the VPN bridge: the VPN module is
/// enabled and a WireGuard config is stored. Settings share one namespace (this
/// module already reads `vpnKillSwitch` / `vpnCheckUrl`), so this stays a
/// data-level check; the VPN crate is never named.
fn vpn_sealed_expected(host: &dyn HostCtx) -> bool {
    host.module_enabled("tv.kroma.vpn") && !host.setting_str("vpnWgConfig", "").trim().is_empty()
}

fn kbps_setting(host: &dyn HostCtx, key: &str) -> Option<u32> {
    let kbps = host.setting_i64(key, 0);
    (kbps > 0).then(|| u32::try_from(kbps.saturating_mul(1024)).unwrap_or(u32::MAX))
}

/// Fetch a `.torrent`'s bytes for a grab. Fetched DIRECTLY (never through the VPN
/// proxy) so a LAN indexer (Jackett/Prowlarr) stays reachable and the fetch can't
/// hang behind the tunnel; only the torrent's peer traffic goes through the VPN.
/// A grab from a built-in Cardigann indexer is fetched through the indexer
/// module's authenticated-session port (private trackers cookie-gate the
/// download, so a bare fetch would get the HTML login page); Torznab / manual
/// grabs fall back to a plain fetch.
fn fetch_torrent_for(host: &dyn HostCtx, row: &db::DownloadRow) -> Result<Vec<u8>> {
    // Retry transient transport failures. Trackers behind Cloudflare
    // intermittently drop the TLS connection (`curl (35) SSL_ERROR_ZERO_RETURN`,
    // reset, timeout); a fresh attempt almost always succeeds. Content errors
    // (not a .torrent, empty) are NOT retried - they won't change on a retry.
    let mut last = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(600 * attempt));
        }
        match fetch_torrent_once(host, row) {
            Ok(bytes) => return Ok(bytes),
            Err(e) if is_transient_fetch(&e) => {
                tracing::warn!(id = %row.id, attempt = attempt + 1, error = %format!("{e:#}"), "torrent fetch transient failure; retrying");
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("torrent fetch failed")))
}

/// One `.torrent` fetch attempt: through the source indexer's authenticated
/// Cardigann session if it is a built-in indexer, else a plain fetch.
fn fetch_torrent_once(host: &dyn HostCtx, row: &db::DownloadRow) -> Result<Vec<u8>> {
    if let Some(indexer_id) = &row.indexer_id {
        // `None` means it is not a built-in Cardigann indexer, so fall through to
        // a plain fetch. Downloads never names the indexer crate.
        if let Some(port) =
            kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::TorrentFetchPort>(host)
        {
            if let Some(result) = port.fetch_torrent(host, indexer_id, &row.magnet_or_url) {
                return result;
            }
        }
    }
    fetch_torrent_file(&row.magnet_or_url)
}

/// A transport-level failure worth retrying (vs a content error that won't
/// change): any `curl` transport error - SSL, connection reset/refused, empty
/// reply, timeout - surfaces as "curl exit N".
fn is_transient_fetch(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}");
    msg.contains("curl exit")
        || msg.contains("SSL")
        || msg.contains("timed out")
        || msg.contains("Connection reset")
        || msg.contains("empty response")
}

fn fetch_torrent_file(url: &str) -> Result<Vec<u8>> {
    let resp = kroma_module_sdk::http::Fetch::new().max_time(30).get(url)?.ensure_ok()?;
    if resp.body.is_empty() {
        bail!("indexer returned an empty response");
    }
    // A tracker error page is HTML/JSON, not a bencoded torrent (starts with 'd').
    if resp.body.first() != Some(&b'd') {
        bail!("indexer did not return a .torrent file (got: {})", snippet(&resp.body));
    }
    Ok(resp.body)
}

fn snippet(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(120).collect::<String>().replace('\n', " ")
}

/// The download manager IS the download-client host: engine modules resolve this
/// port (`kroma_module_sdk::ports::DownloadClientHost`) and register/unregister
/// their client kind on enable/disable, without depending on this crate.
impl kroma_module_sdk::ports::DownloadClientHost for DownloadManager {
    fn register_engine(&self, register: fn(&mut crate::DownloadClientRegistry)) {
        let mut reg = self.clients.write().expect("download client registry lock");
        register(&mut reg);
    }

    fn unregister_engine(&self, kind: &str) {
        self.clients.write().expect("download client registry lock").unregister(kind);
    }
}

/// The download manager's VPN surface, exposed as a port so the VPN module shows
/// the engine's kill-switch status, runs a seal check and restarts it after a
/// config change, without depending on this crate.
#[kroma_module_sdk::host::async_trait]
impl kroma_module_sdk::ports::DownloadVpnPort for DownloadManager {
    fn vpn_status(&self) -> Option<kroma_module_sdk::ports::VpnStatusView> {
        self.vpn_status()
    }

    fn vpn_seal_check(&self, host: &dyn HostCtx) -> Option<kroma_module_sdk::ports::VpnSeal> {
        self.vpn_check(host).map(|c| kroma_module_sdk::ports::VpnSeal {
            sealed: c.sealed(),
            proxied_ip: c.proxied_ip,
            direct_ip: c.direct_ip,
            error: c.error,
        })
    }

    async fn restart_engine(&self, host: &dyn HostCtx) {
        self.start_rqbit(host).await;
    }
}

// --- Cross-module capability ports (resolved by the Acquisition module) ---
// The grab + ledger surfaces acquisition needs, exposed as SDK ports so it never
// depends on this crate. Both just forward to the inherent methods / free fns.

impl kroma_module_sdk::ports::DownloadGrabPort for DownloadManager {
    fn grab(&self, host: &dyn HostCtx, spec: GrabSpec) -> Result<DownloadRow> {
        DownloadManager::grab(self, host, spec)
    }
    fn list_files(
        &self,
        host: &dyn HostCtx,
        magnet_or_url: &str,
    ) -> Result<Vec<crate::TorrentFileEntry>> {
        DownloadManager::list_files(self, host, magnet_or_url)
    }
    fn gate_open(&self) -> bool {
        DownloadManager::gate_open(self)
    }
    fn activate(&self, host: &dyn HostCtx, row: &DownloadRow) {
        DownloadManager::activate(self, host, row);
    }
    fn drop_data(&self, host: &dyn HostCtx, row: &DownloadRow) {
        DownloadManager::drop_data(self, host, row);
    }
}

/// The downloads-ledger read/write port (a ZST; the ledger operations are free
/// functions on the pool). Registered at boot so acquisition's import pass reads
/// completed rows + flips status without naming this crate.
pub struct DownloadDb;

impl kroma_module_sdk::ports::DownloadDbPort for DownloadDb {
    fn completed_downloads(&self, host: &dyn HostCtx) -> Result<Vec<DownloadRow>> {
        let conn = host.db().get()?;
        Ok(db::completed_downloads(&conn)?)
    }
    fn mark_download_imported(
        &self,
        host: &dyn HostCtx,
        id: &str,
        paths: &[String],
        now_ms: i64,
    ) -> Result<()> {
        db::mark_download_imported(host.db(), id, paths, now_ms)
    }
    fn set_download_status(
        &self,
        host: &dyn HostCtx,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<bool> {
        db::set_download_status(host.db(), id, status, error)
    }
}
