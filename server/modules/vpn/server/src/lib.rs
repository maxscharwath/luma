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

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::process::Child;

use luma_module_host::HostCtx;

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

    /// Whether a WireGuard config is stored (drives `active_proxy_url`).
    pub fn wg_configured(host: &dyn HostCtx) -> bool {
        !host.setting_str("vpnWgConfig", "").trim().is_empty()
    }

    /// The local SOCKS5 the bridge exposes when configured.
    pub fn local_proxy_url(host: &dyn HostCtx) -> String {
        let port = host.setting_i64("vpnLocalPort", 25345).clamp(1, 65535);
        format!("socks5://127.0.0.1:{port}")
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
