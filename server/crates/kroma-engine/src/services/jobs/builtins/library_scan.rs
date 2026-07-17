//! `library.scan` full library rescan (phase 1) + sync, then kick phase-2
//! probing, search reindex and TMDB enrichment the same pipeline as `POST
//! /api/scan`, shared via `services::scan`.

use super::prelude::*;

/// Manual + debounced library-watch: rescan the media folders for changes.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("library.scan"),
    category: Category::Library,
    schedule: None,
    triggers: &[Trigger::LibraryChange],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    let state = &ctx.state;
    ctx.info("scanning libraries (walk + sync)…");
    let data = crate::services::scan::scan_and_publish(state)?;
    ctx.info(format!(
        "scan complete {} libraries, {} shows, {} items",
        data.libraries.len(),
        data.shows.len(),
        data.items.len()
    ));

    if ctx.cancelled() {
        return Ok(());
    }
    crate::services::scan::spawn_follow_ups(state, &data);
    ctx.info("dispatched probing, search reindex and TMDB enrichment");
    Ok(())
}
