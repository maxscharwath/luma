//! Uniform JSON error responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Build a JSON error response: `{ "error": "<message>" }` with the given status.
pub fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Localised JSON error: resolves message `key` in `locale` against the shared
/// catalogs (`packages/core/src/locales`).
pub fn lerr(locale: &str, status: StatusCode, key: &str) -> Response {
    json_error(status, &crate::i18n::t(locale, key, &[]))
}
