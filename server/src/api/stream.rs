//! Media byte delivery: original-file range streaming, the from-zero HLS remux
//! (a continuous ffmpeg master + alternate audio renditions, served as it grows),
//! and on-demand WebVTT subtitle extraction. Responses are media bytes / HLS
//! playlists, not JSON.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::api::error::json_error;
use crate::api::util::query;
use crate::db;
use crate::infra::stream::stream_or_demo_error;
use crate::model::MediaItem;
use crate::state::SharedState;

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
    headers: HeaderMap,
) -> Result<Response, Response> {
    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    let abs_path = pick_file_path(&item, q.file.as_deref());
    Ok(stream_or_demo_error(abs_path.as_deref(), &headers).await)
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

/// `GET /api/items/:id/hls/:mode/:anchor/:audio/index.m3u8` (mode = `copy`|`aac`,
/// anchor = start seconds for input `-ss`, audio = audio-relative track index) →
/// a single media playlist for video + that ONE audio track, muxed. Each
/// (anchor, audio) is its OWN session with its OWN child URLs. Language switching
/// is a reload with a different `audio` (hls.js alternate-audio was unreliable).
/// Segments are served by [`hls_file`].
pub async fn hls_master(
    State(state): State<SharedState>,
    Path((id, mode, anchor, audio)): Path<(String, String, u64, u32)>,
) -> Response {
    let Some(aac) = parse_mode(&mode) else {
        return json_error(StatusCode::BAD_REQUEST, "bad mode");
    };
    let Some(item) = load_item(&state, id).await else {
        return json_error(StatusCode::NOT_FOUND, "item not found");
    };
    let Some(abs) = item.abs_path.clone() else {
        return json_error(StatusCode::NOT_FOUND, "no media file for item");
    };
    match state.hls.master(&item.id, &abs, audio, aac, anchor).await {
        // `X-Hls-Start` is the REAL start (keyframe at-or-before the requested
        // anchor) - the client reads it for `baseSec` so the clock/subtitles stay
        // aligned with the A/V (which `-noaccurate_seek` starts at that keyframe).
        Some((body, start)) => {
            let mut resp = playlist_response(body);
            if let Ok(v) = header::HeaderValue::from_str(&format!("{start:.3}")) {
                resp.headers_mut().insert("X-Hls-Start", v);
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
) -> Response {
    let Some(aac) = parse_mode(&mode) else {
        return json_error(StatusCode::BAD_REQUEST, "bad mode");
    };
    let immutable = !file.ends_with(".m3u8"); // segments/init are fixed per anchor; playlists grow
    match state.hls.file(&id, aac, anchor, audio, &file).await {
        Some((bytes, ct)) => Response::builder()
            .header(header::CONTENT_TYPE, ct)
            // Each anchor's URLs are unique, so a segment's bytes never change →
            // safe to cache immutably. Playlists grow (event) → no-store.
            .header(
                header::CACHE_CONTROL,
                if immutable { "public, max-age=31536000, immutable" } else { "no-store" },
            )
            .body(Body::from(bytes))
            .unwrap(),
        None => json_error(StatusCode::NOT_FOUND, "segment not found (session expired?)"),
    }
}

async fn load_item(state: &SharedState, id: String) -> Option<MediaItem> {
    query(&state.db, move |pool| db::get_item(&pool, &id)).await.ok().flatten()
}

/// `copy` → `false` (stream-copy), `aac` → `true` (transcode to stereo AAC).
fn parse_mode(mode: &str) -> Option<bool> {
    match mode {
        "copy" => Some(false),
        "aac" => Some(true),
        _ => None,
    }
}

fn playlist_response(body: String) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
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

    let abs = match query(&state.db, move |pool| db::get_item(&pool, &id)).await {
        Ok(Some(item)) => item.abs_path,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "item not found"),
        Err(resp) => return resp,
    };
    let Some(abs) = abs else {
        return json_error(StatusCode::NOT_FOUND, "no media file for item");
    };

    // Disk cache: extracting a text subtitle reads the WHOLE file (cues are
    // interleaved throughout), which is slow over a network mount - so do it ONCE
    // per (file, mtime, track) and serve the cached WebVTT instantly thereafter.
    let cache = subs_cache_path(&state.config.data_dir, &abs, index);
    if let Ok(bytes) = tokio::fs::read(&cache).await {
        return vtt_response(bytes);
    }
    match extract_webvtt(&abs, index).await {
        Some(bytes) => {
            if let Some(dir) = cache.parent() {
                let _ = tokio::fs::create_dir_all(dir).await;
                let _ = tokio::fs::write(&cache, &bytes).await;
            }
            vtt_response(bytes)
        }
        None => json_error(StatusCode::NOT_FOUND, "subtitle unavailable (image-based or missing)"),
    }
}

fn vtt_response(bytes: Vec<u8>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(bytes))
        .unwrap()
}

/// `<data>/subs/<hash>.vtt`, keyed by file path + mtime + track index so a
/// replaced file re-extracts and each track caches independently.
fn subs_cache_path(data_dir: &std::path::Path, abs: &str, index: usize) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    index.hash(&mut h);
    let mtime = std::fs::metadata(abs)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    mtime.hash(&mut h);
    data_dir.join("subs").join(format!("{:016x}.vtt", h.finish()))
}

/// Max wall-clock for a single subtitle extraction. Extracting a text track reads
/// the WHOLE file (cues are interleaved), which on a multi-GB film over a network
/// mount - especially while HLS remuxes compete for it - can take a minute or two.
/// Generous so it completes (and caches); a truly stalled mount is still killed.
const SUBTITLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(150);

/// Run ffmpeg to transcode subtitle stream `index` to WebVTT (text subs only),
/// bounded by [`SUBTITLE_TIMEOUT`]. Uses `tokio::process` directly (no
/// `spawn_blocking`) with `-nostdin` + `Stdio::null()` stdin so ffmpeg can never
/// block waiting on the terminal; `kill_on_drop` reaps the child on timeout.
async fn extract_webvtt(path: &str, index: usize) -> Option<Vec<u8>> {
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
    let out = tokio::time::timeout(SUBTITLE_TIMEOUT, child.wait_with_output())
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
        assert_eq!(parse_mode("copy"), Some(false));
        assert_eq!(parse_mode("aac"), Some(true));
        assert_eq!(parse_mode("bogus"), None);
    }

}
