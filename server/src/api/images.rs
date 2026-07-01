//! Artwork endpoints: inline SVG poster placeholders, locally-cached WebP/JPEG
//! artwork, and the composited landscape "card" JPEG for Samsung TV preview tiles.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::api::error::json_error;
use crate::api::poster::render_poster;
use crate::api::util::{blocking, query};
use crate::db;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// Poster / card rendering plus static image serving.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/shows/:id/poster", get(show_poster))
        .route("/items/:id/poster", get(item_poster))
        .route("/items/:id/card", get(item_card))
        .route("/images/:name", get(image))
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

/// Allowed `?w=` rendition widths. A fixed bucket set keeps the on-disk cache
/// bounded (each source image can gain at most this many variants) and makes
/// every rendition shareable between clients that ask for similar sizes.
const IMAGE_WIDTHS: [u32; 4] = [160, 320, 480, 780];

#[derive(Debug, Deserialize)]
pub struct ImageQuery {
    /// Requested display width (px); snapped up to the nearest bucket.
    pub w: Option<u32>,
}

/// `GET /api/images/:name?w=` → locally-cached WebP artwork (poster/backdrop),
/// optionally downscaled to a bucketed width (`?w=`, see [`IMAGE_WIDTHS`]).
/// Immutable, content-addressed filenames → cache forever.
pub async fn image(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Query(q): Query<ImageQuery>,
) -> Response {
    // Reject anything but a simple cache filename (no path traversal).
    let safe = !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !safe {
        return json_error(StatusCode::BAD_REQUEST, "invalid image name");
    }

    // Sized rendition: produced once (cwebp/ffmpeg, on the blocking pool), then
    // served from disk forever. Falls through to the original on any failure.
    if let Some(w) = q.w.filter(|_| name.ends_with(".webp")) {
        let width = IMAGE_WIDTHS.iter().copied().find(|b| *b >= w).unwrap_or(0);
        if width > 0 {
            let data_dir = state.config.data_dir.clone();
            let sized_name = name.clone();
            let made =
                blocking(move || Ok(crate::infra::image::sized_rendition(&data_dir, &sized_name, width)))
                    .await;
            if let Ok(Some((path, content_type))) = made {
                if let Ok(bytes) = tokio::fs::read(&path).await {
                    return image_response(bytes, content_type);
                }
            }
        }
    }

    // JPEG rendition for Samsung TV preview tiles (the carousel rejects WebP).
    // `<hash>.webp.jpg` → transcode the cached `<hash>.webp` on demand.
    if let Some(webp) = name.strip_suffix(".jpg").filter(|s| s.ends_with(".webp")) {
        let data_dir = state.config.data_dir.clone();
        let webp = webp.to_string();
        let made = blocking(move || Ok(crate::infra::image::jpeg_rendition(&data_dir, &webp))).await;
        return match made {
            Ok(Some(jpg)) => match tokio::fs::read(&jpg).await {
                Ok(bytes) => image_response(bytes, "image/jpeg"),
                Err(_) => json_error(StatusCode::NOT_FOUND, "image not found"),
            },
            _ => json_error(StatusCode::NOT_FOUND, "image not found"),
        };
    }

    let path = crate::infra::image::images_dir(&state.config.data_dir).join(&name);
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
        let Some(base_path) = crate::infra::image::card_base_png(&data_dir, &webp) else {
            return Ok(None);
        };
        let base = std::fs::read(&base_path)?;
        let logo_bytes = logo
            .and_then(|name| crate::infra::image::card_logo_png(&data_dir, &name))
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
    url.strip_prefix(crate::infra::image::PUBLIC_PREFIX)
}
