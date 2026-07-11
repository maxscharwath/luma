//! Runtime-module surface: proxy HTTP to a WASM module's `handle_http` export,
//! and serve an installed module's frontend (Module Federation) assets.
//!
//! - `/api/plugin/:id/*path` (session-gated + enabled-gated): forwards the
//!   request to the module's guest and returns its response, so an installed
//!   module serves its own API with no axum routes. A disabled module 404s.
//! - `/modules/:id/*path` (public, at the root): serves `<data>/modules/:id/fe/*`
//!   -- the `remoteEntry.js` + chunks the frontend `loadRemote`s (no bearer, like
//!   the icon route). Mounted before the SPA fallback.

use std::path::{Component, PathBuf};

use axum::body::{Body, Bytes};
use axum::extract::{Path, RawQuery, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Router;

use luma_module_wasm::{HttpReq, HttpResp};

use crate::state::SharedState;

/// Session-gated proxy, relative to `/api`.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/plugin/:id/*path", any(proxy))
}

/// Public FE-asset serving, mounted at the app root (before the SPA fallback).
pub fn asset_routes() -> Router<SharedState> {
    Router::new().route("/modules/:id/*path", get(serve_fe))
}

async fn proxy(
    State(state): State<SharedState>,
    Path((id, path)): Path<(String, String)>,
    method: Method,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    // A disabled module's endpoints disappear, matching its nav + pages.
    if !luma_engine::modules::module_enabled(&state.settings, &id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let req = HttpReq {
        method: method.as_str().to_string(),
        path: format!("/{}", path.trim_start_matches('/')),
        query: query.unwrap_or_default(),
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    // extism `call` is blocking; run it off the async runtime.
    let wasm = state.wasm.clone();
    let out = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<HttpResp>> {
        let host = wasm.read().map_err(|_| anyhow::anyhow!("wasm host lock poisoned"))?;
        if host.find(&id).is_none() {
            return Ok(None);
        }
        Ok(Some(host.handle_http(&id, &req)?))
    })
    .await;
    match out {
        Ok(Ok(Some(resp))) => build_response(resp),
        Ok(Ok(None)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, format!("module error: {e:#}")).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn build_response(resp: HttpResp) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);
    let mut had_content_type = false;
    for (k, v) in &resp.headers {
        if k.eq_ignore_ascii_case("content-type") {
            had_content_type = true;
        }
        if let (Ok(name), Ok(val)) = (HeaderName::try_from(k), HeaderValue::try_from(v)) {
            builder = builder.header(name, val);
        }
    }
    if !had_content_type {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder
        .body(Body::from(resp.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn serve_fe(State(state): State<SharedState>, Path((id, path)): Path<(String, String)>) -> Response {
    // Rebuild both id and path from Normal components so neither can escape the
    // module's fe dir.
    let Some(id) = safe_segment(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let rel: PathBuf = PathBuf::from(&path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    if rel.as_os_str().is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file = state.config.data_dir.join("modules").join(id).join("fe").join(&rel);
    match tokio::fs::read(&file).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, content_type_for(&rel)),
                // remoteEntry.js is unhashed + refetched on each app load; assets are
                // content-hashed. `no-cache` is safe for both (revalidate, not stale).
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A single path segment that is a plain name (no separators / traversal).
fn safe_segment(s: &str) -> Option<&str> {
    if s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\']) {
        None
    } else {
        Some(s)
    }
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("map") => "application/json",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}
