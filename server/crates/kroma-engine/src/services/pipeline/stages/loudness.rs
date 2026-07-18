//! Pipeline stage `loudness`: EBU R128 measurement of each file's default audio
//! track (integrated loudness, loudness range, true peak, and the centre
//! channel alone on 5.1+), persisted with a derived verdict
//! (`ok` / `highDynamics` / `quietDialog`). Wraps [`crate::infra::loudness`].
//!
//! Audio-only decode, but it still READS the whole container so the cost is
//! dominated by I/O on big remuxes: one file at a time, paused during playback,
//! and only probed files are in scope (the track layout must be known).

use anyhow::Result;

use crate::services::jobs::{JobContext, JobKey, Trigger};
use crate::state::SharedState;

use super::common::stage;

// One decode at a time: the measurement is disk-read-bound, and fanning out would
// starve any concurrent stream from the same (often network) mount. Nightly (after
// the 1:00 probe pass has landed fresh files), and chained after `pipeline.probe`
// so a manual probe drain flows straight into analysis.
stage! {
    short: "loudness",
    subject_kind: "file",
    concurrency: 1,
    pause_for_playback: true,
    schedule: Some("0 4 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("pipeline.probe"))],
}

/// Every **probed** file, signed by `mtime:size` (a replaced file re-measures;
/// unprobed files enter scope once the probe stage lands).
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    crate::db::analyzable_file_sigs(&state.db)
}

fn process(ctx: &JobContext, file_id: &str) -> Result<()> {
    let Some((abs_path, tracks_json)) = crate::db::loudness_target(&ctx.state.db, file_id)? else {
        return Ok(()); // file row gone (or un-probed) since enumerate
    };
    if abs_path.starts_with("demo://") {
        return Ok(()); // demo/seed rows have no real bytes to decode
    }
    let tracks: Vec<kroma_domain::AudioStream> =
        serde_json::from_str(&tracks_json).unwrap_or_default();
    // Analyze the track playback picks by default; others aren't worth a full
    // decode each until something needs them.
    let Some(track) = tracks.iter().find(|t| t.default).or_else(|| tracks.first()) else {
        return Ok(()); // no audio at all: nothing to measure
    };
    let result =
        crate::infra::loudness::measure(std::path::Path::new(&abs_path), track.index, track.channels)?;
    let analysis = crate::infra::loudness::to_analysis(&result);
    crate::db::set_audio_analysis(&ctx.state.db, file_id, track.index, &analysis)?;
    ctx.info(format!(
        "loudness {}: I={:.1} LUFS, LRA={:.1} LU{} -> {:?}",
        abs_path,
        analysis.lufs_i,
        analysis.lra,
        analysis
            .dialog_lufs
            .map(|d| format!(", dialog={d:.1} LUFS"))
            .unwrap_or_default(),
        analysis.verdict,
    ));
    Ok(())
}
