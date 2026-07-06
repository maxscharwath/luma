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
use luma_torrents::{AddTorrentReq, ClientDef, DownloadClient, RqbitConfig, RqbitEngine};

use crate::db::{self, DownloadClientRow, DownloadRow};
use crate::infra::events::ServerEvent;
use crate::model::{RequestStatus, ScoredReleaseView, VpnStatusView};
use crate::services::jobs::now_ms;
use crate::state::SharedState;

/// The LUMA category/label applied inside external clients.
pub const LABEL: &str = "luma";

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
    /// Scratch dir (qBittorrent cookie jars).
    state_dir: PathBuf,
    /// Root for per-download output folders of the embedded engine.
    downloads_dir: PathBuf,
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
            downloads_dir: state_dir.join("downloads"),
            state_dir,
        })
    }

    /// Start (or restart) the embedded engine from current settings. Errors
    /// are logged, not fatal: external clients keep working without it.
    ///
    /// Robust restart: start the NEW session first (ephemeral DHT/peer ports so
    /// it never collides with the old), swap it in only on success, then stop
    /// the old. A failed restart therefore leaves the previous engine running
    /// instead of killing downloads.
    pub async fn start_rqbit(&self, state: &SharedState) {
        let cfg = RqbitConfig {
            session_dir: self.state_dir.join("session"),
            download_dir: self.downloads_dir.clone(),
            socks_proxy_url: active_proxy_url(state),
            listen_port: u16::try_from(state.settings.get_i64("rqbitPort", 0).max(0)).ok(),
            download_bps: kbps_setting(state, "rqbitDownKbps"),
            upload_bps: kbps_setting(state, "rqbitUpKbps"),
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

    /// Fetch a torrent's file list (metadata only, no download) via the
    /// preferred engine, so the admin can analyze + select before grabbing.
    pub fn list_files(&self, state: &SharedState, magnet_or_url: &str) -> Result<Vec<luma_torrents::TorrentFileEntry>> {
        let conn = state.db.get()?;
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

    /// Build the engine for a stored client row.
    pub fn engine_for(&self, row: &DownloadClientRow) -> Result<Box<dyn DownloadClient>> {
        let def = ClientDef {
            kind: row.kind.clone(),
            url: row.url.clone(),
            username: row.username.clone(),
            password: row.password.clone(),
        };
        luma_torrents::client_for(&def, self.rqbit(), &self.state_dir)
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
    pub fn vpn_check(&self, state: &SharedState) -> Option<luma_torrents::proxycheck::VpnCheck> {
        let Some(proxy) = active_proxy_url(state) else {
            self.gate_open.store(true, Ordering::Relaxed);
            *self.vpn_status.lock().unwrap() = None;
            return None;
        };
        let check_url = state.settings.get_str("vpnCheckUrl", "https://api.ipify.org");
        let check = luma_torrents::proxycheck::check(&proxy, &check_url);
        let sealed = check.sealed();
        // Opt-in: the kill switch does nothing unless the admin turns it on.
        let kill_switch = state.settings.get_bool("vpnKillSwitch", false);
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
            self.close_gate(state);
        } else if (!kill_switch || sealed) && !was_open {
            self.open_gate(state);
        }
        let paused = !self.gate_open.load(Ordering::Relaxed);
        let status = VpnStatusView { connected: sealed, exit_ip: check.proxied_ip.clone(), paused };
        let changed = self.vpn_status.lock().unwrap().replace(status.clone()) != Some(status.clone());
        if changed {
            state.events.publish(ServerEvent::VpnStatus {
                connected: status.connected,
                exit_ip: status.exit_ip.clone(),
                paused: status.paused,
            });
        }
        Some(check)
    }

    /// Close: refuse new grabs, pause every active embedded-engine download
    /// (external clients guard their own tunnel), remember exactly which rows
    /// we paused so recovery never resumes a user-paused torrent.
    fn close_gate(&self, state: &SharedState) {
        self.gate_open.store(false, Ordering::Relaxed);
        tracing::warn!("VPN kill switch engaged: pausing embedded downloads");
        let mut held: Vec<String> = Vec::new();
        if let Ok(conn) = state.db.get() {
            if let Ok(rows) = db::active_downloads(&conn) {
                drop(conn);
                for row in rows {
                    if row.client_id != db::EMBEDDED_CLIENT_ID || row.status == "paused" {
                        continue;
                    }
                    if self.pause(state, &row.id).is_ok() {
                        held.push(row.id);
                    }
                }
            }
        }
        *self.paused_by_killswitch.lock().unwrap() = held;
    }

    fn open_gate(&self, state: &SharedState) {
        self.gate_open.store(true, Ordering::Relaxed);
        let held = std::mem::take(&mut *self.paused_by_killswitch.lock().unwrap());
        if !held.is_empty() {
            tracing::info!(count = held.len(), "VPN restored: resuming held downloads");
        }
        for id in held {
            let _ = self.resume(state, &id);
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
    pub fn grab(&self, state: &SharedState, spec: GrabSpec) -> Result<DownloadRow> {
        if !self.gate_open() {
            bail!("downloads are held by the VPN kill switch");
        }
        if spec.magnet_or_url.trim().is_empty() {
            bail!("no magnet or download link");
        }
        let conn = state.db.get()?;
        // Dedup: refuse a second grab of a torrent already in the queue (same
        // magnet/URL, not failed/removed). Retrying a failed one is still fine.
        if let Some(existing) = db::active_download_by_url(&conn, spec.magnet_or_url.trim())? {
            bail!("this release is already in the queue (\"{}\", status: {})", existing.title.as_deref().unwrap_or(&existing.release_title), existing.status);
        }
        let client = db::preferred_download_client(&conn)?
            .ok_or_else(|| anyhow!("no enabled download client"))?;
        drop(conn);

        let id = crate::services::scan::short_hash(&format!(
            "download|{}|{}",
            spec.release_title,
            crate::services::auth::random_token()
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
        db::insert_download(&state.db, &row)?;
        db::set_wanted_status(&state.db, &spec.wanted_ids, "grabbed", now_ms())?;
        if let Some(req_id) = &row.request_id {
            // Do NOT persist a `downloading` status on the request: it's a
            // transient phase derived at read time from the live download
            // relationship (see api::requests overlay), so it self-heals when
            // the grab fails or the torrent is deleted. Just nudge listeners.
            state.events.publish(ServerEvent::RequestUpdated {
                id: req_id.clone(),
                status: RequestStatus::Downloading.as_str().to_string(),
            });
        }
        tracing::info!(release = %row.release_title, client = %client.name, "queued torrent grab");
        Ok(row)
    }

    /// Background phase of a grab: actually hand the torrent to the engine
    /// (slow: magnet resolve / `.torrent` fetch, up to a couple of minutes) and
    /// move the row to `downloading`, or mark it `failed` with the error. Safe
    /// to run detached from the request that queued it.
    pub fn activate(&self, state: &SharedState, row: &DownloadRow) {
        let client = match state.db.get().and_then(|c| Ok(db::get_download_client(&c, &row.client_id)?)) {
            Ok(Some(c)) => c,
            _ => {
                let _ = db::set_download_status(&state.db, &row.id, "failed", Some("download client unavailable"));
                return;
            }
        };
        let engine = match self.engine_for(&client) {
            Ok(e) => e,
            Err(e) => {
                let _ = db::set_download_status(&state.db, &row.id, "failed", Some(&format!("engine unavailable: {e:#}")));
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
                match fetch_torrent_file(&row.magnet_or_url) {
                    Ok(bytes) => {
                        tracing::info!(id = %row.id, bytes = bytes.len(), "fetched .torrent directly (bypassing VPN)");
                        Some(bytes)
                    }
                    Err(e) => {
                        let msg = format!("could not fetch .torrent from the indexer: {e:#}");
                        tracing::warn!(id = %row.id, error = %msg, "torrent file fetch failed");
                        let _ = db::set_download_status(&state.db, &row.id, "failed", Some(&msg));
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
                let current = state
                    .db
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
                        let _ = db::set_download_ref(&state.db, &row.id, &client_ref);
                        tracing::info!(release = %row.release_title, "torrent added then paused (paused while adding)");
                    }
                    _ => {
                        // Dedup by info-hash: the engine returns the SAME ref for
                        // identical content grabbed from a different URL. If another
                        // live row already owns this torrent, don't run two against
                        // one - fail this one (the engine torrent stays for the other).
                        let dup = state
                            .db
                            .get()
                            .ok()
                            .and_then(|c| db::other_active_download_with_ref(&c, &row.id, &client_ref).ok().flatten());
                        if let Some(other) = dup {
                            let name = other.title.as_deref().unwrap_or(&other.release_title);
                            let _ = db::set_download_status(&state.db, &row.id, "failed", Some(&format!("duplicate of \"{name}\" (same torrent already downloading)")));
                            tracing::info!(id = %row.id, "grab duplicates a live download; marked failed");
                        } else {
                            if let Err(e) = db::activate_download(&state.db, &row.id, &client_ref) {
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
                let _ = db::set_download_status(&state.db, &row.id, "failed", Some(&msg));
            }
        }
    }

    /// Re-attempt a failed (or removed) grab: drop any half-added torrent from
    /// the engine, reset the row to `queued`, and return it so the caller can
    /// re-run [`Self::activate`] in the background. Also re-flips its wanted rows
    /// so a re-grab covers them again.
    pub fn retry(&self, state: &SharedState, id: &str) -> Result<DownloadRow> {
        let (row, client) = self.row_and_client(state, id)?;
        if !self.gate_open() {
            bail!("downloads are held by the VPN kill switch");
        }
        // Best-effort: remove a stale/half-added torrent before re-adding.
        if !row.client_ref.is_empty() {
            if let Ok(engine) = self.engine_for(&client) {
                let _ = engine.remove(&row.client_ref, false);
            }
        }
        db::reset_download_for_retry(&state.db, id)?;
        let conn = state.db.get()?;
        let row = db::get_download(&conn, id)?.ok_or_else(|| anyhow!("download not found"))?;
        Ok(row)
    }

    /// Remove the torrent from the engine + delete its downloaded data, but KEEP
    /// the ledger row (status stays `imported`). Used by the "delete after import"
    /// option to free the download folder + stop seeding once the file is safely
    /// in the library (the hardlink/copy there survives). Best-effort.
    pub fn drop_data(&self, state: &SharedState, row: &DownloadRow) {
        if row.client_ref.is_empty() {
            return;
        }
        let client = state.db.get().ok().and_then(|c| db::get_download_client(&c, &row.client_id).ok().flatten());
        if let Some(client) = client {
            if let Ok(engine) = self.engine_for(&client) {
                if let Err(e) = engine.remove(&row.client_ref, true) {
                    tracing::warn!(id = %row.id, error = %format!("{e:#}"), "delete-after-import: engine remove failed");
                    return;
                }
                // The engine no longer tracks it; blank the ref so pause/resume
                // and the monitor don't try to poll a gone torrent.
                let _ = db::set_download_ref(&state.db, &row.id, "");
                tracing::info!(release = %row.release_title, "deleted torrent + data after import");
            }
        }
    }

    /// Pause/resume/remove one download by row id, mirroring engine + ledger.
    /// A row with an empty `client_ref` is still being added in the background
    /// (slow magnet/`.torrent` resolve); we skip the engine call and just move
    /// the ledger, and `activate()` honors that state when the add lands.
    pub fn pause(&self, state: &SharedState, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(state, id)?;
        if !row.client_ref.is_empty() {
            self.engine_for(&client)?.pause(&row.client_ref)?;
        }
        db::set_download_status(&state.db, id, "paused", None)?;
        Ok(())
    }

    pub fn resume(&self, state: &SharedState, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(state, id)?;
        if row.client_ref.is_empty() {
            // Not in the engine yet: re-queue so it gets (re)added.
            db::set_download_status(&state.db, id, "queued", None)?;
            return Ok(());
        }
        self.engine_for(&client)?.resume(&row.client_ref)?;
        db::set_download_status(&state.db, id, "downloading", None)?;
        Ok(())
    }

    pub fn remove(&self, state: &SharedState, id: &str, delete_data: bool) -> Result<()> {
        let (row, client) = self.row_and_client(state, id)?;
        // The engine may already have dropped it (or never had it); removal
        // stays best-effort so the ledger can always be cleaned up.
        if !row.client_ref.is_empty() {
            if let Ok(engine) = self.engine_for(&client) {
                if let Err(e) = engine.remove(&row.client_ref, delete_data) {
                    tracing::warn!(id, error = %format!("{e:#}"), "engine remove failed");
                }
            }
        }
        db::delete_download_row(&state.db, id)?;
        Ok(())
    }

    /// Pause every LUMA-tracked download that is still active (best-effort per
    /// row). Only our ledger's torrents are touched, never foreign torrents in a
    /// shared external client. Returns how many were paused.
    pub fn pause_all(&self, state: &SharedState) -> Result<usize> {
        let rows = {
            let conn = state.db.get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.status == "paused" {
                continue;
            }
            match self.pause(state, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "pause_all: skipped a download"),
            }
        }
        Ok(n)
    }

    /// Resume every LUMA download we previously paused. Returns the count.
    pub fn resume_all(&self, state: &SharedState) -> Result<usize> {
        let rows = {
            let conn = state.db.get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.status != "paused" {
                continue;
            }
            match self.resume(state, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "resume_all: skipped a download"),
            }
        }
        Ok(n)
    }

    /// Force a tracker/DHT re-announce ("ask more peers") on one download.
    pub fn reannounce(&self, state: &SharedState, id: &str) -> Result<()> {
        let (row, client) = self.row_and_client(state, id)?;
        if !row.client_ref.is_empty() {
            self.engine_for(&client)?.reannounce(&row.client_ref)?;
        }
        Ok(())
    }

    /// Force a tracker/DHT re-announce ("ask more peers") on every active
    /// download. Best-effort per row. Returns how many were reannounced.
    pub fn reannounce_all(&self, state: &SharedState) -> Result<usize> {
        let rows = {
            let conn = state.db.get()?;
            db::active_downloads(&conn)?
        };
        let mut n = 0;
        for row in rows {
            if row.client_ref.is_empty() || row.status == "paused" {
                continue;
            }
            match self.reannounce(state, &row.id) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(id = %row.id, error = %format!("{e:#}"), "reannounce_all: skipped a download"),
            }
        }
        Ok(n)
    }

    fn row_and_client(&self, state: &SharedState, id: &str) -> Result<(DownloadRow, DownloadClientRow)> {
        let conn = state.db.get()?;
        let row = db::get_download(&conn, id)?.ok_or_else(|| anyhow!("download not found"))?;
        let client = db::get_download_client(&conn, &row.client_id)?
            .ok_or_else(|| anyhow!("download client no longer configured"))?;
        Ok((row, client))
    }
}

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

impl GrabSpec {
    /// From a scored release the search chose, for a specific request/title.
    #[allow(clippy::too_many_arguments)]
    pub fn from_release(
        release: &ScoredReleaseView,
        magnet_or_url: &str,
        tmdb_id: u64,
        title: Option<String>,
        year: Option<u32>,
        request_id: Option<String>,
        wanted_ids: Vec<String>,
    ) -> Self {
        Self {
            magnet_or_url: magnet_or_url.to_string(),
            kind: release.target.clone(),
            tmdb_id,
            title,
            year,
            season: release.season,
            episodes: release.episodes.clone(),
            release_title: release.title.clone(),
            indexer_id: Some(release.indexer_id.clone()),
            size_bytes: release.size_bytes,
            score: release.score,
            score_breakdown: serde_json::to_string(&release.breakdown).ok(),
            request_id,
            wanted_ids,
            only_files: None,
            details_url: release.details_url.clone(),
        }
    }
}

/// The local SOCKS5 the embedded engine routes torrent peers through, when a
/// WireGuard config is stored. This is the ONLY VPN path: LUMA runs a
/// wireproxy bridge (WireGuard in, `socks5://127.0.0.1:<port>` out) and hands
/// that local URL to librqbit. The SOCKS5 is an internal implementation detail
/// of routing WireGuard traffic (librqbit only proxies via SOCKS5); it is not
/// a user-facing option. `None` = no VPN, torrent traffic goes out directly.
pub fn active_proxy_url(state: &SharedState) -> Option<String> {
    crate::services::vpn::Vpn::wg_configured(state)
        .then(|| crate::services::vpn::Vpn::local_proxy_url(state))
}

fn kbps_setting(state: &SharedState, key: &str) -> Option<u32> {
    let kbps = state.settings.get_i64(key, 0);
    (kbps > 0).then(|| u32::try_from(kbps.saturating_mul(1024)).unwrap_or(u32::MAX))
}

/// Fetch a `.torrent` file from the indexer DIRECTLY (no VPN proxy), so a LAN
/// indexer (Jackett/Prowlarr) stays reachable and the fetch can't hang behind
/// the tunnel. Only the torrent's peer traffic should go through the VPN.
fn fetch_torrent_file(url: &str) -> Result<Vec<u8>> {
    let resp = luma_fetch::Fetch::new().max_time(30).get(url)?.ensure_ok()?;
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
