//! The Hello WASM demo module's backend, a sandboxed extism guest.
//!
//! It exports one function, `handle_http`, that the host proxies at
//! `/api/plugin/dev.luma.hellowasm/*`. Only decisions/JSON cross the boundary --
//! never file bytes. This proves a runtime-installed module can serve its own API
//! with no axum routes and no server rebuild.

use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct HttpReq {
    method: String,
    /// Path after the `/api/plugin/<id>` prefix (e.g. "/ping").
    path: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    #[allow(dead_code)]
    body: String,
}

#[derive(Serialize)]
struct HttpResp {
    status: u16,
    body: String,
}

#[plugin_fn]
pub fn handle_http(Json(req): Json<HttpReq>) -> FnResult<Json<HttpResp>> {
    let resp = match req.path.as_str() {
        "/ping" => HttpResp {
            status: 200,
            body: serde_json::json!({
                "message": "hello from a runtime-loaded WASM module",
                "method": req.method,
                "query": req.query,
            })
            .to_string(),
        },
        other => HttpResp {
            status: 404,
            body: serde_json::json!({ "error": "not found", "path": other }).to_string(),
        },
    };
    Ok(Json(resp))
}
