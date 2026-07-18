//! Pipeline stage `subtitles`: pre-extract every embedded TEXT subtitle track to
//! its WebVTT cache, ONE ffmpeg pass per file, so the first time a viewer turns
//! subtitles on they load instantly (a disk read) instead of waiting on a
//! whole-file demux mid-playback. Wraps [`crate::infra::subtitles`]; the ledger
//! adds resumability, priority for fresh items, and per-item failure visibility.
//!
//! Image subs (PGS/VobSub) are skipped - they are bitmap and cannot become text,
//! so an item with only image subs is never enumerated (nothing to do).

use anyhow::{anyhow, Result};

use crate::infra::subtitles;
use crate::services::jobs::{JobContext, JobKey, Trigger};
use crate::state::SharedState;

use super::common::stage;

// One ffmpeg pass per item (all tracks muxed out together); the dispatcher pauses
// between items while anyone is streaming, as this reads whole files. Nightly, and
// chained after `storyboard` (rather than firing on the same library change), so
// the CPU-heavy stages run one after another instead of all at once. Also manual.
stage! {
    short: "subtitles",
    subject_kind: "item",
    concurrency: 2,
    pause_for_playback: true,
    schedule: Some("0 3 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("pipeline.storyboard"))],
}

/// Every item with a backing file AND at least one text subtitle track, signed by
/// that file's `mtime:size` so a replaced file re-extracts. Items with no text subs
/// (none, or image-only) are not subjects. Fully-cached items still enumerate so
/// the ledger shows them `done`; `process` no-ops when nothing is pending.
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let items = crate::db::list_items(&state.db, None)?;
    Ok(items
        .into_iter()
        .filter_map(|i| {
            let abs = i.abs_path.as_deref()?;
            if !i.subtitles.iter().any(|s| subtitles::is_text_codec(&s.codec)) {
                return None;
            }
            Some((i.id, super::sig_for_path(abs)))
        })
        .collect())
}

fn process(ctx: &JobContext, item_id: &str) -> Result<()> {
    let item = crate::db::get_item(&ctx.state.db, item_id)?
        .ok_or_else(|| anyhow!("item {item_id} no longer exists"))?;
    let Some(abs) = item.abs_path.as_deref() else {
        return Ok(()); // no backing file: nothing to extract
    };
    // All text tracks already cached (or none): a clean no-op cache hit. The
    // per-file lock dedupes against the on-demand endpoint and the playback
    // pre-warm. Threading the stage's cancellation kills the in-flight ffmpeg
    // pass at the next poll tick instead of running out the full timeout.
    let cancel = || ctx.cancelled();
    subtitles::extract_pending_locked(&ctx.state.config.data_dir, abs, &item.subtitles, &cancel)
        .map_err(|e| anyhow!(e))
}
