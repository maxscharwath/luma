//! Media byte delivery: original-file range streaming, live per-track HLS remux,
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
fn pick_file_path(item: &crate::model::MediaItem, file_id: Option<&str>) -> Option<String> {
    if let Some(fid) = file_id {
        if let Some(f) = item.files.iter().find(|f| f.id == fid) {
            return f.abs_path.clone();
        }
    }
    item.abs_path.clone()
}

/// Parse an HLS `variant` path segment of the form `a<idx>c<0|1>` into the
/// audio-relative track index and the stream-copy flag. `a0c0` is the legacy
/// default (first track, re-encoded to AAC).
fn parse_variant(variant: &str) -> Option<(u32, bool)> {
    let rest = variant.strip_prefix('a')?;
    let (idx, copy) = rest.split_once('c')?;
    let idx: u32 = idx.parse().ok()?;
    let copy = match copy {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    Some((idx, copy))
}

/// Parse an HLS *master* `variant`: `master`/`masteraac` (offset 0) or the
/// positioned form `master.<copy|aac>.<startMs>`. Returns `(transcode_to_aac,
/// start_seconds)`, or `None` when it's not a master variant (a per-track
/// `a<idx>c<copy>` instead). The start is baked into the path (not a query) so
/// the relative segment/rendition URLs the player derives keep the same session.
fn master_spec(variant: &str) -> Option<(bool, f64)> {
    let spec = variant.strip_prefix("master")?;
    if spec.is_empty() {
        return Some((false, 0.0));
    }
    if spec == "aac" {
        return Some((true, 0.0));
    }
    let rest = spec.strip_prefix('.')?; // "<copy|aac>.<startMs>"
    let mut it = rest.splitn(2, '.');
    let aac = it.next() == Some("aac");
    let start_secs = it
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(0.0, |ms| ms as f64 / 1000.0);
    Some((aac, start_secs))
}

/// `GET /api/items/:id/hls/:variant/index.m3u8` → HLS playlist for a per-track
/// remux. The video stream is always copied untouched; the audio track named by
/// `variant` (`a<idx>c<copy>`) is either stream-copied (`c1`, preserves surround)
/// or re-encoded to stereo AAC (`c0`, for runtimes that can't decode the source
/// codec). Powers both the audio-track picker and the AC3/EAC3/DTS fallback.
/// Direct-play stays the default for the first track everywhere it can decode.
pub async fn hls_playlist(
    State(state): State<SharedState>,
    Path((id, variant)): Path<(String, String)>,
) -> Result<Response, Response> {
    let id2 = id.clone();
    let item = query(&state.db, move |pool| db::get_item(&pool, &id2))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    let abs = item
        .abs_path
        .clone()
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "no media file for item"))?;
    let path = std::path::Path::new(&abs);
    let key = format!("{id}:{variant}");

    // Bound concurrent transcodes (Transcodage → Flux simultanés max) by evicting
    // the most-idle session rather than rejecting playback: a client's own seeks
    // spawn a fresh per-position session each time, so a hard 503 would block the
    // very stream the user is trying to watch. Reusing this key never evicts.
    if !state.transcode.has(&key).await {
        let cap = crate::services::settings::max_transcodes(&state.settings);
        state.transcode.make_room(cap, &key).await;
    }

    let bytes = if let Some(spec) = master_spec(&variant) {
        // One stream carrying every audio track as an alternate rendition, so the
        // client switches language in place (no reload). `aac` re-encodes every
        // rendition to stereo AAC for runtimes that can't decode the source audio
        // (e.g. AC3/EAC3 on Chrome); else stream-copy (surround kept). `start_secs`
        // (-ss) starts the remux at the requested position so resume/seek to any
        // offset is available immediately, instead of waiting for a from-0 remux.
        let (aac, start_secs) = spec;
        let mut tracks: Vec<crate::infra::transcode::MasterTrack> = item
            .audio_tracks
            .iter()
            .map(|a| crate::infra::transcode::MasterTrack {
                index: a.index,
                language: a.language.clone(),
                default: false,
            })
            .collect();
        if tracks.is_empty() {
            return Err(json_error(StatusCode::BAD_REQUEST, "item has no audio tracks for a master playlist"));
        }
        // Exactly one default rendition: the container default, else the first.
        let def = item.audio_tracks.iter().position(|a| a.default).unwrap_or(0);
        tracks[def].default = true;
        state.transcode.master(&key, path, &tracks, aac, start_secs).await
    } else {
        let (audio_idx, copy) = parse_variant(&variant)
            .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "bad hls variant"))?;
        state.transcode.playlist(&key, path, audio_idx, copy).await
    };

    let resp = match bytes {
        Some(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap(),
        None => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "audio remux unavailable (is ffmpeg installed?)",
        ),
    };
    Ok(resp)
}

/// `GET /api/items/:id/hls/:variant/:file` → an init fragment or media segment
/// for a live per-track remux session. A refreshed playlist (`index.m3u8`) is
/// also served here as the `event` playlist grows.
pub async fn hls_segment(
    State(state): State<SharedState>,
    Path((id, variant, file)): Path<(String, String, String)>,
) -> Response {
    let key = format!("{id}:{variant}");
    match state.transcode.file(&key, &file).await {
        Some((bytes, content_type)) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap(),
        None => json_error(StatusCode::NOT_FOUND, "segment not found (session expired?)"),
    }
}

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

    match extract_webvtt(&abs, index).await {
        Some(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .unwrap(),
        None => json_error(StatusCode::NOT_FOUND, "subtitle unavailable (image-based or missing)"),
    }
}

/// Max wall-clock for a single subtitle extraction. A stalled ffmpeg (slow or
/// disconnected mount, a pathological stream) is killed rather than pinning a
/// worker forever.
const SUBTITLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
