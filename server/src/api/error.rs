//! Uniform JSON error responses.

use axum::http::StatusCode;
use axum::response::Response;

/// Build a JSON error response: `{ "error": "<message>" }` with the given status.
/// Defined in kroma-engine (shared with `infra::stream`); re-exported here so the
/// `crate::api::error::json_error` call sites are unchanged.
pub use kroma_engine::json_error;

/// Localised JSON error: resolves message `key` in `locale` against the shared
/// catalogs (`packages/core/src/locales`).
pub fn lerr(locale: &str, status: StatusCode, key: &str) -> Response {
    json_error(status, &crate::i18n::t(locale, key, &[]))
}
