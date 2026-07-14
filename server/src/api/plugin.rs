//! Installed-module frontend assets: serves an installed module's Module
//! Federation remote from disk.
//!
//! - `/modules/:id/*path` (public, at the root): serves `<data>/modules/:id/fe/*`
//!   -- the `remoteEntry.js` + chunks the frontend `loadRemote`s (no bearer, like
//!   the icon route). The supervisor unpacks a `.lmod`'s `fe/` there on install.
//!   Mounted before the SPA fallback.

use std::path::{Component, PathBuf};

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::state::SharedState;

/// Public FE-asset serving, mounted at the app root (before the SPA fallback).
pub fn asset_routes() -> Router<SharedState> {
    Router::new().route("/modules/:id/*path", get(serve_fe))
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
