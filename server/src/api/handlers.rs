//! REST API route handlers. All responses are JSON unless noted (poster = SVG,
//! stream = media bytes). DB work runs on `spawn_blocking` threads.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tracing::{error, info};

use crate::api::error::json_error;
use crate::api::poster::render_poster;
use crate::db;
use crate::events::ServerEvent;
use crate::metadata::{self, Target};
use crate::model::Kind;
use crate::state::SharedState;
use crate::stream::stream_or_demo_error;

/// Run a blocking DB closure off the async runtime, mapping failures to a 500.
pub(crate) async fn blocking<T, F>(f: F) -> Result<T, Response>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            error!(error = %e, "database error");
            Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
        }
        Err(e) => {
            error!(error = %e, "task join error");
            Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
        }
    }
}

/// Clone the connection pool and run a blocking DB closure off the async runtime.
/// A thin combinator over [`blocking`] that hands the closure its own `Pool`, so
/// handlers don't repeat `let pool = state.db.clone();` before every query.
pub(crate) async fn query<T, F>(pool: &db::Pool, f: F) -> Result<T, Response>
where
    F: FnOnce(db::Pool) -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    blocking(move || f(pool)).await
}

#[derive(Debug, Deserialize)]
pub struct LibraryQuery {
    pub library: Option<String>,
}

/// `GET /api/health`
pub async fn health(State(state): State<SharedState>) -> Result<Response, Response> {
    let (libraries, items, shows) = query(&state.db, move |pool| db::counts(&pool)).await?;
    Ok(Json(super::dto::Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        ffprobe: state.ffprobe_available,
        libraries,
        items,
        shows,
    })
    .into_response())
}

/// `GET /api/libraries` → `Library[]`
pub async fn list_libraries(State(state): State<SharedState>) -> Result<Response, Response> {
    let libs = query(&state.db, move |pool| db::list_libraries(&pool)).await?;
    Ok(Json(libs).into_response())
}

/// `GET /api/items` (optional `?library=`) → all playable items (movies + episodes).
pub async fn list_items(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let items = query(&state.db, move |pool| db::list_items(&pool, q.library.as_deref())).await?;
    Ok(Json(items).into_response())
}

/// `GET /api/movies` (optional `?library=`) → `MediaItem[]` (movies only).
pub async fn list_movies(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let items = query(&state.db, move |pool| db::list_movies(&pool, q.library.as_deref())).await?;
    Ok(Json(items).into_response())
}

/// `GET /api/shows` (optional `?library=`) → `Show[]`
pub async fn list_shows(
    State(state): State<SharedState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Response, Response> {
    let shows = query(&state.db, move |pool| db::list_shows(&pool, q.library.as_deref())).await?;
    Ok(Json(shows).into_response())
}

/// `GET /api/shows/:id` → `{ show, seasons[] }`
pub async fn get_show(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let detail = query(&state.db, move |pool| db::get_show(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "show not found"))?;
    Ok(Json(detail).into_response())
}

/// `GET /api/shows/:id/poster` → inline SVG placeholder.
pub async fn show_poster(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id2 = id.clone();
    let title = query(&state.db, move |pool| db::show_title(&pool, &id2))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "show not found"))?;
    Ok(render_poster(&id, &title))
}

/// `GET /api/items/:id` → `MediaItem`
pub async fn get_item(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    Ok(Json(item).into_response())
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

    // Enforce the concurrent-transcode cap (Transcodage → Flux simultanés max).
    // Reusing an existing session for this key never counts against it.
    if !state.transcode.has(&key).await {
        let cap = crate::settings::max_transcodes(&state.settings);
        if state.transcode.active_count().await >= cap {
            return Err(json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "trop de transcodages simultanés",
            ));
        }
    }

    let bytes = if let Some(spec) = master_spec(&variant) {
        // One stream carrying every audio track as an alternate rendition, so the
        // client switches language in place (no reload). `aac` re-encodes every
        // rendition to stereo AAC for runtimes that can't decode the source audio
        // (e.g. AC3/EAC3 on Chrome); else stream-copy (surround kept). `start_secs`
        // (-ss) starts the remux at the requested position so resume/seek to any
        // offset is available immediately, instead of waiting for a from-0 remux.
        let (aac, start_secs) = spec;
        let mut tracks: Vec<crate::transcode::MasterTrack> = item
            .audio_tracks
            .iter()
            .map(|a| crate::transcode::MasterTrack {
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

/// `GET /api/items/:id/poster` → inline SVG placeholder.
pub async fn item_poster(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id2 = id.clone();
    let item = query(&state.db, move |pool| db::get_item(&pool, &id2))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;
    Ok(render_poster(&id, &item.title))
}

/// `GET /api/images/:name` → locally-cached WebP artwork (poster/backdrop).
/// Immutable, content-addressed filenames → cache forever.
pub async fn image(State(state): State<SharedState>, Path(name): Path<String>) -> Response {
    // Reject anything but a simple cache filename (no path traversal).
    let safe = !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !safe {
        return json_error(StatusCode::BAD_REQUEST, "invalid image name");
    }

    // JPEG rendition for Samsung TV preview tiles (the carousel rejects WebP).
    // `<hash>.webp.jpg` → transcode the cached `<hash>.webp` on demand.
    if let Some(webp) = name.strip_suffix(".jpg").filter(|s| s.ends_with(".webp")) {
        let data_dir = state.config.data_dir.clone();
        let webp = webp.to_string();
        let made = blocking(move || Ok(crate::image::jpeg_rendition(&data_dir, &webp))).await;
        return match made {
            Ok(Some(jpg)) => match tokio::fs::read(&jpg).await {
                Ok(bytes) => image_response(bytes, "image/jpeg"),
                Err(_) => json_error(StatusCode::NOT_FOUND, "image not found"),
            },
            _ => json_error(StatusCode::NOT_FOUND, "image not found"),
        };
    }

    let path = crate::image::images_dir(&state.config.data_dir).join(&name);
    match tokio::fs::read(&path).await {
        Ok(bytes) => image_response(bytes, content_type_for(&name)),
        Err(_) => json_error(StatusCode::NOT_FOUND, "image not found"),
    }
}

/// Cached-artwork content type by extension (WebP posters/backdrops, PNG logos).
fn content_type_for(name: &str) -> &'static str {
    if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/webp"
    }
}

/// Content-addressed artwork response: serve `bytes` as `content_type`, cached
/// forever (filenames are immutable hashes).
fn image_response(bytes: Vec<u8>, content_type: &str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(bytes))
        .unwrap()
}

#[derive(Debug, Deserialize)]
pub struct CardQuery {
    /// Category label baked onto the card, e.g. "Ajout récent".
    pub label: Option<String>,
    /// Resume fraction 0.0–1.0 → draws a progress bar.
    pub progress: Option<f32>,
}

/// `GET /api/items/:id/card?label=&progress=` → a 640×360 landscape JPEG "card"
/// (backdrop + category badge + LUMA brand logo + title-treatment logo) for
/// Samsung TV Smart Hub preview tiles.
pub async fn item_card(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(q): Query<CardQuery>,
) -> Result<Response, Response> {
    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;

    // Prefer the 16:9 backdrop; fall back to the poster. Both must be locally
    // cached (a `/api/images/<hash>.webp` path) to composite.
    let meta = item.metadata.as_ref();
    let webp = meta
        .and_then(|m| m.backdrop_url.as_deref())
        .and_then(cache_name)
        .or_else(|| meta.and_then(|m| m.poster_url.as_deref()).and_then(cache_name))
        .map(str::to_string);
    let Some(webp) = webp else {
        return Err(json_error(StatusCode::NOT_FOUND, "no artwork for card"));
    };

    // Optional title-treatment logo (cached PNG → bounded overlay PNG).
    let logo = meta
        .and_then(|m| m.logo_url.as_deref())
        .and_then(cache_name)
        .map(str::to_string);

    let label = q.label.unwrap_or_default();
    let progress = q.progress;
    let data_dir = state.config.data_dir.clone();

    let rendered = blocking(move || {
        let Some(base_path) = crate::image::card_base_png(&data_dir, &webp) else {
            return Ok(None);
        };
        let base = std::fs::read(&base_path)?;
        let logo_bytes = logo
            .and_then(|name| crate::image::card_logo_png(&data_dir, &name))
            .and_then(|path| std::fs::read(&path).ok());
        Ok(crate::api::card::render(&crate::api::card::Card {
            base_png: &base,
            label: &label,
            logo_png: logo_bytes.as_deref(),
            progress,
        }))
    })
    .await?;

    let resp = match rendered {
        Some(jpg) => image_response(jpg, "image/jpeg"),
        None => json_error(StatusCode::NOT_FOUND, "artwork unavailable"),
    };
    Ok(resp)
}

/// Bare cache filename from a `/api/images/<name>` URL, or `None` if remote.
fn cache_name(url: &str) -> Option<&str> {
    url.strip_prefix(crate::image::PUBLIC_PREFIX)
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

/// `GET /api/items/:id/metadata` → TMDB details + IDs for one item.
///
/// Movies resolve against TMDB movies; episodes resolve against the parent show
/// (TV). Results are cached. Returns 503 if `LUMA_TMDB_API_KEY` is unset, 404 if
/// the item is unknown or TMDB has no match.
pub async fn item_metadata(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let api_key = require_tmdb_key(&state)?;

    let item = query(&state.db, move |pool| db::get_item(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "item not found"))?;

    // Episodes inherit their show's identity for the lookup. `item` is owned and
    // unused afterwards, so move its strings out rather than cloning.
    let year = item.year;
    let (target, title) = if item.kind == Kind::Episode {
        (Target::Tv, item.show_title.unwrap_or(item.title))
    } else {
        (Target::Movie, item.title)
    };

    resolve_metadata(state, api_key, target, title, year).await
}

/// `GET /api/shows/:id/metadata` → TMDB details + IDs for one show.
pub async fn show_metadata(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let api_key = require_tmdb_key(&state)?;

    let show = query(&state.db, move |pool| db::get_show(&pool, &id))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "show not found"))?
        .show;

    resolve_metadata(state, api_key, Target::Tv, show.title, show.year).await
}

/// The configured TMDB key, or a ready 503 telling the operator to set it. Shared
/// by the two metadata handlers.
fn require_tmdb_key(state: &SharedState) -> Result<String, Response> {
    state.config.tmdb_api_key.clone().ok_or_else(|| {
        json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "metadata disabled: set LUMA_TMDB_API_KEY",
        )
    })
}

/// Shared tail for the two metadata handlers: run the (blocking) TMDB lookup off
/// the async runtime and shape the JSON / 404 response.
async fn resolve_metadata(
    state: SharedState,
    api_key: String,
    target: Target,
    title: String,
    year: Option<u32>,
) -> Result<Response, Response> {
    let language = state.config.tmdb_language.clone();
    let result = blocking(move || {
        Ok(metadata::lookup(
            &state.metadata_cache,
            &api_key,
            &language,
            target,
            &title,
            year,
        ))
    })
    .await?;

    let resp = match result {
        Some(meta) => Json(meta).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "no metadata match"),
    };
    Ok(resp)
}

/// `POST /api/scan` → rescan all dirs, reseeding demo content if empty.
pub async fn rescan(State(state): State<SharedState>) -> Result<Response, Response> {
    let defs = crate::settings::library_defs(&state.settings, &state.config);

    state.events.publish(ServerEvent::ScanStarted);
    crate::activity::scan_started(&state.activity);

    // Phase 1 (fast): walk + stat only, diff-synced (preserves metadata + probed
    // data via the mtime cache). Phase 2 probing is spawned afterwards.
    let data = query(&state.db, move |pool| {
        let mut data = crate::scan::scan_all(&defs);
        if data.items.is_empty() {
            info!("scan yielded no items; seeding demo content");
            data = crate::demo::demo_data();
        }
        db::sync_all(&pool, &data.libraries, &data.shows, &data.items, &data.mtimes)?;
        Ok(data)
    })
    .await?;

    let (libraries, shows, items) = (data.libraries.len(), data.shows.len(), data.items.len());
    crate::activity::scan_completed(&state.activity, libraries, shows, items, crate::scan::now_iso8601());
    // Tell live clients the catalog changed, then run phase-2 probing and
    // re-resolve TMDB art in the background (both emit live updates).
    state.events.publish(ServerEvent::ScanCompleted { items, shows, libraries });
    state.events.publish(ServerEvent::LibraryUpdated);
    crate::probe::spawn_probe_pass(
        state.db.clone(),
        state.ffprobe_available,
        state.events.clone(),
        state.activity.clone(),
    );
    crate::enrich::maybe_spawn(&state, &data.items, &data.shows);
    Ok(Json(super::dto::ScanResult { scanned: items, libraries, shows }).into_response())
}

/// `GET /api/status` → live scan/enrichment snapshot.
pub async fn status(State(state): State<SharedState>) -> Response {
    let snap = crate::activity::snapshot(&state.activity);
    Json(snap).into_response()
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Number of trailing lines to return (default 200, max 5000).
    pub tail: Option<usize>,
}

/// `GET /api/logs?tail=N` → the last N lines of the current server log, as
/// `text/plain`. Reads the most-recently-modified file under `<data>/logs/`.
pub async fn logs(
    State(state): State<SharedState>,
    Query(q): Query<LogsQuery>,
) -> Result<Response, Response> {
    let dir = state.config.logs_dir();
    let tail = q.tail.unwrap_or(200).min(5000);

    let text = blocking(move || Ok(read_log_tail(&dir, tail))).await?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

/// Read the last `tail` lines of the newest log file in `dir` (empty if none).
fn read_log_tail(dir: &std::path::Path, tail: usize) -> String {
    let newest = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((mtime, p))
        })
        .max_by_key(|(mtime, _)| *mtime)
        .map(|(_, p)| p);

    let Some(path) = newest else {
        return String::new();
    };
    let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        // Keep the always-200, text/plain contract (an empty body for the
        // legitimately-empty-log case) but don't swallow a real read failure.
        tracing::warn!(path = %path.display(), error = %e, "failed to read log file for /api/logs");
        String::new()
    });
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(tail);
    lines[start..].join("\n")
}
