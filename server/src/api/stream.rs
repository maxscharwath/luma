//! Media byte delivery: original-file range streaming, the from-zero HLS remux
//! (a continuous ffmpeg master + alternate audio renditions, served as it grows),
//! and on-demand WebVTT subtitle extraction. Responses are media bytes / HLS
//! playlists, not JSON.

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::api::error::json_error;
use crate::api::util::{client_ip, query};
use crate::db;
use crate::infra::hls::StreamMode;
use crate::infra::stream::stream_or_demo_error;
use crate::infra::subtitles;
use crate::model::MediaItem;
use crate::services::playback;
use crate::services::settings;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// The byte sink for a media request, targeting the LAN or WAN bandwidth counter
/// by the client's network class (same classification as playback sessions).
fn byte_sink(state: &SharedState, headers: &HeaderMap, addr: &SocketAddr) -> crate::infra::metrics::ByteSink {
    let ip = client_ip(headers, addr);
    let is_lan = playback::is_lan(&ip, &settings::local_networks(&state.settings));
    state.metrics.sink(is_lan)
}

/// Direct-play streaming, HLS remux, storyboard previews and subtitle tracks.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/items/{id}/stream", get(stream_item))
        .route("/items/{id}/hls/{mode}/{anchor}/{audio}/index.m3u8", get(hls_master))
        .route("/items/{id}/hls/{mode}/{anchor}/{audio}/{file}", get(hls_file))
        .route("/items/{id}/storyboard", get(storyboard))
        .route("/items/{id}/storyboard.img", get(storyboard_image))
        .route("/items/{id}/subtitles/{track}", get(subtitles))
}

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    /// Optional specific file id to stream. Defaults to the item's default file.
    pub file: Option<String>,
}

/// `GET /api/items/:id/stream` (optional `?file=<fileId>`) → range-streamed
/// original file. Without `?file`, the item's default/best file is served.
pub async fn stream_item(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(q): Query<StreamQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    let abs_path = pick_file_path(&item, q.file.as_deref());
    let sink = byte_sink(&state, &headers, &addr);
    Ok(stream_or_demo_error(abs_path.as_deref(), &headers, sink).await)
}

/// Resolve which physical file to stream: an explicit `?file=<id>` when it
/// belongs to the item, else the item's default/representative file.
fn pick_file_path(item: &MediaItem, file_id: Option<&str>) -> Option<String> {
    if let Some(fid) = file_id {
        if let Some(f) = item.files.iter().find(|f| f.id == fid) {
            return f.abs_path.clone();
        }
    }
    item.abs_path.clone()
}

// ----- HLS: one muxed program per (mode, anchor, audio) -----------------------

/// `GET /api/items/:id/hls/:mode/:anchor/:audio/index.m3u8` (mode = `copy`|`aac`|
/// `aac-standard`|`aac-night`, anchor = start seconds for input `-ss`, audio =
/// audio-relative track index) → a single media playlist for video + that ONE
/// audio track, muxed. The `aac-*` filter modes apply a loudness compressor
/// (night-mode volume leveling) during the transcode, for clients with no local
/// audio DSP (Tizen AVPlay). Each (mode, anchor, audio) is its OWN session with
/// its OWN child URLs. Language switching is a reload with a different `audio`
/// (hls.js alternate-audio was unreliable). Segments are served by [`hls_file`].
pub async fn hls_master(
    State(state): State<SharedState>,
    Path((id, mode, anchor, audio)): Path<(String, String, u64, u32)>,
) -> Response {
    let Some(mode) = StreamMode::parse(&mode) else {
        return json_error(StatusCode::BAD_REQUEST, "bad mode");
    };
    let Some(item) = load_item(&state, id).await else {
        return json_error(StatusCode::NOT_FOUND, "item not found");
    };
    let Some(abs) = item.abs_path.clone() else {
        return json_error(StatusCode::NOT_FOUND, "no media file for item");
    };
    // Offline mount / moved file: fail in one stat instead of spawning ffmpeg
    // and polling ~20s for a playlist that will never appear (a hung 500).
    let abs_check = abs.clone();
    let exists = tokio::task::spawn_blocking(move || std::path::Path::new(&abs_check).exists())
        .await
        .unwrap_or(false);
    if !exists {
        return json_error(StatusCode::NOT_FOUND, "media file unavailable (mount offline?)");
    }
    match state.hls.master(&item.id, &abs, audio, mode, anchor).await {
        // `X-Hls-Start` is the REAL start (keyframe at-or-before the requested
        // anchor) - the client reads it for `baseSec` so the clock/subtitles stay
        // aligned with the A/V (which `-noaccurate_seek` starts at that keyframe).
        Some((body, start)) => {
            let mut resp = playlist_response(body);
            if let Ok(v) = header::HeaderValue::from_str(&format!("{start:.3}")) {
                resp.headers_mut().insert("X-Hls-Start", v);
            }
            // `X-Media-Duration` is the TRUE total length (s): the DB duration when
            // the file was probed, else a cached on-demand ffprobe. The client uses
            // it when its catalog `durationMs` is missing so the slider spans the
            // whole movie instead of the growing EVENT playlist's live edge.
            let dur_ms = match item.duration_ms {
                Some(d) => Some(d),
                None => state.hls.input_duration_ms(&abs).await,
            };
            if let Some(secs) = dur_ms.map(|ms| ms as f64 / 1000.0).filter(|s| *s > 0.0) {
                if let Ok(v) = header::HeaderValue::from_str(&format!("{secs:.3}")) {
                    resp.headers_mut().insert("X-Media-Duration", v);
                }
            }
            resp
        }
        None => json_error(StatusCode::INTERNAL_SERVER_ERROR, "HLS remux unavailable (is ffmpeg installed?)"),
    }
}

/// `GET /api/items/:id/hls/:mode/:anchor/:audio/:file` → a child file (init or
/// media segment) of the `(mode, anchor, audio)` session. A not-yet-produced
/// segment is polled for until ffmpeg flushes it.
pub async fn hls_file(
    State(state): State<SharedState>,
    Path((id, mode, anchor, audio, file)): Path<(String, String, u64, u32, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let Some(mode) = StreamMode::parse(&mode) else {
        return json_error(StatusCode::BAD_REQUEST, "bad mode");
    };
    let immutable = !file.ends_with(".m3u8"); // segments/init are fixed per anchor; playlists grow
    match state.hls.file(&id, mode, anchor, audio, &file).await {
        Some((bytes, ct)) => {
            // Meter the segment/playlist bytes into the bandwidth chart. The
            // whole body is buffered, so count it up front (it delivers within a
            // sample or two); playlists are tiny.
            byte_sink(&state, &headers, &addr).add(bytes.len() as u64);
            Response::builder()
            .header(header::CONTENT_TYPE, ct)
            // Each anchor's URLs are unique, so a segment's bytes never change →
            // safe to cache immutably. Playlists grow (event) → no-store.
            .header(
                header::CACHE_CONTROL,
                if immutable { "public, max-age=31536000, immutable" } else { "no-store" },
            )
            .body(Body::from(bytes))
            .unwrap()
        }
        None => json_error(StatusCode::NOT_FOUND, "segment not found (session expired?)"),
    }
}

async fn load_item(state: &SharedState, id: String) -> Option<MediaItem> {
    query(&state.db, move |pool| db::get_item(&pool, &id)).await.ok().flatten()
}

fn playlist_response(body: String) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap()
}

// ----- Storyboard (scrub-bar hover thumbnails) --------------------------------

/// `GET /api/items/:id/storyboard` → the sprite-sheet manifest (JSON) the player
/// needs to map a cursor time → a tile. Returns 202 `{"status":"pending"}` while
/// the sheet is being generated (the client polls), or 404 when the item has no
/// file / unknown duration. The sheet itself is served by [`storyboard_image`].
pub async fn storyboard(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let Some(item) = load_item(&state, id).await else {
        return json_error(StatusCode::NOT_FOUND, "item not found");
    };
    use crate::infra::storyboard::Status;
    match state.storyboard.get(&item).await {
        Status::Ready(m) => json_no_store(StatusCode::OK, serde_json::to_vec(&m).unwrap_or_default()),
        Status::Pending => json_no_store(StatusCode::ACCEPTED, br#"{"status":"pending"}"#.to_vec()),
        Status::Unavailable => json_error(StatusCode::NOT_FOUND, "storyboard unavailable"),
    }
}

/// `GET /api/items/:id/storyboard.img` → the cached sprite sheet (WebP or JPEG;
/// the content type is set from whichever was produced). Immutable (the manifest's
/// `?v=<key>` cache-busts when the source file changes); 404 until generated.
pub async fn storyboard_image(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let Some(item) = load_item(&state, id).await else {
        return json_error(StatusCode::NOT_FOUND, "item not found");
    };
    match state.storyboard.sheet(&item).await {
        Some((bytes, content_type)) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(Body::from(bytes))
            .unwrap(),
        None => json_error(StatusCode::NOT_FOUND, "storyboard not generated"),
    }
}

/// A `no-store` JSON response with an explicit status (manifest / pending marker).
fn json_no_store(status: StatusCode, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap()
}

// ----- Subtitles --------------------------------------------------------------

/// `GET /api/items/:id/subtitles/:track` → extract an embedded **text** subtitle
/// stream to WebVTT via ffmpeg, for the custom client renderer. `track` is the
/// 0-based subtitle index (a trailing `.vtt` is allowed). Image subtitles
/// (PGS/VobSub) can't convert and return 404.
pub async fn subtitles(
    State(state): State<SharedState>,
    Path((id, track)): Path<(String, String)>,
) -> Response {
    let index: usize = match track.trim_end_matches(".vtt").parse() {
        Ok(n) => n,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid subtitle index"),
    };

    let item = match query(&state.db, move |pool| db::get_item(&pool, &id)).await {
        Ok(Some(item)) => item,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "item not found"),
        Err(resp) => return resp,
    };
    let Some(abs) = item.abs_path.clone() else {
        return json_error(StatusCode::NOT_FOUND, "no media file for item");
    };

    // Disk cache: extracting a text subtitle reads the WHOLE file (cues are
    // interleaved throughout), which is slow over a network mount - so do it ONCE
    // per (file, mtime, track) and serve the cached WebVTT instantly thereafter.
    // Normally the pipeline `subtitles` stage has already warmed this; this endpoint
    // is the fallback for a track it has not reached yet.
    // Computing the cache key stats the file (mtime); on a slow mount that sync call
    // would block the tokio worker, so do it on the blocking pool.
    let data_dir = state.config.data_dir.clone();
    let cache = {
        let (abs, data_dir) = (abs.clone(), data_dir.clone());
        match tokio::task::spawn_blocking(move || subtitles::cache_path(&data_dir, &abs, index)).await {
            Ok(p) => p,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "subtitle cache error"),
        }
    };
    if let Ok(bytes) = tokio::fs::read(&cache).await {
        return vtt_response(bytes);
    }
    // Cache miss: demux the file ONCE and warm EVERY text track (so a later
    // language switch is instant too), then serve the one that was requested.
    // The per-file lock joins any extraction already in flight (the playback
    // pre-warm, another client, a retry) instead of demuxing in parallel.
    let subs = item.subtitles.clone();
    let (abs2, data_dir2) = (abs.clone(), data_dir.clone());
    let _ = tokio::task::spawn_blocking(move || {
        subtitles::extract_pending_locked(&data_dir2, &abs2, &subs, &|| false)
    })
    .await;
    if let Ok(bytes) = tokio::fs::read(&cache).await {
        return vtt_response(bytes);
    }
    // Fallback: `item.subtitles` metadata can be empty/stale, so the batch pass may
    // not have covered THIS index. Extract just the requested track codec-agnostically
    // (the old behavior), cache it, and serve it; only 404 if that yields nothing too.
    if let Some(bytes) = extract_webvtt(&abs, index).await {
        if let Some(dir) = cache.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        let _ = tokio::fs::write(&cache, &bytes).await;
        return vtt_response(bytes);
    }
    json_error(StatusCode::NOT_FOUND, "subtitle unavailable (image-based or missing)")
}

fn vtt_response(bytes: Vec<u8>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(bytes))
        .unwrap()
}

/// Run ffmpeg to transcode subtitle stream `index` to WebVTT (text subs only),
/// bounded by [`subtitles::TIMEOUT`]. Uses `tokio::process` directly (no
/// `spawn_blocking`) with `-nostdin` + `Stdio::null()` stdin so ffmpeg can never
/// block waiting on the terminal; `kill_on_drop` reaps the child on timeout. This
/// single-track variant backs subtitle *translation* (the source track for the LLM
/// pass); playback extraction goes through [`subtitles::extract_batch_blocking`].
pub(crate) async fn extract_webvtt(path: &str, index: usize) -> Option<Vec<u8>> {
    let child = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", &format!("0:s:{index}"), "-f", "webvtt", "pipe:1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    // On timeout the future is dropped, which (via kill_on_drop) kills ffmpeg.
    // The budget scales with the file size (a whole-file read), like the batch path.
    let out = tokio::time::timeout(subtitles::timeout_for(path), child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_variants() {
        assert_eq!(StreamMode::parse("copy"), Some(StreamMode::Copy));
        assert_eq!(StreamMode::parse("aac"), Some(StreamMode::Aac));
        assert_eq!(StreamMode::parse("aac-standard"), Some(StreamMode::AacStandard));
        assert_eq!(StreamMode::parse("aac-night"), Some(StreamMode::AacNight));
        assert_eq!(StreamMode::parse("bogus"), None);
    }

}
