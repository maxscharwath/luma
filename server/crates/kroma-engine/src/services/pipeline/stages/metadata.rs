//! Pipeline stage `metadata`: resolve TMDB metadata (poster/backdrop/overview/
//! cast/IDs) per movie and show. Wraps [`crate::services::enrich::enrich_one`]
//! (idempotent: enriched titles are skipped, shows still run their incremental
//! per-season pass, TMDB misses are recorded `done` so they stop being retried
//! every run the one thing the detached scan-time enrich can't do).

use anyhow::Result;

use crate::model::{Category, Kind};
use crate::services::jobs::{Builtin, JobContext, JobKey};
use crate::services::pipeline::stage::Stage;
use crate::state::SharedState;

pub const STAGE: Stage = Stage {
    short: "metadata",
    key: "pipeline.metadata",
    subject_kind: "item",
    concurrency: 8,
    pause_for_playback: false,
    enumerate,
    process,
};

/// Nightly + manual. The detached scan-time enrich covers fresh scans; this stage
/// keeps the ledger honest (misses -> done) and is retriable.
pub const SPEC: Builtin = Builtin {
    key: JobKey("pipeline.metadata"),
    category: Category::Pipeline,
    schedule: Some("15 4 * * *"),
    triggers: &[],
    run,
};

fn run(ctx: &JobContext) -> Result<()> {
    crate::services::pipeline::dispatcher::run(&STAGE, ctx)
}

/// Every movie/loose video + every show, signed by `title:year` (a rename
/// re-queues it). Shows also fold in `episode_count` so gaining a new season /
/// episodes re-queues the show and the fresh episodes get enriched. Episodes
/// inherit their show's metadata, so they are not enumerated here.
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for i in crate::db::list_items(&state.db, None)? {
        if matches!(i.kind, Kind::Movie | Kind::Video) {
            out.push((i.id, format!("{}:{}", i.title, i.year.unwrap_or(0))));
        }
    }
    for s in crate::db::list_shows(&state.db, None)? {
        out.push((s.id, format!("{}:{}:{}", s.title, s.year.unwrap_or(0), s.episode_count)));
    }
    Ok(out)
}

fn process(ctx: &JobContext, id: &str) -> Result<()> {
    // Movies are `items`; shows are not, so a hit on `get_item` means "movie".
    let is_show = crate::db::get_item(&ctx.state.db, id)?.is_none();
    crate::services::enrich::enrich_one(&ctx.state, id, is_show)
}
