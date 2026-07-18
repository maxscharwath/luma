//! Pipeline stage `markers`: detect intro/credits segments, one **season** at a
//! time (chromaprint aligns a season's episodes pairwise, so the season is the
//! natural unit). Wraps [`crate::services::markers::job::detect_season`]; the
//! ledger makes it incremental (a season whose episode files are unchanged is
//! skipped) and per-season failures visible, replacing the old whole-library
//! re-fingerprint that took hours every run.

use anyhow::{anyhow, Result};

use crate::services::jobs::{JobContext, JobKey, Trigger};
use crate::state::SharedState;

use super::common::stage;

// One season at a time; `detect_season` parallelizes the episode decode internally
// and yields to playback there, so the dispatcher does not. Nightly, and chained
// after `subtitles` (the tail of the storyboard -> subtitles -> markers heavy-stage
// chain, so they run one at a time rather than all firing on the same library
// change). Also manual.
stage! {
    short: "markers",
    subject_kind: "season",
    concurrency: 1,
    pause_for_playback: false,
    schedule: Some("30 3 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("pipeline.subtitles"))],
}

/// One subject per season that has at least one probed episode. Subject id is
/// `"{show_id}#{season}"`; signature = detection mode + every episode file's
/// `mtime:size`, so a replaced episode or a mode change re-runs just that season.
/// When detection is off, nothing is in scope (existing tasks are then purged).
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let mode = state.settings.get_str("introDetection", "chapters");
    if mode == "off" {
        return Ok(Vec::new());
    }
    let shows = crate::db::list_shows(&state.db, None)?;
    let mut out = Vec::new();
    for show in &shows {
        let Some(detail) = crate::db::get_show(&state.db, &show.id)? else {
            continue;
        };
        for season in &detail.seasons {
            if let Some(sig) = season_signature(&mode, season) {
                out.push((format!("{}#{}", show.id, season.number), sig));
            }
        }
    }
    Ok(out)
}

/// The ledger signature for one season: detection mode + every playable episode
/// file's `mtime:size`. `None` when the season has no probed episodes yet (wait
/// for probe/scan). An unreadable episode (mount blip) collapses the whole season
/// to [`UNREADABLE_SIG`](crate::db::pipeline::UNREADABLE_SIG) so `reconcile` skips
/// it rather than re-fingerprinting on every flap.
fn season_signature(mode: &str, season: &crate::model::Season) -> Option<String> {
    let mut parts = vec![mode.to_string()];
    let mut playable = 0usize;
    let mut unreadable = false;
    for ep in &season.episodes {
        if let (Some(abs), Some(d)) = (ep.abs_path.as_deref(), ep.duration_ms) {
            if d > 0 {
                playable += 1;
                let sig = super::sig_for_path(abs);
                unreadable |= sig == crate::db::pipeline::UNREADABLE_SIG;
                parts.push(sig);
            }
        }
    }
    if playable == 0 {
        return None; // no probed episodes yet: wait for probe/scan
    }
    if unreadable {
        Some(crate::db::pipeline::UNREADABLE_SIG.to_string())
    } else {
        Some(crate::services::scan::short_hash(&parts.join("|")))
    }
}

fn process(ctx: &JobContext, subject_id: &str) -> Result<()> {
    let (show_id, season_num) = subject_id
        .rsplit_once('#')
        .ok_or_else(|| anyhow!("malformed season subject id {subject_id}"))?;
    let season_num: u32 = season_num
        .parse()
        .map_err(|_| anyhow!("malformed season number in {subject_id}"))?;
    let detail = crate::db::get_show(&ctx.state.db, show_id)?
        .ok_or_else(|| anyhow!("show {show_id} no longer exists"))?;
    let season = detail
        .seasons
        .iter()
        .find(|s| s.number == season_num)
        .ok_or_else(|| anyhow!("season {season_num} of {show_id} no longer exists"))?;
    crate::services::markers::job::detect_season(ctx, season)?;
    Ok(())
}
