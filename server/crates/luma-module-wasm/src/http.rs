//! The HTTP envelope a WASM module's `handle_http` export exchanges with the
//! host as JSON across the extism boundary. The host proxies `/api/plugin/<id>/*`
//! to `handle_http` and turns the [`HttpResp`] back into an axum response.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A request handed across the wasm boundary to a module's `handle_http` export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpReq {
    pub method: String,
    /// Path AFTER the `/api/plugin/<id>` prefix (e.g. "/ping"), always leading-slash.
    pub path: String,
    /// Raw query string (no leading "?"), empty when none.
    #[serde(default)]
    pub query: String,
    /// Request body as a UTF-8 string (JSON APIs), empty when none.
    #[serde(default)]
    pub body: String,
}

/// The response a module's `handle_http` export returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResp {
    #[serde(default = "default_status")]
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
}

fn default_status() -> u16 {
    200
}
