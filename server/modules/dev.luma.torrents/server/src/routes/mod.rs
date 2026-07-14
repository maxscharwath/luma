//! The Downloads module's admin API (`/api/admin/*`), relocated out of the
//! binary so the `dev.luma.torrents` module owns its whole server-side vertical:
//! the torrent engines ([`clients`]), the download queue + history ([`queue`]),
//! and the library file-organize tool ([`organize`]).
//!
//! Every handler is generic over the host state `S: HostCtx`, so the module runs
//! both in-process (`S = SharedState`) and out-of-process (`S = RemoteHost`, its
//! `.lmod` form). The organize vertical reaches settings + library folders + the
//! DB through the [`HostCtx`] seam, never naming the engine's `AppState`.

mod clients;
mod organize;
mod queue;

use axum::Router;

use luma_module_sdk::host::HostCtx;

/// The Downloads module's full admin router: engines, queue and organize merged.
/// Mounted behind the module's enabled-gate by the host, so the whole surface
/// 404s while the module is disabled.
pub fn routes<S: HostCtx + Clone + Send + Sync + 'static>() -> Router<S> {
    clients::routes::<S>().merge(queue::routes::<S>()).merge(organize::routes::<S>())
}
