//! Managed WireGuard-to-SOCKS5 bridge for torrent traffic, Proton VPN first.
//!
//! Proton (like most consumer VPNs) exposes no public SOCKS5 endpoint, but it
//! hands out standard WireGuard configs (account.protonvpn.com -> Downloads ->
//! WireGuard configuration; pick a P2P server). LUMA turns such a config into
//! a LOCAL SOCKS5 proxy by supervising a `wireproxy` child (userspace
//! WireGuard, single static binary), self-provisioned exactly like the
//! `cloudflared` connector in the `luma-remote` crate. The embedded
//! torrent engine then routes every peer connection through
//! `socks5://127.0.0.1:<port>`; the rest of the server never touches the
//! tunnel. Works identically for Mullvad or any other WireGuard provider.

mod provision;
pub mod routes;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::Router;
use tokio::process::Child;

use luma_module_host::{async_trait, service, HostCtx, ServerModule};

/// This module's registry entry (manifest + packaged icon, embedded at compile
/// time from the shared module folder).
pub const MODULE: luma_module_sdk::EmbeddedModule = luma_module_sdk::EmbeddedModule::new(
    include_str!("../../module.json"),
    include_bytes!("../../icon.svg"),
);

pub struct Vpn {
    data_dir: PathBuf,
    child: tokio::sync::Mutex<Option<Child>>,
    /// Bumped on every (re)configure so a stale exit-waiter never respawns an
    /// old generation.
    generation: AtomicU64,
}

impl Vpn {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { data_dir, child: tokio::sync::Mutex::new(None), generation: AtomicU64::new(0) })
    }

    fn dir(&self) -> PathBuf {
        self.data_dir.join("vpn")
    }

    /// (Re)apply the stored config: start / restart / stop the bridge child.
    /// Call at boot and whenever `vpnWgConfig` / `vpnLocalPort` change.
    pub async fn apply(self: &Arc<Self>, host: &dyn HostCtx) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // Always stop the previous child first (config change or teardown).
        if let Some(mut old) = self.child.lock().await.take() {
            let _ = old.kill().await;
        }
        let wg = host.setting_str("vpnWgConfig", "");
        if wg.trim().is_empty() {
            return;
        }
        let port = host.setting_i64("vpnLocalPort", 25345).clamp(1, 65535);
        if let Err(e) = self.clone().start_bridge(generation, wg, port as u16).await {
            tracing::warn!(error = %e, "wireguard bridge failed to start");
        }
    }

    /// Stop the bridge child and prevent respawns (the VPN module was disabled).
    /// Bumping the generation makes the current supervisor exit; `apply` brings
    /// the bridge back up when the module is re-enabled.
    pub async fn stop(self: &Arc<Self>) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(mut old) = self.child.lock().await.take() {
            let _ = old.kill().await;
        }
    }

    async fn start_bridge(
        self: Arc<Self>,
        generation: u64,
        wg_config: String,
        port: u16,
    ) -> Result<(), String> {
        let bin = provision::ensure(&self.data_dir).await?;
        let dir = self.dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create vpn dir: {e}"))?;
        let wg_path = dir.join("wg.conf");
        std::fs::write(&wg_path, wg_config.trim().to_string() + "\n")
            .map_err(|e| format!("write wg.conf: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&wg_path, std::fs::Permissions::from_mode(0o600));
        }
        let conf_path = dir.join("wireproxy.conf");
        std::fs::write(
            &conf_path,
            format!("WGConfig = {}\n\n[Socks5]\nBindAddress = 127.0.0.1:{port}\n", wg_path.display()),
        )
        .map_err(|e| format!("write wireproxy.conf: {e}"))?;

        let child = spawn_child(&bin, &conf_path)?;
        tracing::info!(port, "wireguard bridge started (wireproxy)");
        *self.child.lock().await = Some(child);

        // Supervisor: while this generation is the active config, respawn the
        // child (with a small backoff) whenever it dies. The kill switch
        // covers the gap. Respawns re-run the child directly, never `apply`.
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if me.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                let died = {
                    let mut guard = me.child.lock().await;
                    match guard.as_mut() {
                        Some(child) => child.try_wait().map(|s| s.is_some()).unwrap_or(true),
                        None => return,
                    }
                };
                if died {
                    tracing::warn!("wireguard bridge exited; restarting in 5s");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    if me.generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    match spawn_child(&bin, &conf_path) {
                        Ok(child) => *me.child.lock().await = Some(child),
                        Err(e) => tracing::warn!(error = %e, "wireguard bridge restart failed"),
                    }
                }
            }
        });
        Ok(())
    }

    /// Whether the bridge child is currently alive.
    pub async fn running(&self) -> bool {
        let mut guard = self.child.lock().await;
        match guard.as_mut() {
            Some(child) => child.try_wait().map(|s| s.is_none()).unwrap_or(false),
            None => false,
        }
    }
}

fn spawn_child(bin: &std::path::Path, conf: &std::path::Path) -> Result<Child, String> {
    tokio::process::Command::new(bin)
        .arg("-c")
        .arg(conf)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn wireproxy: {e}"))
}

/// Whether a managed WireGuard config is stored (for the module's own admin view).
pub fn wg_configured(host: &dyn HostCtx) -> bool {
    !host.setting_str("vpnWgConfig", "").trim().is_empty()
}

/// The [`VpnProxyPort`](luma_contracts::VpnProxyPort) impl: the local SOCKS5 URL
/// this module's bridge exposes, derived from settings. The composition root
/// registers it so downloads / indexers route through the bridge without ever
/// depending on this crate.
pub struct VpnProxy;

impl luma_contracts::VpnProxyPort for VpnProxy {
    fn proxy_url(&self, host: &dyn HostCtx) -> Option<String> {
        wg_configured(host).then(|| {
            let port = host.setting_i64("vpnLocalPort", 25345).clamp(1, 65535);
            format!("socks5://127.0.0.1:{port}")
        })
    }
}

/// `GET /api/admin/vpn` the VPN configuration card's state. VPN routing is
/// WireGuard-only: a stored config runs a managed wireproxy bridge the embedded
/// engine routes through.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnAdminView {
    /// A WireGuard config is stored.
    pub wg_configured: bool,
    /// The bridge child is currently alive.
    pub bridge_running: bool,
    pub local_port: u16,
    pub status: Option<luma_domain::VpnStatusView>,
}

/// `PUT /api/admin/vpn` body. `wgConfig` is write-only: pass the full WireGuard
/// config text from any provider (Mullvad, Proton, AirVPN, a self-hosted peer),
/// or an empty string to remove it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveVpnBody {
    pub wg_config: Option<String>,
    pub local_port: Option<u16>,
}

/// `POST /api/admin/vpn/test` a live probe through (and around) the proxy.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnTestResult {
    pub sealed: bool,
    pub proxied_ip: Option<String>,
    pub direct_ip: Option<String>,
    pub error: Option<String>,
}

/// This module's id (matches its `module.json`).
pub const MODULE_ID: &str = "dev.luma.vpn";

/// The VPN sub-module: serves the bridge's admin routes and, on enable, brings
/// the WireGuard-to-SOCKS5 bridge up (from the stored config); on disable, tears
/// it down so nothing is left tunnelling. It resolves its own [`Vpn`] through the
/// host's service registry.
pub struct VpnModule;

#[async_trait]
impl<S: HostCtx + Clone + Send + Sync + 'static> ServerModule<S> for VpnModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn admin_routes(&self, _host: &S) -> Option<Router<S>> {
        Some(routes::routes::<S>())
    }

    async fn on_enable(&self, host: Arc<dyn HostCtx>) {
        if let Some(vpn) = service::<Vpn>(host.as_ref()) {
            vpn.apply(host.as_ref()).await;
        }
    }

    async fn on_disable(&self, host: Arc<dyn HostCtx>) {
        if let Some(vpn) = service::<Vpn>(host.as_ref()) {
            vpn.stop().await;
        }
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module<S: HostCtx + Clone + Send + Sync + 'static>() -> Box<dyn ServerModule<S>> {
    Box::new(VpnModule)
}
