//! Local MP3 cache for TV theme songs (Plex-style).
//!
//! Plex plays a show's title theme under the detail page. The community
//! "tvthemes" archive serves these as `http://tvthemes.plexapp.com/<tvdb>.mp3`,
//! keyed by TheTVDB series id which TMDB hands us via `external_ids`. During
//! enrichment we download a show's theme once and store it under `<data>/themes/`,
//! then the catalog serves it from LUMA itself (`GET /api/themes/<tvdb>.mp3`).
//! Movies have no archive entry, so this is a no-op for them.
//!
//! Best-effort throughout: a missing theme (the archive 404s for most titles), no
//! network, or no `curl` simply leaves `theme_url` unset and nothing breaks.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::model::Metadata;

/// Public route prefix served by `GET /api/themes/:name`.
pub const PUBLIC_PREFIX: &str = "/api/themes/";

/// The community theme-song archive, keyed by TheTVDB series id.
const ARCHIVE: &str = "http://tvthemes.plexapp.com";

/// Reject anything smaller than this as an error body / empty file rather than a
/// real theme (a genuine MP3 is comfortably larger).
const MIN_BYTES: u64 = 8 * 1024;

/// Monotonic counter so concurrent downloads of the same theme get distinct temp
/// paths and never clobber each other's in-progress file.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Directory holding cached theme MP3s.
pub fn themes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("themes")
}

/// Resolve a show's theme song into a locally-cached MP3, rewriting
/// `meta.theme_url` to its public path. Returns the metadata unchanged when the
/// title isn't a show (no `tvdb_id`), already has a theme, or none could be
/// fetched.
pub fn localize(data_dir: &Path, mut meta: Metadata) -> Metadata {
    if meta.theme_url.is_some() {
        return meta; // idempotent: already cached on a prior pass
    }
    if let Some(tvdb_id) = meta.tvdb_id {
        if let Some(local) = cache(data_dir, tvdb_id) {
            meta.theme_url = Some(local);
        }
    }
    meta
}

/// Ensure the theme for `tvdb_id` is cached and return its public path
/// (`/api/themes/<tvdb>.mp3`), or `None` when the archive has none / download
/// fails.
fn cache(data_dir: &Path, tvdb_id: u64) -> Option<String> {
    let dir = themes_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;

    // `tvdb_id` is numeric, so the filename is path-safe by construction.
    let name = format!("{tvdb_id}.mp3");
    let out = dir.join(&name);
    if !out.exists() && !download(tvdb_id, &out) {
        return None;
    }
    Some(format!("{PUBLIC_PREFIX}{name}"))
}

/// Download the theme for `tvdb_id` to `out`. Writes to a unique temp first and
/// atomically renames so a concurrent reader never observes a partial file.
/// Rejects tiny bodies (archive error pages) so we don't cache a non-theme.
fn download(tvdb_id: u64, out: &Path) -> bool {
    let url = format!("{ARCHIVE}/{tvdb_id}.mp3");
    let tmp = unique_tmp(out);
    // `-f` fails on HTTP >= 400 (the archive 404s for unknown shows); bound the
    // download in both time and size so a bad URL can't stall or balloon the pass.
    let dl = Command::new("curl")
        .args(["-sf", "-L", "--max-time", "25", "--max-filesize", "30M", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status();
    let ok = matches!(dl, Ok(s) if s.success())
        && std::fs::metadata(&tmp).map(|m| m.len() >= MIN_BYTES).unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    match std::fs::rename(&tmp, out) {
        Ok(()) => true,
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            false
        }
    }
}

/// A unique `.mp3` sibling temp path for `out` that can't collide with a
/// concurrent writer.
fn unique_tmp(out: &Path) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let base = out.file_name().and_then(|n| n.to_str()).unwrap_or("theme.mp3");
    out.with_file_name(format!("{base}.{}.{seq}.tmp.mp3", std::process::id()))
}
