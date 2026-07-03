//! Storyboard render orchestration: turn a duration into a sampling plan, drive
//! the tile extraction, montage the frames into one mosaic, and encode the sheet
//! (WebP preferred, JPEG fallback) alongside its manifest.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::model::MediaItem;

use super::extract::{extract_tiles, use_hwaccel};
use super::proc::{finalize, run_capturing, unique_tmp, Cancel, TMP_SEQ};
use super::{Manifest, Plan, Storyboard, TILE_H, TILE_W, playable};

/// WebP quality (0–100) for the sheet. Low is fine the tiles are tiny and only
/// previewed on hover, so this trades invisible quality for a much smaller file.
const WEBP_QUALITY: &str = "58";
/// Wall-clock ceiling for a local step (the montage / a JPEG encode). Generous a
/// stalled disk is killed, not hung on.
const TIMEOUT: Duration = Duration::from_secs(600);

/// Render the sheet for `(abs, dur_s)` and publish it + the manifest atomically
/// (sheet first, so a reader that sees the manifest always finds the sheet).
/// `Ok(())` on success; `Err(reason)` carries the first failing step's cause.
pub(super) fn generate(abs: &str, dir: &Path, key: &str, item_id: &str, dur_s: f64, cancel: Cancel) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("could not create the cache dir {}: {e}", dir.display()))?;
    let plan = Plan::for_duration(dur_s);
    render(abs, dir, key, &plan, use_hwaccel(abs, dur_s), cancel)?;
    let manifest = Manifest {
        url: format!("/api/items/{item_id}/storyboard.img?v={key}"),
        interval: f64::from(plan.interval),
        tile_w: TILE_W,
        tile_h: TILE_H,
        cols: plan.cols,
        rows: plan.rows,
        count: plan.count,
        duration: dur_s,
    };
    let body = serde_json::to_vec(&manifest)
        .map_err(|e| format!("could not serialize the manifest: {e}"))?;
    let json = dir.join(format!("{key}.json"));
    let jtmp = unique_tmp(&json);
    std::fs::write(&jtmp, &body).map_err(|e| format!("could not write the manifest: {e}"))?;
    if finalize(&jtmp, &json) {
        Ok(())
    } else {
        Err("could not publish the manifest (atomic rename failed)".to_string())
    }
}

/// Extract every tile via a parallel keyframe seek, montage them into one mosaic,
/// then encode to WebP (preferred) or JPEG. `Ok(ext)` is the produced extension;
/// `Err(reason)` reports which step failed and why.
fn render(abs: &str, dir: &Path, key: &str, plan: &Plan, hwaccel: bool, cancel: Cancel) -> std::result::Result<&'static str, String> {
    // Per-run scratch dir for the individual tile frames; removed once the sheet
    // is built (so a crash mid-generation can't leave hundreds of stray PNGs).
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let scratch = dir.join(format!(".sb-{key}-{}-{seq}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("could not create the storyboard scratch dir: {e}"))?;
    let res = render_into(abs, dir, key, plan, &scratch, hwaccel, cancel);
    let _ = std::fs::remove_dir_all(&scratch);
    res
}

/// The body of [`render`], with `scratch` guaranteed to exist and be cleaned up
/// by the caller regardless of outcome.
fn render_into(
    abs: &str,
    dir: &Path,
    key: &str,
    plan: &Plan,
    scratch: &Path,
    hwaccel: bool,
    cancel: Cancel,
) -> std::result::Result<&'static str, String> {
    extract_tiles(abs, scratch, plan, hwaccel, cancel)?;
    // A cancel during tile extraction stops the workers early; bail before the
    // (now-pointless) montage/encode so we don't publish a half-sampled sheet.
    if cancel() {
        return Err("cancelled".to_string());
    }
    let mosaic = scratch.join("mosaic.png");
    montage(scratch, plan, &mosaic)?;

    // Prefer WebP (smallest; cwebp → ffmpeg libwebp), fall back to JPEG. Both are
    // local encodes of the mosaic no more video reads.
    let webp = unique_tmp(&dir.join(format!("{key}.webp")));
    if crate::infra::image::encode_webp_quality(&mosaic, &webp, WEBP_QUALITY) && webp.exists() {
        return if finalize(&webp, &dir.join(format!("{key}.webp"))) {
            Ok("webp")
        } else {
            Err("could not publish the WebP sheet (atomic rename failed)".to_string())
        };
    }
    let _ = std::fs::remove_file(&webp);
    let jpg = unique_tmp(&dir.join(format!("{key}.jpg")));
    match png_to_jpeg(&mosaic, &jpg) {
        Err(e) => Err(format!("no WebP encoder and the JPEG fallback failed: {e}")),
        Ok(()) if finalize(&jpg, &dir.join(format!("{key}.jpg"))) => Ok("jpg"),
        Ok(()) => Err("could not publish the JPEG sheet (atomic rename failed)".to_string()),
    }
}

/// Lay the extracted `px_<NNNN>.png` tiles into one `cols x rows` mosaic (local,
/// no video reads). `Err` carries ffmpeg's captured cause.
fn montage(scratch: &Path, plan: &Plan, out: &Path) -> std::result::Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    // `-threads 1`: assembling one mosaic frame from local PNGs is cheap, so cap
    // the decoder pool rather than let it grab every core (the ffmpeg gate bounds
    // how many run, this bounds each one's footprint).
    cmd.args(["-v", "error", "-nostdin", "-threads", "1", "-y", "-start_number", "0", "-i"])
        .arg(scratch.join("px_%04d.png"))
        .args(["-frames:v", "1", "-vf", &format!("tile={}x{}", plan.cols, plan.rows)])
        .arg(out);
    run_capturing(cmd, TIMEOUT).map_err(|e| format!("mosaic assembly failed: {e}"))?;
    if out.exists() {
        Ok(())
    } else {
        Err("mosaic assembly reported success but produced no sheet".to_string())
    }
}

/// Encode the (already-built) PNG mosaic to JPEG the universal fallback when no
/// WebP encoder is present. Cheap + local (no video re-read). `Err` carries
/// ffmpeg's captured stderr.
fn png_to_jpeg(png: &Path, out: &Path) -> std::result::Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    // `-threads 1`: a single-frame JPEG encode is trivial; keep it off every core.
    cmd.args(["-y", "-v", "error", "-threads", "1", "-i"])
        .arg(png)
        .args(["-c:v", "mjpeg", "-q:v", "12", "-huffman", "optimal"])
        .arg(out);
    run_capturing(cmd, TIMEOUT)
}

impl Storyboard {
    /// Synchronously ensure `item`'s storyboard exists (skipping already-cached
    /// ones), aborting the in-flight ffmpeg the moment `cancel` flips (a job/stage
    /// was cancelled) so the current pass stops at the next
    /// poll tick instead of running out the full timeout.
    pub fn generate_blocking_cancellable(
        &self,
        item: &MediaItem,
        cancel: Cancel,
    ) -> std::result::Result<(), String> {
        if self.is_cached(item) {
            return Ok(());
        }
        let Some((abs, dur_s)) = playable(item) else {
            return Err("no media file or unknown duration".to_string());
        };
        let key = Self::key(&abs);
        generate(&abs, &self.dir, &key, &item.id, dur_s, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end generation against a real clip. Ignored by default (needs
    /// ffmpeg + writes temp files); run with
    /// `cargo test --bin luma-server -- --ignored storyboard_generates`.
    #[test]
    #[ignore]
    fn storyboard_generates_a_consistent_sheet() {
        let tmp = std::env::temp_dir().join(format!("sb_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let clip = tmp.join("clip.mp4");
        let ok = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=24:duration=30"])
            .args(["-c:v", "libx264", "-g", "48", "-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "could not create the test clip (is ffmpeg installed?)");

        let key = "testkey";
        generate(clip.to_str().unwrap(), &tmp, key, "x", 30.0, &|| false).expect("generation failed");

        // A sheet (WebP or JPEG) + a parseable manifest landed.
        let plan = Plan::for_duration(30.0);
        let sheet = ["webp", "jpg"]
            .iter()
            .map(|e| tmp.join(format!("{key}.{e}")))
            .find(|p| p.exists())
            .expect("no sheet produced");
        let parsed: Manifest =
            serde_json::from_slice(&std::fs::read(tmp.join(format!("{key}.json"))).unwrap()).unwrap();
        assert_eq!(parsed.cols, plan.cols);
        assert_eq!(parsed.tile_w, TILE_W);

        // The real sheet pixels match the declared geometry exactly (so the
        // client's background-position math never drifts).
        let dims = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
            .arg(&sheet)
            .output()
            .unwrap();
        let dims = String::from_utf8_lossy(&dims.stdout);
        let expected = format!("{},{}", plan.cols * TILE_W, plan.rows * TILE_H);
        assert_eq!(dims.trim(), expected, "sheet dimensions != cols*tileW x rows*tileH");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
