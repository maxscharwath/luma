//! LUMA engine: everything the HTTP layer drives, below the router.
//!
//! This crate holds the outbound adapters (`infra`), the business logic
//! (`services`), the composed application state (`state`), the request-locale
//! extractor (`i18n`), and the flat wire-model barrel (`model`, re-exporting
//! [`luma_domain`]). The `luma-server` binary is a thin `api` router over it.
//!
//! Lower layers live in their own crates and are aliased here so the many
//! `crate::db::…` / `crate::config::…` / `crate::domain::…` call sites keep
//! resolving after the split.

use std::sync::OnceLock;
use std::time::Instant;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

// Lower-layer crates, aliased to their historical in-crate module paths.
pub(crate) use luma_config as config;
pub(crate) use luma_db as db;
pub(crate) use luma_domain as domain;

pub mod i18n;
pub mod infra;
pub mod model;
pub mod modules;
pub mod services;
pub mod state;

/// Process start time, for the admin uptime readout. Seeded on first call
/// (from `main`), read by [`infra::metrics`].
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// When this process started (monotonic). Seeded on first call.
pub fn process_started() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

/// Build a `{ "error": message }` JSON response with the given status. Shared by
/// the api handlers and `infra::stream` (which returns responses directly).
pub fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
