//! Theme-song endpoint: serves the locally-cached TV theme MP3s that the detail
//! page loops under the hero (see [`crate::infra::theme`]).

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;

use crate::api::error::json_error;
use crate::infra::metrics::ByteSink;
use crate::infra::stream::stream_file;
use crate::infra::theme::themes_dir;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// `GET /api/themes/:name`.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/themes/{name}", get(theme))
}

/// `GET /api/themes/:name` → a locally-cached theme MP3, with `Range` support so
/// `<audio>` can seek/loop. Content-addressed by TVDB id → cached for a week.
pub async fn theme(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    // Honour the feature flag at serve time too, so disabling it silences any
    // theme still referenced by already-enriched metadata (before a re-scan
    // clears it).
    if !crate::services::settings::theme_songs_enabled(&state.settings) {
        return json_error(StatusCode::NOT_FOUND, "theme songs are disabled");
    }

    // Reject anything but a simple `<digits>.mp3` cache filename (no traversal).
    let safe = name.ends_with(".mp3")
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !safe {
        return json_error(StatusCode::BAD_REQUEST, "invalid theme name");
    }

    let path = themes_dir(&state.config.data_dir).join(&name);
    // UI theme songs aren't media playback; keep them out of the bandwidth chart.
    let mut resp = stream_file(&path, &headers, ByteSink::none()).await;
    // Themes are effectively immutable for a given show; let clients cache them.
    if resp.status().is_success() {
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=604800"),
        );
    }
    resp
}
