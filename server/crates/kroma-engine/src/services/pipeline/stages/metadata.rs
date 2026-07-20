//! Pipeline stage `metadata`: resolve TMDB metadata (poster/backdrop/overview/
//! cast/IDs) per movie and show. Wraps [`crate::services::enrich::enrich_one`]
//! (idempotent: enriched titles are skipped, shows still run their incremental
//! per-season pass, TMDB misses are recorded `done` so they stop being retried
//! every run the one thing the detached scan-time enrich can't do).

use anyhow::Result;

use crate::model::Kind;
use crate::services::jobs::JobContext;
use crate::state::SharedState;

use super::common::stage;

// Nightly + manual. The detached scan-time enrich covers fresh scans; this stage
// keeps the ledger honest (misses -> done) and is retriable.
stage! {
    short: "metadata",
    subject_kind: "item",
    concurrency: 8,
    pause_for_playback: false,
    schedule: Some("15 4 * * *"),
    triggers: &[],
}

/// Every movie/loose video + every show, signed by `title:year:pin` (a rename or
/// a corrected TMDB match re-queues it). Shows also fold in `episode_count` so
/// gaining a new season / episodes re-queues the show and the fresh episodes get
/// enriched. Episodes inherit their show's metadata, so they are not enumerated
/// here.
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    use crate::db::metadata_core::{ITEM, SHOW};
    let mut out = Vec::new();
    // Folding the operator's pin into the signature is what makes a correction
    // stick: without it the ledger still considers the element done under its old
    // `title:year` and the nightly pass would never revisit it.
    let item_pins = crate::db::tmdb_pin::all_for_kind(&state.db, ITEM)?;
    let show_pins = crate::db::tmdb_pin::all_for_kind(&state.db, SHOW)?;
    for i in crate::db::list_items(&state.db, None)? {
        if matches!(i.kind, Kind::Movie | Kind::Video) {
            let pin = item_pins.get(&i.id).copied().unwrap_or(0);
            out.push((i.id, format!("{}:{}:{pin}", i.title, i.year.unwrap_or(0))));
        }
    }
    for s in crate::db::list_shows(&state.db, None)? {
        let pin = show_pins.get(&s.id).copied().unwrap_or(0);
        out.push((
            s.id,
            format!("{}:{}:{}:{pin}", s.title, s.year.unwrap_or(0), s.episode_count),
        ));
    }
    Ok(out)
}

fn process(ctx: &JobContext, id: &str) -> Result<()> {
    // Movies are `items`; shows are not, so a hit on `get_item` means "movie".
    let is_show = crate::db::get_item(&ctx.state.db, id)?.is_none();
    crate::services::enrich::enrich_one(&ctx.state, id, is_show)
}
