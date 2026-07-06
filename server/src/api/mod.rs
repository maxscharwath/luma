//! API surface: composes each feature module's `routes()` into the `/api`
//! router, then layers CORS, tracing and the SPA fallback. Individual routes
//! live next to their handlers in the submodules, not here.

pub mod admin;
pub mod card;
pub mod dto;
pub mod error;
pub mod playback;
pub mod poster;
pub mod ws;

mod accounts;
mod discover;
mod extract;
mod home;
mod images;
mod invites;
mod media;
mod metadata;
mod people;
mod pin;
mod recommend;
mod requests;
mod search;
mod online_subs;
mod stream;
mod suggest;
mod themes;
mod util;

use axum::Router;
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// Build the application router with all `/api` routes plus CORS and tracing.
pub fn router(state: SharedState) -> Router {
    // Each feature module owns its routes via a `routes()` function, so adding a
    // route means editing the module that handles it, not this table. Modules are
    // flat-merged (their paths span prefixes like `/items` and `/shows`); the
    // admin subtree gets its own `/admin` prefix via `nest`.
    let api = Router::new()
        .merge(media::routes())
        .merge(search::routes())
        .merge(people::routes())
        .merge(metadata::routes())
        .merge(images::routes())
        .merge(stream::routes())
        .merge(recommend::routes())
        .merge(suggest::routes())
        .merge(online_subs::routes())
        .merge(themes::routes())
        .merge(home::routes())
        .merge(ws::routes())
        .merge(playback::routes())
        .merge(accounts::routes())
        .merge(pin::routes())
        .merge(invites::routes())
        .merge(discover::routes())
        .merge(requests::routes())
        .nest("/admin", admin::routes());

    let mut app = Router::new().nest("/api", api);

    // Single-binary deploy: serve the built web SPA on the same origin as the API.
    // Static assets are served from disk; any unmatched route falls back to the
    // SPA shell so client-side routing (e.g. /films, /movie/:id) works on refresh.
    // Skipped in dev (no LUMA_WEB_DIR) where the web runs on its own Vite server.
    // `precompressed_*` serves the `.br`/`.gz` siblings the web build emits
    // (scripts/precompress.mjs), so static bytes cost the NAS zero compression
    // CPU; assets without a sibling fall through to the live CompressionLayer.
    if let Some(web_dir) = state.config.web_dir.clone() {
        let shell = web_dir.join("_shell.html");
        app = app.fallback_service(
            ServeDir::new(web_dir)
                .precompressed_br()
                .precompressed_gzip()
                .fallback(ServeFile::new(shell).precompressed_br().precompressed_gzip()),
        );
    }

    // Compress JSON + SPA assets on the fly (big win for catalog payloads on the
    // LAN). Media bytes are exempt: video/audio streams and HLS segments are
    // already-compressed formats where gzip only burns the NAS CPU, and the image
    // endpoints serve WebP/JPEG (the default predicate already skips image/*).
    let compression = CompressionLayer::new().compress_when(
        DefaultPredicate::new()
            .and(NotForContentType::new("video/"))
            .and(NotForContentType::new("audio/"))
            .and(NotForContentType::new("application/vnd.apple.mpegurl")),
    );

    app.layer(CorsLayer::permissive())
        .layer(compression)
        .layer(axum::middleware::from_fn(spa_cache_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Cache policy for the SPA files: Vite content-hashes every built asset, so
/// hashed files are immutable (cache for a year) while the shell and any
/// unhashed file revalidate (`no-cache` = cached but conditionally refetched).
/// Without this the TV re-downloads the whole bundle on every app launch.
/// `/api/*` responses are left untouched.
async fn spa_cache_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue};

    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    if path.starts_with("/api/") || res.headers().contains_key(header::CACHE_CONTROL) {
        return res;
    }
    let policy = if is_hashed_asset(&path) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
    res
}

/// Whether a request path looks like a Vite content-hashed asset
/// (`Poster-BKMFTghM.js`, `assets/index-DXQwrN_7.css`): a `-<hash>` stem
/// suffix of 8+ [A-Za-z0-9_] chars and a non-HTML extension.
fn is_hashed_asset(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or("");
    if name.ends_with(".html") {
        return false;
    }
    let Some((stem, _ext)) = name.rsplit_once('.') else {
        return false;
    };
    stem.rsplit_once('-')
        .is_some_and(|(_, h)| h.len() >= 8 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}
