//! mDNS / DNS-SD advertising so LAN clients can find the server without manual
//! configuration.
//!
//! Advertises a `_luma._tcp` service and resolves the hostname `luma.local` to
//! this machine's LAN address(es). Browsers / TV webviews can't *browse* mDNS
//! from JavaScript, but many client OSes resolve a `.local` hostname so a
//! client can simply try `http://luma.local:<port>` and reach us with no IP
//! entry. Best-effort: if mDNS can't start (no multicast, etc.) the server runs
//! fine without it.

use std::net::{IpAddr, UdpSocket};

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::info;

pub const HOSTNAME: &str = "luma.local.";
pub const SERVICE_TYPE: &str = "_luma._tcp.local.";

/// Start advertising on `port`. Returns the running daemon keep it alive for
/// the process lifetime (dropping it unregisters the service).
pub fn advertise(port: u16, instance: &str) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;

    // TXT records: where the API lives + our version, for richer clients.
    let props = [("path", "/api"), ("version", env!("CARGO_PKG_VERSION"))];

    // Advertise only the primary LAN IP. `enable_addr_auto` would publish every
    // interface (Docker bridges, VPNs, …), and a client could pick a dead one.
    let service = match primary_lan_ip() {
        Some(ip) => {
            let ip = ip.to_string();
            info!("mDNS: advertising {HOSTNAME} → {ip}:{port} ({SERVICE_TYPE})");
            ServiceInfo::new(SERVICE_TYPE, instance, HOSTNAME, ip.as_str(), port, &props[..])?
        }
        None => {
            info!("mDNS: advertising {SERVICE_TYPE} on :{port} (auto addresses)");
            ServiceInfo::new(SERVICE_TYPE, instance, HOSTNAME, "", port, &props[..])?.enable_addr_auto()
        }
    };

    daemon.register(service)?;
    Ok(daemon)
}

/// The primary outbound LAN IPv4 the source address the OS would use to reach
/// the internet. Found by "connecting" a UDP socket (no packets are sent) and
/// reading its local address.
fn primary_lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

pub mod module;
pub use module::MODULE;

use std::sync::{Arc, Mutex};

use luma_module_sdk::host::{async_trait, HostCtx, ServerModule};

/// This module's id (matches `module.json`).
pub const MODULE_ID: &str = "dev.luma.mdns";

/// The mDNS module's backend behavior: on enable it advertises the core's port
/// over mDNS (gated on the `localDiscovery` setting) and holds the daemon; on
/// disable it drops it. It has no routes — it's a lifecycle-only service.
#[derive(Default)]
pub struct MdnsModule {
    daemon: Mutex<Option<ServiceDaemon>>,
}

/// The core's listen port, from the `LUMA_CORE_URL` the supervisor set (so the
/// advertised `.local` address points at the server, not this helper process).
fn core_port() -> Option<u16> {
    std::env::var("LUMA_CORE_URL").ok()?.rsplit(':').next()?.trim_end_matches('/').parse().ok()
}

#[async_trait]
impl<S: HostCtx + Clone + Send + Sync + 'static> ServerModule<S> for MdnsModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    async fn on_enable(&self, host: Arc<dyn HostCtx>) {
        if !host.setting_bool("localDiscovery", true) {
            info!("mDNS: local discovery disabled in settings");
            return;
        }
        let Some(port) = core_port() else {
            tracing::warn!("mDNS: no LUMA_CORE_URL port; not advertising");
            return;
        };
        match advertise(port, "LUMA") {
            Ok(daemon) => *self.daemon.lock().unwrap() = Some(daemon),
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "mDNS advertising unavailable"),
        }
    }

    async fn on_disable(&self, _host: Arc<dyn HostCtx>) {
        // Dropping the daemon unregisters the service.
        self.daemon.lock().unwrap().take();
    }
}

/// This module's backend behavior, for the out-of-process runtime.
pub fn server_module<S: HostCtx + Clone + Send + Sync + 'static>() -> Box<dyn ServerModule<S>> {
    Box::new(MdnsModule::default())
}
