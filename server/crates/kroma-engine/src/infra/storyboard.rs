//! YouTube-style scrub-bar preview "storyboard": one sprite sheet (a mosaic of
//! evenly-spaced thumbnails) plus a tiny JSON manifest, generated once per file
//! and cached on disk. The player shows the tile under the cursor via CSS
//! `background-position` no per-frame work, a single image fetch.
//!
//! Generation is fast because it never reads the whole file: each tile is grabbed
//! by a FAST KEYFRAME SEEK (`-ss <t>` before `-i`, so ffmpeg jumps straight to the
//! GOP at that timestamp and reads only that), and the tiles are extracted MANY AT
//! A TIME across a small thread pool. The individual frames are then montaged into
//! one mosaic (`tile` filter) and encoded to **WebP** (via `cwebp`, like the rest
//! of KROMA's artwork the smallest format), falling back to JPEG where WebP is
//! unavailable. The source video is never re-encoded.
//!
//! Why not one linear `-skip_frame nokey` pass? That demuxes the entire file to
//! reach the last keyframe, so a multi-GB film costs minutes (it is bound by the
//! read of every byte). Seeking reads only the ~240 sampled GOPs, turning a
//! whole-file read into (tiles / workers) fast seeks.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::warn;

use crate::model::MediaItem;
use kroma_primitives::short_hash;

mod extract;
mod proc;
mod render;

/// Tile size (true 16:9, BOTH dimensions even so 4:2:0 chroma never coerces the
/// crop to an odd size which would drift the client's `background-position`).
/// Kept small on purpose a hover preview is tiny, so 160x90 + WebP makes the
/// sheet a few hundred KB at most.
const TILE_W: u32 = 160;
const TILE_H: u32 = 90;
/// Hard cap on grid cells, bounding the sheet size + ffmpeg output (a 3 h film
/// still fits in a single sheet).
const MAX_TILES: u32 = 240;
/// Never sample finer than this short clips don't need hundreds of near-identical
/// tiles.
const MIN_INTERVAL: u32 = 2;
/// Bump to invalidate every cached sheet when the generation parameters change.
const VERSION: u32 = 3;
/// Max concurrent storyboard ITEMS keeps generation off the HLS remux's back
/// (and the NAS quiet) when several items are opened at once. Each item also fans
/// its tiles out over its own worker pool ([`tile_workers`]).
const MAX_CONCURRENT: usize = 2;
/// After a failed generation, don't retry the same key for this long. The player
/// polls the manifest every few seconds, and without a cooldown a persistent
/// failure (offline mount, unreadable file) re-spawned ffmpeg on EVERY poll an
/// endless churn of doomed processes.
const FAIL_COOLDOWN: Duration = Duration::from_secs(120);

/// What the player needs to map a cursor time → a tile in the sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Sprite-sheet URL (`/api/items/:id/storyboard.img?v=<key>`); the `v` busts
    /// the immutable cache when the source file (its mtime) changes. The on-disk
    /// format (WebP or JPEG) is internal the route sets the content type.
    pub url: String,
    /// Seconds of video between consecutive tiles.
    pub interval: f64,
    pub tile_w: u32,
    pub tile_h: u32,
    pub cols: u32,
    pub rows: u32,
    /// Number of real tiles (trailing grid cells may be blank padding).
    pub count: u32,
    /// Total media duration (s).
    pub duration: f64,
}

/// Result of asking for an item's storyboard.
pub enum Status {
    Ready(Manifest),
    /// Generating (or just started) poll again shortly.
    Pending,
    /// No media file or unknown duration nothing to build.
    Unavailable,
}

/// The grid layout + sampling cadence derived from a runtime's duration.
pub(super) struct Plan {
    interval: u32,
    cols: u32,
    rows: u32,
    count: u32,
}

impl Plan {
    fn for_duration(dur_s: f64) -> Self {
        let interval = ((dur_s / f64::from(MAX_TILES)).ceil() as u32).max(MIN_INTERVAL);
        // `fps=1/interval` over the duration emits floor(dur/interval)+1 frames.
        let count = ((dur_s as u32 / interval) + 1).clamp(1, MAX_TILES);
        let cols = (f64::from(count).sqrt().ceil() as u32).max(1);
        let rows = count.div_ceil(cols).max(1);
        Self { interval, cols, rows, count }
    }
}

/// Sprite-sheet engine: a cache dir + in-flight dedup + a concurrency gate +
/// a recent-failure cooldown.
pub struct Storyboard {
    dir: PathBuf,
    inflight: Arc<Mutex<HashSet<String>>>,
    failed: Arc<Mutex<HashMap<String, Instant>>>,
    sem: Arc<Semaphore>,
}

impl Storyboard {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("storyboards"),
            inflight: Arc::new(Mutex::new(HashSet::new())),
            failed: Arc::new(Mutex::new(HashMap::new())),
            sem: Arc::new(Semaphore::new(MAX_CONCURRENT)),
        }
    }

    /// Per-(file, mtime) cache key bumping the source remaps the key, so a
    /// replaced file regenerates instead of serving a stale sheet.
    fn key(abs: &str) -> String {
        let mtime = std::fs::metadata(abs)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        short_hash(&format!("{abs}:{mtime}:v{VERSION}"))
    }

    /// Cached sheet bytes + content type for `item`, trying WebP then JPEG; `None`
    /// if it has not been generated yet.
    pub async fn sheet(&self, item: &MediaItem) -> Option<(Vec<u8>, &'static str)> {
        let abs = item.abs_path.clone()?;
        // `key` derivation does a blocking `fs::metadata`; run it off the async
        // worker so a stalled mount can't wedge the runtime.
        let key = tokio::task::spawn_blocking(move || Self::key(&abs)).await.ok()?;
        for (ext, ct) in [("webp", "image/webp"), ("jpg", "image/jpeg")] {
            if let Ok(bytes) = tokio::fs::read(self.dir.join(format!("{key}.{ext}"))).await {
                return Some((bytes, ct));
            }
        }
        None
    }

    /// Delete `item`'s cached sheet + manifest, so a reprocess regenerates it
    /// (the cache key is content-addressed, so this is the only way to force a
    /// rebuild without a source change).
    pub fn invalidate(&self, item: &MediaItem) {
        let Some(abs) = item.abs_path.as_deref() else {
            return;
        };
        let key = Self::key(abs);
        for ext in ["webp", "jpg", "json"] {
            let _ = std::fs::remove_file(self.dir.join(format!("{key}.{ext}")));
        }
    }

    /// Whether `item`'s storyboard is already on disk (cheap existence check, used
    /// by the pre-generation job to skip done work).
    pub fn is_cached(&self, item: &MediaItem) -> bool {
        let Some(abs) = item.abs_path.as_deref() else {
            return false;
        };
        self.dir.join(format!("{}.json", Self::key(abs))).exists()
    }

    /// The manifest if cached; otherwise kick off generation (deduped +
    /// concurrency-bounded) and report `Pending` for the caller to poll.
    pub async fn get(&self, item: &MediaItem) -> Status {
        let Some((abs, dur_s)) = playable(item) else {
            return Status::Unavailable;
        };
        // `key` derivation does a blocking `fs::metadata`; run it off the async
        // worker so a stalled mount can't wedge the runtime. The same stat also
        // answers "does the source even exist" an offline mount must not spawn
        // a doomed ffmpeg (let alone one per poll).
        let abs_key = abs.clone();
        let Ok((key, exists)) =
            tokio::task::spawn_blocking(move || (Self::key(&abs_key), Path::new(&abs_key).exists())).await
        else {
            return Status::Unavailable;
        };
        if let Ok(bytes) = tokio::fs::read(self.dir.join(format!("{key}.json"))).await {
            if let Ok(m) = serde_json::from_slice::<Manifest>(&bytes) {
                return Status::Ready(m);
            }
        }
        if !exists {
            return Status::Unavailable;
        }
        // Recently failed (unreadable/corrupt file, dying mount): don't retry on
        // every poll; the cooldown expiring or a source change (new key) retries.
        if let Some(at) = self.failed.lock().unwrap().get(&key) {
            if at.elapsed() < FAIL_COOLDOWN {
                return Status::Unavailable;
            }
        }

        // Not cached: claim the key (concurrent pollers then get Pending) + spawn.
        if !self.inflight.lock().unwrap().insert(key.clone()) {
            return Status::Pending; // already generating
        }
        let dir = self.dir.clone();
        let inflight = self.inflight.clone();
        let failed = self.failed.clone();
        let sem = self.sem.clone();
        let item_id = item.id.clone();
        let log_id = item.id.clone();
        tokio::spawn(async move {
            let _permit = sem.acquire_owned().await; // bound concurrent ffmpeg
            let key2 = key.clone();
            let res = tokio::task::spawn_blocking(move || render::generate(&abs, &dir, &key2, &item_id, dur_s, &|| false))
                .await
                .unwrap_or_else(|e| Err(format!("generation task crashed: {e}")));
            match res {
                Err(reason) => {
                    warn!(item = %log_id, "storyboard generation failed: {reason}");
                    failed.lock().unwrap().insert(key.clone(), Instant::now());
                }
                Ok(()) => {
                    failed.lock().unwrap().remove(&key);
                }
            }
            inflight.lock().unwrap().remove(&key);
        });
        Status::Pending
    }
}

/// `(abs_path, duration_s)` for a playable item, or `None` when either is missing.
fn playable(item: &MediaItem) -> Option<(String, f64)> {
    let abs = item.abs_path.clone()?;
    let ms = item.duration_ms.filter(|&d| d > 0)?;
    Some((abs, ms as f64 / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_bounds_tiles_and_grid() {
        let p = Plan::for_duration(7200.0);
        assert!(p.interval >= MIN_INTERVAL);
        assert!(p.count <= MAX_TILES);
        assert!(p.cols * p.rows >= p.count);

        let s = Plan::for_duration(60.0);
        assert_eq!(s.interval, MIN_INTERVAL);
        assert!(s.count < MAX_TILES);
        assert!(s.cols * s.rows >= s.count);
    }
}
