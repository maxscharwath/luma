//! Audio-fingerprint intro/credits detection (Jellyfin "Intro Skipper" style).
//!
//! `ffmpeg` decodes a window of mono PCM, `rusty-chromaprint` turns it into a
//! Chromaprint fingerprint, and the crate's `match_fingerprints` aligns two
//! episodes to surface the run they share — the intro (near the start of the file)
//! or the recurring credits theme (near the end). No external `fpcalc` binary: we
//! already ship `ffmpeg` and fingerprint in-process.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use rusty_chromaprint::{match_fingerprints, Configuration, Fingerprinter};

/// Keep only well-aligned segments. `Segment::score` is 0 (identical) … 32 (worst).
const MAX_SCORE: f64 = 10.0;

/// Chromaprint config (preset_test1 ≈ standard fpcalc). Cheap to build.
pub fn config() -> Configuration {
    Configuration::preset_test1()
}

/// A fingerprint plus where its window began in the file (seconds), so a match's
/// in-window time can be mapped back to an absolute position.
pub struct WindowFp {
    pub data: Vec<u32>,
    pub window_start_s: f64,
}

/// Decode `secs` of mono PCM from `path` and fingerprint it. `from_end` takes the
/// last `secs` (credits) instead of the first (intro). `duration_s` anchors the
/// end window in absolute time.
pub fn fingerprint_window(
    path: &Path,
    secs: u32,
    from_end: bool,
    duration_s: f64,
) -> Result<WindowFp> {
    let cfg = config();
    let sr = cfg.sample_rate();
    let mut cmd = Command::new("ffmpeg");
    // Audio-only decode is single-stream work; cap the decoder pool so marker
    // jobs never fan out threads across every core.
    cmd.args(["-v", "error", "-nostdin", "-threads", "1"]);
    if from_end {
        cmd.arg("-sseof").arg(format!("-{secs}"));
    }
    cmd.arg("-i").arg(path);
    if !from_end {
        cmd.arg("-t").arg(secs.to_string());
    }
    cmd.args(["-vn", "-ac", "1", "-ar"])
        .arg(sr.to_string())
        .args(["-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    // Share the process-wide ffmpeg budget (see `infra::ffmpeg_gate`); the permit
    // is held only for the decode and dropped as this block ends.
    let out = {
        let _permit = crate::infra::ffmpeg_gate::acquire();
        cmd.output().context("spawn ffmpeg for fingerprint")?
    };
    if !out.status.success() {
        bail!("ffmpeg exited with {}", out.status);
    }
    let samples: Vec<i16> = out
        .stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    if samples.is_empty() {
        bail!("no audio decoded from {}", path.display());
    }

    let mut fp = Fingerprinter::new(&cfg);
    fp.start(sr, 1).map_err(|e| anyhow::anyhow!("fingerprinter start: {e:?}"))?;
    fp.consume(&samples);
    fp.finish();
    Ok(WindowFp {
        data: fp.fingerprint().to_vec(),
        window_start_s: if from_end { (duration_s - secs as f64).max(0.0) } else { 0.0 },
    })
}

/// The (start, end) seconds **within `a`'s window** of the longest well-aligned
/// segment shared by `a` and `b` whose start lies in `region` and that is at least
/// `min_len_s` long. `None` if nothing qualifies.
pub fn matched_range(
    a: &[u32],
    b: &[u32],
    region: (f32, f32),
    min_len_s: f32,
) -> Option<(f32, f32)> {
    let cfg = config();
    let segments = match_fingerprints(a, b, &cfg).ok()?;
    let ranges: Vec<(f32, f32)> = segments
        .iter()
        .filter(|s| s.score <= MAX_SCORE)
        .map(|s| (s.start1(&cfg), s.end1(&cfg)))
        .collect();
    pick_range(&ranges, region, min_len_s)
}

/// Pure: from candidate `(start, end)` ranges, keep those starting in `region` and
/// at least `min_len_s` long; return the longest. Separated out for testing.
pub fn pick_range(ranges: &[(f32, f32)], region: (f32, f32), min_len_s: f32) -> Option<(f32, f32)> {
    ranges
        .iter()
        .copied()
        .filter(|(s, e)| *s >= region.0 && *s <= region.1 && (e - s) >= min_len_s)
        .max_by(|x, y| (x.1 - x.0).partial_cmp(&(y.1 - y.0)).unwrap_or(std::cmp::Ordering::Equal))
}

/// Pure: consensus of per-pair ranges — the median range, accepted only if at
/// least `min_support` of the candidates start within 3 s of it. `None` otherwise.
/// Guards against one anomalous episode producing a spurious marker.
pub fn consensus(mut ranges: Vec<(f32, f32)>, min_support: usize) -> Option<(f32, f32)> {
    if ranges.len() < min_support.max(1) {
        return None;
    }
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (ms, me) = ranges[ranges.len() / 2];
    let support = ranges.iter().filter(|(s, _)| (s - ms).abs() <= 3.0).count();
    (support >= min_support.max(1)).then_some((ms, me))
}

/// Convert an in-window `secs` offset to an absolute position in ms.
pub fn abs_ms(window_start_s: f64, secs: f32) -> u64 {
    ((window_start_s + secs as f64) * 1000.0).max(0.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_range_filters_region_and_length_then_longest() {
        let ranges = [
            (0.5, 5.0),   // too short (4.5 s)
            (2.0, 20.0),  // 18 s, in region → candidate
            (2.5, 30.0),  // 27.5 s, in region → longest
            (400.0, 460.0), // outside region
        ];
        assert_eq!(pick_range(&ranges, (0.0, 60.0), 10.0), Some((2.5, 30.0)));
        // Tighter min length drops everything.
        assert_eq!(pick_range(&ranges, (0.0, 60.0), 40.0), None);
        // Region excludes the early starts.
        assert_eq!(pick_range(&ranges, (300.0, 500.0), 10.0), Some((400.0, 460.0)));
    }

    #[test]
    fn consensus_needs_support() {
        let agree = vec![(1.0, 90.0), (1.5, 91.0), (2.0, 89.0)];
        assert_eq!(consensus(agree, 2), Some((1.5, 91.0)));
        // Scattered starts → no consensus.
        assert_eq!(consensus(vec![(1.0, 90.0), (40.0, 130.0)], 2), None);
        // Too few candidates.
        assert_eq!(consensus(vec![(1.0, 90.0)], 2), None);
    }

    #[test]
    fn abs_ms_anchors_to_window() {
        assert_eq!(abs_ms(0.0, 12.0), 12_000); // intro window starts at 0
        assert_eq!(abs_ms(1200.0, 15.0), 1_215_000); // end window offset added
    }
}
