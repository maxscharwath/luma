//! API surface: route table and JSON helpers.

pub mod admin;
pub mod card;
pub mod error;
pub mod handlers;
pub mod playback;
pub mod poster;
pub mod users;
pub mod ws;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// Build the application router with all `/api` routes plus CORS and tracing.
pub fn router(state: SharedState) -> Router {
    let api = Router::new()
        .route("/health", get(handlers::health))
        .route("/libraries", get(handlers::list_libraries))
        .route("/items", get(handlers::list_items))
        .route("/movies", get(handlers::list_movies))
        .route("/shows", get(handlers::list_shows))
        .route("/shows/:id", get(handlers::get_show))
        .route("/shows/:id/poster", get(handlers::show_poster))
        .route("/shows/:id/metadata", get(handlers::show_metadata))
        .route("/items/:id", get(handlers::get_item))
        .route("/items/:id/stream", get(handlers::stream_item))
        .route("/items/:id/hls/:variant/index.m3u8", get(handlers::hls_playlist))
        .route("/items/:id/hls/:variant/:file", get(handlers::hls_segment))
        .route("/items/:id/poster", get(handlers::item_poster))
        .route("/items/:id/card", get(handlers::item_card))
        .route("/items/:id/metadata", get(handlers::item_metadata))
        .route("/items/:id/subtitles/:track", get(handlers::subtitles))
        .route("/images/:name", get(handlers::image))
        .route("/events", get(ws::events))
        .route("/status", get(handlers::status))
        .route("/logs", get(handlers::logs))
        .route("/scan", post(handlers::rescan))
        // --- accounts / sessions / profiles ---
        .route("/auth/register", post(users::register))
        .route("/auth/login", post(users::login))
        .route("/auth/logout", post(users::logout))
        .route("/auth/me", get(users::me).patch(users::update_me))
        .route("/auth/quickconnect/initiate", post(users::quick_initiate))
        .route("/auth/quickconnect/authorize", post(users::quick_authorize))
        .route("/auth/quickconnect/poll", get(users::quick_poll))
        .route("/users", get(users::list_users))
        .route(
            "/users/avatar",
            post(users::upload_avatar).layer(DefaultBodyLimit::max(users::MAX_AVATAR_BYTES)),
        )
        // --- invitations (registration is invite-only after the owner) ---
        .route("/invites", post(users::create_invite).get(users::list_invites))
        .route(
            "/invites/:token",
            get(users::check_invite).delete(users::delete_invite),
        )
        // --- playback progress / resume ---
        .route("/progress", get(users::list_progress))
        .route("/continue", get(users::continue_watching))
        .route(
            "/progress/:id",
            get(users::get_progress)
                .put(users::save_progress)
                .delete(users::delete_progress),
        )
        // --- live playback sessions (admin dashboard "En cours de lecture") ---
        .route("/playback/ping", post(playback::ping))
        .route("/playback/stop", post(playback::stop))
        // --- admin console ---
        .route("/admin/server", get(admin::server_info))
        .route("/admin/sessions", get(admin::sessions))
        .route("/admin/sessions/:id/stop", post(admin::terminate_session))
        .route("/admin/metrics", get(admin::metrics))
        .route("/admin/storage", get(admin::storage))
        .route("/admin/cache/clear", post(admin::clear_cache))
        .route("/admin/users", get(admin::list_users))
        .route(
            "/admin/users/:id",
            axum::routing::patch(admin::update_user).delete(admin::delete_user),
        )
        .route(
            "/admin/libraries",
            get(admin::list_libraries).post(admin::create_library),
        )
        .route(
            "/admin/libraries/:id",
            axum::routing::patch(admin::update_library).delete(admin::delete_library),
        )
        .route("/admin/libraries/:id/scan", post(admin::scan_library))
        .route(
            "/admin/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/admin/stats/top-users", get(admin::top_users))
        .route("/admin/stats/history", get(admin::history))
        .route("/admin/stats/overview", get(admin::overview));

    let mut app = Router::new().nest("/api", api);

    // Single-binary deploy: serve the built web SPA on the same origin as the API.
    // Static assets are served from disk; any unmatched route falls back to the
    // SPA shell so client-side routing (e.g. /films, /movie/:id) works on refresh.
    // Skipped in dev (no LUMA_WEB_DIR) where the web runs on its own Vite server.
    if let Some(web_dir) = state.config.web_dir.clone() {
        let shell = web_dir.join("_shell.html");
        app = app.fallback_service(ServeDir::new(web_dir).fallback(ServeFile::new(shell)));
    }

    app.layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
