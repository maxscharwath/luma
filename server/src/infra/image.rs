//! Local WebP cache for remote (TMDB) artwork.
//!
//! Each poster/backdrop is downloaded once and transcoded to WebP via `ffmpeg`
//! (already required for `ffprobe`), stored under `<data>/images/`. The catalog
//! then serves art from LUMA itself — smaller files, faster loads, and
//! resilient to TMDB outages. If caching fails (no ffmpeg / no network) the
//! original remote URL is kept, so nothing breaks.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::model::Metadata;
use crate::services::scan::short_hash;

/// Monotonic counter so two concurrent renditions of the same artwork get
/// distinct temp paths and never clobber each other's in-progress file.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Public route prefix served by `GET /api/images/:name`.
pub const PUBLIC_PREFIX: &str = "/api/images/";

/// WebP quality (0–100) and effort. 80/6 keeps posters crisp at a fraction of
/// the JPEG size.
const WEBP_QUALITY: &str = "80";
const WEBP_EFFORT: &str = "6";

/// Directory holding cached WebP artwork.
pub fn images_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("images")
}

/// Rewrite a [`Metadata`]'s poster/backdrop URLs to locally-cached WebP,
/// transcoding on the way. Returns the metadata unchanged for any image that
/// can't be cached.
pub fn localize(data_dir: &Path, mut meta: Metadata) -> Metadata {
    if let Some(url) = meta.poster_url.as_deref() {
        if let Some(local) = cache(data_dir, url) {
            meta.poster_url = Some(local);
        }
    }
    if let Some(url) = meta.backdrop_url.as_deref() {
        if let Some(local) = cache(data_dir, url) {
            meta.backdrop_url = Some(local);
        }
    }
    // Logo: keep as PNG (transparency must survive — WebP transcode is skipped).
    if let Some(url) = meta.logo_url.as_deref() {
        if let Some(local) = cache_verbatim(data_dir, url, "png") {
            meta.logo_url = Some(local);
        }
    }
    // Cast profile photos — same WebP cache, so the rail serves local art too.
    for member in &mut meta.cast {
        if let Some(url) = member.profile_url.as_deref() {
            if let Some(local) = cache(data_dir, url) {
                member.profile_url = Some(local);
            }
        }
    }
    meta
}

/// Download a remote image and cache it verbatim (no transcode) as
/// `<hash>.<ext>`, so transparency survives — used for title logos. Returns the
/// public `/api/images/<hash>.<ext>` path, or `None` on failure.
fn cache_verbatim(data_dir: &Path, remote_url: &str, ext: &str) -> Option<String> {
    if !remote_url.starts_with("http") {
        return Some(remote_url.to_string());
    }
    let dir = images_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("{}.{ext}", short_hash(remote_url));
    let out = dir.join(&name);
    if !out.exists() {
        // Download to a unique temp, then atomically rename onto the served path.
        let tmp = unique_tmp(&out);
        let dl = Command::new("curl")
            .args(["-sf", "-L", "--max-time", "25", "-o"])
            .arg(&tmp)
            .arg(remote_url)
            .status();
        if !matches!(dl, Ok(s) if s.success()) || !tmp.exists() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        finalize(&tmp, &out)?;
    }
    Some(format!("{PUBLIC_PREFIX}{name}"))
}

/// A title logo (alpha preserved) bounded to fit a card, for overlay. Scales the
/// cached PNG down to ≤300×120 and caches as `<name>.logo.png`. `name` is a bare
/// cached logo filename (`<hash>.png`).
pub fn card_logo_png(data_dir: &Path, name: &str) -> Option<PathBuf> {
    let dir = images_dir(data_dir);
    ffmpeg_rendition(
        &dir.join(name),
        &dir.join(format!("{name}.logo.png")),
        "scale=300:120:force_original_aspect_ratio=decrease",
        &[],
    )
}

/// 640×360 (16:9) cover-fit PNG of a cached WebP, used as the base layer for a
/// Smart Hub preview "card" (tiny-skia decodes PNG natively). ffmpeg scales to
/// fill then centre-crops. Cached as `<hash>.webp.card.png`. `webp_name` is a
/// bare, path-checked cache filename.
pub fn card_base_png(data_dir: &Path, webp_name: &str) -> Option<PathBuf> {
    let dir = images_dir(data_dir);
    ffmpeg_rendition(
        &dir.join(webp_name),
        &dir.join(format!("{webp_name}.card.png")),
        "scale=640:360:force_original_aspect_ratio=increase,crop=640:360",
        &[],
    )
}

/// JPEG rendition of a cached WebP, for Samsung TV Smart Hub preview tiles —
/// the carousel accepts only PNG/JPG (not WebP), max 360 KB, height ≤360 px.
/// Produced on demand from `<hash>.webp` → cached as `<hash>.webp.jpg`, scaled
/// to 360 px tall. `webp_name` is a bare cache filename (already path-checked by
/// the caller). Returns the JPEG path, or `None` if the source is missing or
/// transcoding fails.
pub fn jpeg_rendition(data_dir: &Path, webp_name: &str) -> Option<PathBuf> {
    let dir = images_dir(data_dir);
    ffmpeg_rendition(
        &dir.join(webp_name),
        &dir.join(format!("{webp_name}.jpg")),
        "scale=-2:360:flags=lanczos",
        &["-q:v", "3"],
    )
}

/// Single-frame ffmpeg rendition of `src` → `out` (cached: returns `out`
/// immediately if it already exists). `vf` is the `-vf` filtergraph; `extra`
/// carries any additional output args (e.g. JPEG quality).
///
/// The frame is written to a unique temp sibling and **atomically renamed** into
/// place, so a concurrent reader checking `out.exists()` never observes a
/// half-written file (which would be served 200 + immutable cache-control). The
/// temp keeps `out`'s extension so ffmpeg still infers the right muxer.
fn ffmpeg_rendition(src: &Path, out: &Path, vf: &str, extra: &[&str]) -> Option<PathBuf> {
    if !src.exists() {
        return None;
    }
    if out.exists() {
        return Some(out.to_path_buf());
    }
    let tmp = unique_tmp(out);
    let ok = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-vf", vf, "-frames:v", "1"])
        .args(extra)
        .arg(&tmp)
        .status();
    if !matches!(ok, Ok(s) if s.success()) || !tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    finalize(&tmp, out)
}

/// A unique sibling temp path for `out` that preserves `out`'s extension (so
/// ffmpeg/cwebp still detect the output format) and can't collide with a
/// concurrent writer.
fn unique_tmp(out: &Path) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let base = out.file_name().and_then(|n| n.to_str()).unwrap_or("rendition");
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    out.with_file_name(format!("{base}.{}.{seq}.tmp.{ext}", std::process::id()))
}

/// Atomically move a freshly-written temp file onto its final served path. On
/// failure the temp is cleaned up. Returns `out` on success.
fn finalize(tmp: &Path, out: &Path) -> Option<PathBuf> {
    match std::fs::rename(tmp, out) {
        Ok(()) => Some(out.to_path_buf()),
        Err(_) => {
            let _ = std::fs::remove_file(tmp);
            None
        }
    }
}

/// Ensure `remote_url` is cached as WebP and return its public path
/// (`/api/images/<hash>.webp`). Returns `None` on failure (caller keeps the
/// original URL).
fn cache(data_dir: &Path, remote_url: &str) -> Option<String> {
    // Already a local path (idempotent if called twice).
    if !remote_url.starts_with("http") {
        return Some(remote_url.to_string());
    }
    let dir = images_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;

    let name = format!("{}.webp", short_hash(remote_url));
    let out = dir.join(&name);
    if !out.exists() && !transcode(remote_url, &out) {
        return None;
    }
    Some(format!("{PUBLIC_PREFIX}{name}"))
}

/// Store an uploaded image (raw bytes, any common format) as a
/// content-addressed WebP under the image cache, returning its public path
/// (`/api/images/<hash>.webp`). Reuses the same WebP encoder as TMDB art.
/// Returns `None` if the bytes can't be decoded/encoded (no `cwebp`/ffmpeg or
/// not an image). Served by the existing `GET /api/images/:name`.
pub fn store_upload(data_dir: &Path, bytes: &[u8]) -> Option<String> {
    let dir = images_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;

    let name = format!("{}.webp", content_hash(bytes));
    let out = dir.join(&name);
    if !out.exists() {
        // Transcode goes through files (cwebp/ffmpeg read from disk): write the
        // raw bytes to a unique temp, encode to another unique temp, then
        // atomically rename onto the served path.
        let src_tmp = unique_tmp(&out);
        if std::fs::write(&src_tmp, bytes).is_err() {
            let _ = std::fs::remove_file(&src_tmp);
            return None;
        }
        let out_tmp = unique_tmp(&out);
        let ok = encode_webp(&src_tmp, &out_tmp) && out_tmp.exists();
        let _ = std::fs::remove_file(&src_tmp);
        if !ok {
            let _ = std::fs::remove_file(&out_tmp); // drop any partial output
            return None;
        }
        finalize(&out_tmp, &out)?;
    }
    Some(format!("{PUBLIC_PREFIX}{name}"))
}

/// `hex(sha256(bytes))[..16]` — content address for an uploaded image, so
/// identical uploads dedupe and the immutable cache header stays correct.
fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())[..16].to_string()
}

/// Download `remote_url` and transcode it to a WebP at `out`.
fn transcode(remote_url: &str, out: &Path) -> bool {
    // Download to a unique sibling temp first (curl handles HTTPS/redirects),
    // encode to another unique temp, then atomically rename onto `out` so a
    // concurrent reader never observes a partial WebP.
    let src_tmp = unique_tmp(out);
    let dl = Command::new("curl")
        .args(["-sf", "-L", "--max-time", "25", "-o"])
        .arg(&src_tmp)
        .arg(remote_url)
        .status();
    if !matches!(dl, Ok(s) if s.success()) {
        let _ = std::fs::remove_file(&src_tmp);
        return false;
    }

    let out_tmp = unique_tmp(out);
    let ok = encode_webp(&src_tmp, &out_tmp) && out_tmp.exists();
    let _ = std::fs::remove_file(&src_tmp);
    if !ok {
        let _ = std::fs::remove_file(&out_tmp); // drop any partial output
        return false;
    }
    finalize(&out_tmp, out).is_some()
}

/// Encode `src` → WebP at `out`. Prefers `cwebp` (canonical, present on most
/// systems incl. the Docker image); falls back to ffmpeg's libwebp encoder.
fn encode_webp(src: &Path, out: &Path) -> bool {
    let cwebp = Command::new("cwebp")
        .args(["-quiet", "-q", WEBP_QUALITY, "-m", WEBP_EFFORT])
        .arg(src)
        .arg("-o")
        .arg(out)
        .status();
    if matches!(cwebp, Ok(s) if s.success()) {
        return true;
    }

    let ffmpeg = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(src)
        .args([
            "-frames:v",
            "1",
            "-c:v",
            "libwebp",
            "-quality",
            WEBP_QUALITY,
            "-compression_level",
            WEBP_EFFORT,
        ])
        .arg(out)
        .status();
    matches!(ffmpeg, Ok(s) if s.success())
}
