//! Pipeline stage `storyboard`: pre-generate the scrub-bar sprite sheet for each
//! playable item, so the first hover is instant. Wraps the existing
//! [`crate::infra::storyboard`] engine (its `generate_blocking_cancellable` already skips a
//! cached sheet); the ledger adds resumability, priority for fresh items, and
//! per-item failure visibility.

use anyhow::{anyhow, Result};

use crate::model::Category;
use crate::services::jobs::{Builtin, JobContext, JobKey, Trigger};
use crate::services::pipeline::stage::Stage;
use crate::state::SharedState;

pub const STAGE: Stage = Stage {
    short: "storyboard",
    key: "pipeline.storyboard",
    subject_kind: "item",
    // Two concurrent ffmpeg passes, matching the engine's own gate; the dispatcher
    // pauses between items while anyone is streaming.
    concurrency: 2,
    pause_for_playback: true,
    enumerate,
    process,
};

/// Drain `Builtin`: nightly, on a library change, and manually. Runs the shared
/// dispatcher over [`STAGE`].
pub const SPEC: Builtin = Builtin {
    key: JobKey("pipeline.storyboard"),
    category: Category::Pipeline,
    schedule: Some("0 2 * * *"),
    triggers: &[Trigger::LibraryChange],
    run,
};

fn run(ctx: &JobContext) -> Result<()> {
    crate::services::pipeline::dispatcher::run(&STAGE, ctx)
}

/// Every playable item (has a backing file + a known duration), signed by that
/// file's `mtime:size` so a replaced file regenerates. Cached items are still
/// enumerated so the ledger shows them `done`; `process` no-ops on a cache hit.
/// Unprobed items (duration unknown) are simply skipped until a later probe/scan
/// makes them eligible that is the probe dependency, encoded as a filter.
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let items = crate::db::list_items(&state.db, None)?;
    Ok(items
        .into_iter()
        .filter_map(|i| {
            let abs = i.abs_path.as_deref()?;
            if i.duration_ms.unwrap_or(0) == 0 {
                return None;
            }
            let sig = super::sig_for_path(abs);
            Some((i.id, sig))
        })
        .collect())
}

fn process(ctx: &JobContext, item_id: &str) -> Result<()> {
    let item = crate::db::get_item(&ctx.state.db, item_id)?
        .ok_or_else(|| anyhow!("item {item_id} no longer exists"))?;
    // `generate_blocking_cancellable` returns `Err(String)` with the real cause
    // (ffmpeg error, missing encoder, timeout) and is a no-op when already cached.
    // Thread the stage's cancellation so cancelling the job kills the in-flight
    // ffmpeg pass at the next poll tick instead of running out the full timeout.
    let cancel = || ctx.cancelled();
    ctx.state.storyboard.generate_blocking_cancellable(&item, &cancel).map_err(|e| anyhow!(e))
}
