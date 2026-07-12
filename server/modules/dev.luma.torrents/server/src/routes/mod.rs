//! The Downloads module's admin API (`/api/admin/*`), relocated out of the
//! binary so the `dev.luma.torrents` module owns its whole server-side vertical:
//! the torrent engines ([`clients`]), the download queue + history ([`queue`]),
//! and the library file-organize tool ([`organize`]).
//!
//! Unlike the vpn / indexer module routes (generic over any [`HostCtx`]), these
//! handlers take the app's concrete `SharedState`: they orchestrate the
//! organize vertical, which runs against `luma-engine`'s
//! `AppState` (settings / config / DB) directly. This crate already depends on
//! `luma-engine`, so naming `SharedState` here is free; capability gating and the
//! download-manager lookup still go through the shared [`HostCtx`] seam, exactly
//! like every other module.

mod clients;
mod organize;
mod queue;

use axum::Router;

use luma_engine::state::SharedState;

/// The Downloads module's full admin router: engines, queue and organize merged.
/// Mounted behind the module's enabled-gate by the host, so the whole surface
/// 404s while the module is disabled.
pub fn routes() -> Router<SharedState> {
    clients::routes().merge(queue::routes()).merge(organize::routes())
}
