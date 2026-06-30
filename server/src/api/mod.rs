//! API surface: route table and JSON helpers.

pub mod admin;
pub mod card;
pub mod dto;
pub mod error;
pub mod playback;
pub mod poster;
pub mod ws;

mod accounts;
mod extract;
mod home;
mod images;
mod invites;
mod media;
mod metadata;
mod people;
mod pin;
mod recommend;
mod search;
mod stream;
mod suggest;
mod themes;
mod util;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// Build the application router with all `/api` routes plus CORS and tracing.
pub fn router(state: SharedState) -> Router {
    let api = Router::new()
        .route("/health", get(media::health))
        .route("/libraries", get(media::list_libraries))
        .route("/items", get(media::list_items))
        .route("/movies", get(media::list_movies))
        .route("/shows", get(media::list_shows))
        .route("/search", get(search::search))
        .route("/people", get(people::person))
        .route("/shows/:id", get(media::get_show))
        .route("/shows/:id/up-next", get(playback::up_next))
        .route("/items/:id/next", get(playback::next_episode))
        .route("/shows/:id/poster", get(images::show_poster))
        .route("/shows/:id/metadata", get(metadata::show_metadata))
        .route("/items/:id", get(media::get_item))
        .route("/items/:id/stream", get(stream::stream_item))
        .route("/items/:id/hls/:variant/index.m3u8", get(stream::hls_playlist))
        .route("/items/:id/hls/:variant/:file", get(stream::hls_segment))
        .route("/items/:id/poster", get(images::item_poster))
        .route("/items/:id/card", get(images::item_card))
        .route("/items/:id/metadata", get(metadata::item_metadata))
        .route("/items/:id/similar", get(recommend::similar))
        .route("/items/:id/ai-suggest", get(suggest::ai_suggest))
        .route("/themed", get(recommend::themed))
        .route("/items/:id/subtitles/:track", get(stream::subtitles))
        .route("/images/:name", get(images::image))
        .route("/themes/:name", get(themes::theme))
        .route("/events", get(ws::events))
        .route("/status", get(media::status))
        .route("/logs", get(media::logs))
        .route("/scan", post(media::rescan))
        // --- accounts / sessions / profiles ---
        .route("/auth/register", post(accounts::register))
        .route("/auth/login", post(accounts::login))
        .route("/auth/logout", post(accounts::logout))
        .route("/auth/me", get(accounts::me).patch(accounts::update_me))
        .route("/auth/pin/verify", post(pin::verify_pin))
        .route(
            "/auth/me/pin",
            axum::routing::patch(pin::set_pin).delete(pin::delete_pin),
        )
        .route("/auth/quickconnect/initiate", post(accounts::quick_initiate))
        .route("/auth/quickconnect/authorize", post(accounts::quick_authorize))
        .route("/auth/quickconnect/poll", get(accounts::quick_poll))
        .route("/users", get(accounts::list_users))
        .route(
            "/users/avatar",
            post(accounts::upload_avatar).layer(DefaultBodyLimit::max(accounts::MAX_AVATAR_BYTES)),
        )
        // --- invitations (registration is invite-only after the owner) ---
        .route("/invites", post(invites::create_invite).get(invites::list_invites))
        .route(
            "/invites/:token",
            get(invites::check_invite).delete(invites::delete_invite),
        )
        // --- playback progress / resume ---
        .route("/progress", get(playback::list_progress))
        .route("/continue", get(playback::continue_watching))
        .route("/home", get(home::home))
        .route("/for-you", get(recommend::for_you))
        .route(
            "/progress/:id",
            get(playback::get_progress)
                .put(playback::save_progress)
                .delete(playback::delete_progress),
        )
        // --- watched marker (explicit "seen" state, per user) ---
        .route("/watched", get(playback::list_watched))
        .route(
            "/watched/:id",
            put(playback::mark_watched).delete(playback::unmark_watched),
        )
        // --- "Ma liste" (user bookmarks, synced across web + TV) ---
        .route("/my-list", get(playback::list_my_list))
        .route(
            "/my-list/:id",
            put(playback::add_to_list).delete(playback::remove_from_list),
        )
        // --- live playback sessions (admin dashboard "En cours de lecture") ---
        .route("/playback/ping", post(playback::ping))
        .route("/playback/stop", post(playback::stop))
        // --- admin console ---
        .route("/admin/server", get(admin::server_info))
        .route("/admin/sessions", get(admin::sessions))
        .route("/admin/sessions/:id/stop", post(admin::terminate_session))
        .route("/admin/metrics", get(admin::metrics))
        .route("/admin/llm", get(admin::get_llm).put(admin::save_llm))
        .route("/admin/llm/models", post(admin::llm_models))
        .route("/admin/llm/test", post(admin::test_llm))
        .route("/admin/storage", get(admin::storage))
        .route("/admin/cache/clear", post(admin::clear_cache))
        .route("/admin/cache/reset-metadata", post(admin::reset_metadata))
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
        .route("/admin/backup/export", get(admin::export_backup))
        .route(
            "/admin/backup/import",
            post(admin::import_backup).layer(DefaultBodyLimit::max(admin::MAX_BACKUP_BYTES)),
        )
        .route("/admin/stats/top-users", get(admin::top_users))
        .route("/admin/stats/history", get(admin::history))
        .route("/admin/stats/overview", get(admin::overview))
        // --- background jobs / scheduler ---
        .route("/admin/jobs", get(admin::list_jobs))
        .route("/admin/job-runs/:run_id/logs", get(admin::run_logs))
        .route(
            "/admin/jobs/:key",
            get(admin::job_detail).patch(admin::update_job),
        )
        .route("/admin/jobs/:key/run", post(admin::run_job))
        .route("/admin/jobs/:key/cancel", post(admin::cancel_job));

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
