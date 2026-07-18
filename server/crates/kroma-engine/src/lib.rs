//! KROMA engine: everything the HTTP layer drives, below the router.
//!
//! This crate holds the outbound adapters (`infra`), the business logic
//! (`services`), the composed application state (`state`), the request-locale
//! extractor (`i18n`), and the flat wire-model barrel (`model`, re-exporting
//! [`kroma_domain`]). The `kroma-server` binary is a thin `api` router over it.
//!
//! Lower layers live in their own crates and are aliased here so the many
//! `crate::db::…` / `crate::config::…` / `crate::domain::…` call sites keep
//! resolving after the split.

use std::sync::OnceLock;
use std::time::Instant;

// Lower-layer crates, aliased to their historical in-crate module paths.
pub(crate) use kroma_config as config;
pub(crate) use kroma_db as db;
pub(crate) use kroma_domain as domain;

/// The `{ "error": message }` JSON response builder now lives in the host-seam
/// leaf crate; re-exported so `crate::json_error` / `kroma_engine::json_error`
/// call sites (api handlers, `infra::stream`) are unchanged.
pub use kroma_module_host::json_error;

pub mod host_ctx;
pub mod i18n;
pub mod infra;
pub mod model;
pub mod modules;
pub mod ports;
pub mod services;
pub mod state;

/// Shared `#[cfg(test)]` harness: a minimal real [`state::SharedState`] plus small
/// catalog/ledger seeding helpers, for the services whose logic only runs against
/// a full app state (pipeline dispatcher/reprocess/elements, sections, jobs).
#[cfg(test)]
pub(crate) mod test_support;

/// Process start time, for the admin uptime readout. Seeded on first call
/// (from `main`), read by [`infra::metrics`].
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// When this process started (monotonic). Seeded on first call.
pub fn process_started() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}
