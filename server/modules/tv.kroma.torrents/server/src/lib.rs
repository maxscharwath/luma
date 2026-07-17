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

// The axum `Response` is intentionally the Err type of request guards so handlers
// short-circuit with `?`; boxing every guard for `result_large_err` would churn
// dozens of signatures for no real gain on these error paths.
#![allow(clippy::result_large_err)]

#[cfg(feature = "rqbit")]
mod announce;
// The organize vertical, moved out of the core kroma-engine crate so the core
// depends on ZERO module crates. The acquisition vertical (search / grab / auto /
// import) lives in its own `kroma-acquisition` crate, which depends on THIS crate
// (never the reverse), so disabling Acquisition gates that whole feature.
pub mod db;
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
// Our proxied tracker announce (peers incl. IPv6), re-exported for the
// `engine_probe` diagnostic example.
#[cfg(feature = "rqbit")]
pub use announce::tracker_peers as announce_peers;
// The `downloads` ledger table moved into this crate; the app's request/discover
// overlay reads its live-grab roll-up, so re-export those two at the crate root
// (the binary names `kroma_torrent::requests_with_active_downloads`).
pub use db::{requests_with_active_downloads, ActiveDownload};
// The download manager + monitor (merged in from the former kroma-downloads crate),
// re-exported at the crate root so `kroma_torrent::DownloadManager` etc. keep working.
pub use downloads::{active_proxy_url, DownloadDb, DownloadManager, GrabSpec, LABEL};

/// Whether the embedded engine is compiled into this build.
pub const RQBIT_COMPILED: bool = cfg!(feature = "rqbit");

/// This module's id, shared with `module.json` and the frontend package. The one
/// place callers (route gate, job guards, monitor, lifecycle) name the module.
pub const MODULE_ID: &str = "tv.kroma.torrents";

/// The Downloads module's backend behavior: it serves the queue / download-client
/// / organize admin routes (behind its enabled-gate) and drives the librqbit
/// engine lifecycle, so disabling it 404s those routes and stops the running
/// engine. Its download sub-engines (rqbit / transmission / qBittorrent) plug into
/// the `DownloadClientRegistry`; VPN is a separate module this one
/// `optionalDependsOn`. It reaches its [`DownloadManager`] through the host service
/// registry.
///
/// Generic over the host state `S: HostCtx`, like every other module: it runs
/// in-process (`S = SharedState`) and out-of-process (`S = RemoteHost`, its
/// `.kmod` form). The organize vertical reaches settings + library folders + the
/// DB through the [`HostCtx`] seam, never naming the engine's `AppState`.
pub struct DownloadsModule;

#[kroma_module_sdk::host::async_trait]
impl<S: kroma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>
    kroma_module_sdk::host::ServerModule<S> for DownloadsModule
{
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn migrations(&self) -> &'static str {
        db::MIGRATIONS
    }

    fn admin_routes(&self, _host: &S) -> Option<axum::Router<S>> {
        Some(routes::routes::<S>())
    }

    async fn on_enable(&self, host: std::sync::Arc<dyn kroma_module_sdk::host::HostCtx>) {
        // Everything the Downloads module needs at (re)enable lives here, so the
        // binary shell never seeds rows or spawns the monitor: seed the embedded
        // client row, start the engine, flip disable-paused rows back to active,
        // and ensure the resident monitor is running (spawned once). The VPN bridge
        // is its own module (ordered first by the dependency graph), so its SOCKS5
        // is already up. Awaited (not detached) so a following disable cannot race.
        if let Some(downloads) = kroma_module_sdk::host::service::<DownloadManager>(host.as_ref()) {
            downloads.seed_embedded_client(host.as_ref());
            downloads.start_rqbit(host.as_ref()).await;
            downloads.resume_after_enable(host.as_ref());
            downloads.ensure_monitor(host.clone());
        }
    }

    async fn on_disable(&self, host: std::sync::Arc<dyn kroma_module_sdk::host::HostCtx>) {
        // Tear the engine down entirely (session stopped, active downloads paused)
        // so nothing is left transferring or seeding while disabled.
        if let Some(downloads) = kroma_module_sdk::host::service::<DownloadManager>(host.as_ref()) {
            downloads.disable_embedded(host.as_ref());
        }
    }
}

/// This module's backend behavior, for the host's generic module roster. Generic
/// over the host state so both the in-core roster (`S = SharedState`) and the
/// `.kmod` binary (`S = RemoteHost`) construct it.
pub fn server_module<S: kroma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>(
) -> Box<dyn kroma_module_sdk::host::ServerModule<S>> {
    Box::new(DownloadsModule)
}

// The download-client contract (engine trait + shared types + magnet_info_hash)
// lives in the SDK ports module (kroma_module_sdk::ports) now, so download engine modules depend only on the SDK.
// Re-exported so this crate's own modules keep using crate::DownloadClient etc.
pub use kroma_module_sdk::ports::{
    magnet_info_hash, AddTorrentReq, ClientDef, DownloadClient, DownloadClientCtx,
    DownloadClientHost, DownloadClientRegistry, TorrentFileEntry, TorrentState, TorrentStatus,
    VpnStatusView,
};


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
            reg.register("rqbit", |_def, ctx| {
                // The ctx carries the embedded engine as an opaque `dyn Any` (the
                // contract lives in the SDK ports module (kroma_module_sdk::ports) and knows nothing of RqbitEngine).
                let any = ctx.rqbit.as_ref().ok_or_else(|| anyhow::anyhow!("embedded engine not started"))?;
                let engine = any
                    .clone()
                    .downcast::<RqbitEngine>()
                    .map_err(|_| anyhow::anyhow!("rqbit handle type mismatch"))?;
                Ok(engine.client())
            });
            #[cfg(not(feature = "rqbit"))]
            reg.register("rqbit", |_def, _ctx| {
                anyhow::bail!("embedded engine not compiled (torrent-rqbit feature off)")
            });
            true
        }
        _ => false,
    }
}

/// Register the core download engine (embedded librqbit). Transmission and
/// qBittorrent are their own crates now (`kroma-transmission` / `kroma-qbittorrent`)
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
}
