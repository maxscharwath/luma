//! EBU R128 loudness measurement via ffmpeg's `loudnorm` filter (measurement
//! pass: decode + analyze, never transcodes). One pass over the file measures
//! the full mix; for 5.1+ tracks the same decode also measures the centre
//! channel alone (where dialogue lives) through an `asplit` fork, so the file
//! is read once. The measured values feed both the stored verdict and any
//! future playback-side `loudnorm` two-pass remediation.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use kroma_domain::{AudioAnalysis, AudioVerdict};

/// LRA above this (LU) = the "quiet dialogue, loud explosions" mix.
const LRA_HIGH: f64 = 15.0;
/// Centre channel this far below the full mix (LU) = dialogue is buried.
const DIALOG_GAP: f64 = 8.0;

/// One `loudnorm` measurement (the filter's `input_*` fields).
#[derive(Debug, Clone, Copy)]
pub struct LoudnessStats {
    /// Integrated loudness (LUFS).
    pub input_i: f64,
    /// Loudness range (LU).
    pub input_lra: f64,
    /// True peak (dBTP).
    pub input_tp: f64,
}

/// Full-mix measurement plus, for 5.1+ tracks, the centre channel alone.
#[derive(Debug, Clone, Copy)]
pub struct LoudnessResult {
    pub mix: LoudnessStats,
    pub dialog: Option<LoudnessStats>,
}

/// Loudnorm's shared filter parameters. Only the measurement (`input_*`) side
/// is read; the targets just have to be valid.
const LOUDNORM: &str = "loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json";

/// Measure the loudness of audio track `audio_rel` (audio-relative index) of
/// `path`. `channels` decides whether a centre-channel measurement is worth
/// forking off (5.1 and up). Audio-only decode: minutes-per-movie work, gated
/// on the process-wide ffmpeg budget.
pub fn measure(path: &Path, audio_rel: u32, channels: Option<u32>) -> Result<LoudnessResult> {
    let with_center = channels.is_some_and(|c| c >= 5);
    let mut cmd = Command::new("ffmpeg");
    // `loudnorm` prints its measurement JSON on stderr at the `info` log level.
    cmd.args(["-hide_banner", "-v", "info", "-nostdin", "-threads", "1"]);
    cmd.arg("-i").arg(path);
    if with_center {
        // One decode, two measurements: full mix + centre channel. The graph is
        // built mix-first, so the mix loudnorm always has the LOWER filter
        // index the stderr blocks are matched by index, not print order
        // (ffmpeg flushes them in nondeterministic order).
        cmd.arg("-filter_complex").arg(format!(
            "[0:a:{audio_rel}]asplit=2[full][c];[full]{LOUDNORM}[fo];[c]pan=mono|c0=FC,{LOUDNORM}[co]",
        ));
        cmd.args(["-map", "[fo]", "-f", "null", "-", "-map", "[co]", "-f", "null", "-"]);
    } else {
        cmd.arg("-map").arg(format!("0:a:{audio_rel}"));
        cmd.args(["-af", LOUDNORM, "-f", "null", "-"]);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());

    let out = {
        let _permit = crate::infra::ffmpeg_gate::acquire();
        cmd.output().context("spawn ffmpeg for loudness measurement")?
    };
    if !out.status.success() {
        bail!("ffmpeg loudness pass exited with {}", out.status);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut blocks = parse_loudnorm_blocks(&stderr);
    if blocks.is_empty() {
        bail!("no loudnorm measurement in ffmpeg output for {}", path.display());
    }
    blocks.sort_by_key(|(idx, _)| *idx);
    let mix = blocks[0].1;
    let dialog = if with_center { blocks.get(1).map(|(_, s)| *s) } else { None };
    Ok(LoudnessResult { mix, dialog })
}

/// Extract every `[Parsed_loudnorm_N @ …] { … }` JSON block from ffmpeg's
/// stderr as `(filter_index, stats)`. Blocks print in nondeterministic order;
/// the caller sorts by index.
fn parse_loudnorm_blocks(stderr: &str) -> Vec<(u32, LoudnessStats)> {
    let mut out = Vec::new();
    for (pos, _) in stderr.match_indices("Parsed_loudnorm_") {
        let tail = &stderr[pos + "Parsed_loudnorm_".len()..];
        let idx: u32 = match tail.chars().take_while(char::is_ascii_digit).collect::<String>().parse()
        {
            Ok(i) => i,
            Err(_) => continue,
        };
        // The loudnorm JSON is flat: the first `{ … }` after the tag is it.
        let Some(open) = tail.find('{') else { continue };
        let Some(close) = tail[open..].find('}') else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&tail[open..open + close + 1]) else {
            continue;
        };
        let field = |name: &str, floor: f64| -> f64 {
            let x = v
                .get(name)
                .and_then(|f| f.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(floor);
            // Silence measures as -inf; clamp so the value stays storable and
            // JSON-serializable.
            if x.is_finite() { x } else { floor }
        };
        out.push((
            idx,
            LoudnessStats {
                input_i: field("input_i", -70.0),
                input_lra: field("input_lra", 0.0),
                input_tp: field("input_tp", -99.0),
            },
        ));
    }
    out
}

/// Derive the stored analysis (verdict included) from a measurement.
pub fn to_analysis(res: &LoudnessResult) -> AudioAnalysis {
    let dialog_lufs = res.dialog.map(|d| d.input_i);
    AudioAnalysis {
        lufs_i: res.mix.input_i,
        lra: res.mix.input_lra,
        true_peak: res.mix.input_tp,
        dialog_lufs,
        verdict: verdict(res.mix.input_i, res.mix.input_lra, dialog_lufs),
    }
}

/// Pure verdict policy. Buried dialogue outranks plain wide dynamics: it is the
/// more actionable finding (the boost suggestion names dialogue explicitly).
fn verdict(mix_i: f64, lra: f64, dialog_i: Option<f64>) -> AudioVerdict {
    if let Some(d) = dialog_i {
        // A fully silent centre (-70 floor) means the "5.1" carries no real
        // dialogue channel; that's not a dialogue problem, skip the check.
        if d > -69.0 && mix_i - d > DIALOG_GAP {
            return AudioVerdict::QuietDialog;
        }
    }
    if lra > LRA_HIGH {
        return AudioVerdict::HighDynamics;
    }
    AudioVerdict::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    const STDERR: &str = r#"
size=N/A time=00:00:04.50 bitrate=N/A speed=8.91x
[Parsed_loudnorm_3 @ 0x875009440]
{
	"input_i" : "-31.65",
	"input_tp" : "-27.96",
	"input_lra" : "0.00",
	"input_thresh" : "-41.65",
	"normalization_type" : "dynamic",
	"target_offset" : "0.04"
}
[Parsed_loudnorm_1 @ 0x8750092c0]
{
	"input_i" : "-9.05",
	"input_tp" : "-10.46",
	"input_lra" : "0.00",
	"input_thresh" : "-19.05",
	"normalization_type" : "dynamic",
	"target_offset" : "0.04"
}
[out#0/null @ 0x875008780] video:0KiB audio:17988KiB
"#;

    #[test]
    fn parses_blocks_and_orders_by_filter_index() {
        let mut blocks = parse_loudnorm_blocks(STDERR);
        assert_eq!(blocks.len(), 2);
        blocks.sort_by_key(|(idx, _)| *idx);
        // Index 1 (the mix, declared first in the graph) sorts before index 3
        // (the centre) even though 3 printed first.
        assert_eq!(blocks[0].0, 1);
        assert!((blocks[0].1.input_i - -9.05).abs() < 1e-9);
        assert!((blocks[1].1.input_i - -31.65).abs() < 1e-9);
    }

    #[test]
    fn non_finite_measurements_clamp() {
        let stderr = "[Parsed_loudnorm_0 @ 0x0]\n{\"input_i\" : \"-inf\", \"input_tp\" : \"-inf\", \"input_lra\" : \"0.00\"}";
        let blocks = parse_loudnorm_blocks(stderr);
        assert_eq!(blocks.len(), 1);
        assert!((blocks[0].1.input_i - -70.0).abs() < 1e-9);
        assert!((blocks[0].1.input_tp - -99.0).abs() < 1e-9);
    }

    #[test]
    fn verdict_policy() {
        // Comfortable mix.
        assert_eq!(verdict(-23.0, 9.0, Some(-25.0)), AudioVerdict::Ok);
        // Wide dynamics, dialogue fine.
        assert_eq!(verdict(-23.0, 18.5, None), AudioVerdict::HighDynamics);
        // Buried dialogue wins over the LRA flag.
        assert_eq!(verdict(-9.0, 18.5, Some(-31.6)), AudioVerdict::QuietDialog);
        // Silent centre channel is not "quiet dialogue".
        assert_eq!(verdict(-23.0, 9.0, Some(-70.0)), AudioVerdict::Ok);
    }
}
