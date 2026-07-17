//! Managed WireGuard-to-SOCKS5 bridge for torrent traffic, Proton VPN first.
//!
//! Proton (like most consumer VPNs) exposes no public SOCKS5 endpoint, but it
//! hands out standard WireGuard configs (account.protonvpn.com -> Downloads ->
//! WireGuard configuration; pick a P2P server). KROMA turns such a config into
//! a LOCAL SOCKS5 proxy by supervising a `wireproxy` child (userspace
//! WireGuard, single static binary), self-provisioned exactly like the
//! `cloudflared` connector in the `kroma-remote` crate. The embedded
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

use kroma_module_sdk::host::{async_trait, service, HostCtx, ServerModule};

/// This module's registry entry (manifest + packaged icon, embedded at compile
/// time from the shared module folder).
use kroma_module_sdk::EmbeddedModule;
pub const MODULE: EmbeddedModule = kroma_module_sdk::embedded_module!();

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
        // Force the tunnel IPv4-only. wireproxy carries IPv4 BitTorrent peer
        // connections fine but STALLS IPv6 ones mid-handshake (Proton's IPv6
        // P2P path). Worse, an IPv6-capable tunnel makes the client announce to
        // trackers over IPv6, so the tracker returns IPv6 peers (which then
        // can't be reached) instead of the IPv4 peers wireproxy can use. See
        // `ipv4_only_wg`.
        std::fs::write(&wg_path, ipv4_only_wg(wg_config.trim()) + "\n")
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

        // The module supervisor stops sidecars with SIGKILL, which orphans the
        // wireproxy child of a previous run. A stale bridge keeps the SOCKS
        // port bound with an outdated config, so every fresh spawn here fails
        // with "address in use" while traffic silently rides the old tunnel.
        reap_stale(&dir);
        let child = spawn_child(&bin, &conf_path)?;
        record_pid(&dir, &child);
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
                        Ok(child) => {
                            record_pid(&me.dir(), &child);
                            *me.child.lock().await = Some(child);
                        }
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

/// Rewrite a WireGuard config so the tunnel is IPv4-only: drop IPv6 entries from
/// `Address` and `DNS`, and reduce `AllowedIPs` to IPv4. Everything else (keys,
/// `Endpoint`, `PersistentKeepalive`) is left untouched. This keeps torrent
/// traffic on IPv4, which the wireproxy bridge relays reliably (IPv6 peer
/// connections stall mid-handshake) and which makes trackers hand back IPv4
/// peers. Guard: if a line has no IPv4 value at all it is kept verbatim, so an
/// (unusual) IPv6-only config is never left with an empty address.
fn ipv4_only_wg(config: &str) -> String {
    let is_v4 = |s: &str| !s.contains(':');
    let keep_v4 = |val: &str| -> Option<String> {
        let v4: Vec<&str> = val.split(',').map(str::trim).filter(|a| !a.is_empty() && is_v4(a)).collect();
        (!v4.is_empty()).then(|| v4.join(", "))
    };
    let mut out = Vec::new();
    for line in config.lines() {
        let trimmed = line.trim_start();
        let rewrite = |key: &str| trimmed.strip_prefix(key).map(|rest| rest.trim_start().strip_prefix('=').unwrap_or(rest).trim());
        if let Some(val) = rewrite("Address") {
            match keep_v4(val) {
                Some(v4) => out.push(format!("Address = {v4}")),
                None => out.push(line.to_string()),
            }
        } else if let Some(val) = rewrite("DNS") {
            match keep_v4(val) {
                Some(v4) => out.push(format!("DNS = {v4}")),
                None => out.push(line.to_string()),
            }
        } else if trimmed.starts_with("AllowedIPs") {
            // Drop ::/0 and any IPv6 CIDR; ensure IPv4 default route remains.
            out.push("AllowedIPs = 0.0.0.0/0".to_string());
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

fn pidfile(dir: &std::path::Path) -> PathBuf {
    dir.join("wireproxy.pid")
}

/// Remember the bridge child's pid so the NEXT process generation can reap it
/// if this one dies without cleanup (SIGKILL from the supervisor).
fn record_pid(dir: &std::path::Path, child: &Child) {
    if let Some(pid) = child.id() {
        let _ = std::fs::write(pidfile(dir), pid.to_string());
    }
}

/// Kill a wireproxy orphaned by a previous run, identified by the pidfile and
/// verified by process name (never kill a reused pid). Best-effort.
#[cfg(unix)]
fn reap_stale(dir: &std::path::Path) {
    let path = pidfile(dir);
    let Some(pid) = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return;
    };
    let _ = std::fs::remove_file(&path);
    let comm = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    let is_wireproxy =
        comm.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("wireproxy"));
    if is_wireproxy {
        tracing::warn!(pid, "killing a stale wireproxy from a previous run");
        let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

#[cfg(not(unix))]
fn reap_stale(_dir: &std::path::Path) {}

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

/// The [`VpnProxyPort`](kroma_module_sdk::ports::VpnProxyPort) impl: the local SOCKS5 URL
/// this module's bridge exposes, derived from settings. The composition root
/// registers it so downloads / indexers route through the bridge without ever
/// depending on this crate.
pub struct VpnProxy;

impl kroma_module_sdk::ports::VpnProxyPort for VpnProxy {
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
    pub status: Option<kroma_module_sdk::ports::VpnStatusView>,
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
pub const MODULE_ID: &str = "tv.kroma.vpn";

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

#[cfg(test)]
mod tests {
    use super::ipv4_only_wg;

    #[test]
    fn strips_ipv6_from_dual_stack_config() {
        let cfg = "[Interface]\n\
                   Address = 10.2.0.2/32, 2a07:b944::2:2/128\n\
                   DNS = 10.2.0.1, 2a07:b944::2:1\n\
                   PrivateKey = KEY\n\n\
                   [Peer]\n\
                   PublicKey = PUB\n\
                   AllowedIPs = 0.0.0.0/0, ::/0\n\
                   Endpoint = 89.222.96.158:51820\n\
                   PersistentKeepalive = 25";
        let out = ipv4_only_wg(cfg);
        assert!(out.contains("Address = 10.2.0.2/32"));
        assert!(!out.contains("2a07:b944"), "IPv6 address/DNS must be gone:\n{out}");
        assert!(out.contains("DNS = 10.2.0.1"));
        assert!(out.contains("AllowedIPs = 0.0.0.0/0"));
        assert!(!out.contains("::/0"));
        // Untouched lines survive.
        assert!(out.contains("Endpoint = 89.222.96.158:51820"));
        assert!(out.contains("PersistentKeepalive = 25"));
        assert!(out.contains("PrivateKey = KEY"));
    }

    #[test]
    fn ipv4_only_config_is_unchanged_semantically() {
        let cfg = "[Interface]\nAddress = 10.2.0.2/32\nDNS = 10.2.0.1\n[Peer]\nAllowedIPs = 0.0.0.0/0";
        let out = ipv4_only_wg(cfg);
        assert!(out.contains("Address = 10.2.0.2/32"));
        assert!(out.contains("DNS = 10.2.0.1"));
        assert!(out.contains("AllowedIPs = 0.0.0.0/0"));
    }

    #[test]
    fn ipv6_only_address_is_kept_verbatim() {
        // No IPv4 to fall back to: don't strip to an empty Address.
        let cfg = "[Interface]\nAddress = 2a07:b944::2:2/128\n[Peer]\nAllowedIPs = ::/0";
        let out = ipv4_only_wg(cfg);
        assert!(out.contains("Address = 2a07:b944::2:2/128"));
    }
}
