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
mod modules;
mod people;
mod pin;
mod recommend;
mod requests;
mod search;
mod online_subs;
mod passkeys;
mod plugin;
mod stream;
mod suggest;
mod themes;
mod util;

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Router};
use luma_module_supervisor::Supervisor;
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::api::error::json_error;
use crate::api::extract::bearer_from_headers;
use crate::state::SharedState;

/// Require a valid session (bearer) token on the content routes. Rejects a
/// missing/expired/unknown token with `401` before the handler runs, so the
/// catalogue can't be listed anonymously. Public routes (auth handshake, roster,
/// invites, avatars, health, media bytes) are merged OUTSIDE this layer.
async fn require_session(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    let Some(token) = bearer_from_headers(req.headers()) else {
        return json_error(StatusCode::UNAUTHORIZED, "authentication required");
    };
    let pool = state.db.clone();
    let ok = tokio::task::spawn_blocking(move || crate::db::session_user(&pool, &token))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .is_some();
    if ok {
        next.run(req).await
    } else {
        json_error(StatusCode::UNAUTHORIZED, "authentication required")
    }
}

/// Build the application router with all `/api` routes plus CORS and tracing.
/// Reverse-proxy `/api/module/<id>/<rest>` to the installed module's process
/// (the module validates the forwarded bearer itself against the shared DB).
async fn module_proxy(
    Extension(sup): Extension<Arc<Supervisor>>,
    Path((id, rest)): Path<(String, String)>,
    req: Request,
) -> Response {
    match sup.port_of(&id) {
        Some(port) => {
            let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
            luma_module_supervisor::proxy_to(port, &format!("/{rest}{query}"), req).await
        }
        None => (StatusCode::NOT_FOUND, "module not running").into_response(),
    }
}

pub fn router(state: SharedState, supervisor: Arc<Supervisor>) -> Router {
    // Public endpoints reachable before (or without) a session: the auth
    // handshake + roster + invites, uploaded avatars/art, liveness, and the media
    // byte streams (a `<video>`/hls element can't attach a bearer these carry no
    // catalogue listing and stay open under the LAN trust model).
    let public = Router::new()
        .merge(accounts::routes())
        .merge(passkeys::routes())
        .merge(pin::routes())
        .merge(invites::routes())
        .merge(images::routes())
        .merge(media::public_routes())
        .merge(stream::routes())
        .merge(online_subs::public_routes())
        .merge(themes::routes())
        .merge(modules::public_routes())
        .merge(ws::routes());

    // Content endpoints require a valid session: the catalogue listing + detail,
    // search, people, metadata, discovery/requests, home rows and per-user
    // playback. Knowing the URL no longer lists the library. `themes` +
    // downloaded-subtitle bytes are served publicly above they're fetched by an
    // <audio> element / plain fetch that can't attach a bearer.
    let content = Router::new()
        .merge(media::routes())
        .merge(search::routes())
        .merge(people::routes())
        .merge(metadata::routes())
        .merge(recommend::routes())
        .merge(suggest::routes())
        .merge(online_subs::routes())
        .merge(home::routes())
        .merge(playback::routes())
        .merge(discover::routes())
        .merge(requests::routes())
        .merge(modules::routes())
        .merge(plugin::routes())
        .route_layer(from_fn_with_state(state.clone(), require_session));

    // Each feature module owns its routes via a `routes()` function. The admin
    // subtree gets its own `/admin` prefix and self-gates per-handler (permission
    // checks), so it lives outside the blanket content layer.
    // Out-of-process (.lmod) modules: the /api/_host/* callback API they call back
    // into (token-authed, resolved against the core's HostCtx), and a reverse
    // proxy `/api/module/<id>/*` forwarding to the installed module's process.
    let api = public
        .merge(content)
        .merge(luma_module_supervisor::host_router::<SharedState>(
            supervisor.host_token().to_string(),
        ))
        .route("/module/:id/*rest", axum::routing::any(module_proxy))
        .nest("/admin", admin::routes(state.clone()))
        .layer(Extension(supervisor));

    let mut app = Router::new().nest("/api", api);

    // Installed modules' frontend (Module Federation) assets, served from
    // `<data>/modules/<id>/fe/` at `/modules/<id>/*`, same origin as the API and
    // BEFORE the SPA fallback so an installed remote's `remoteEntry.js` resolves.
    app = app.merge(plugin::asset_routes());

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
