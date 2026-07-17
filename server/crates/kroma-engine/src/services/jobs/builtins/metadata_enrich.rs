//! `metadata.enrich` re-resolve TMDB metadata (posters, overviews, embeddings)
//! for the whole catalog. Runs the enrichment to completion within the run so
//! the Tâches console tracks real progress, duration and per-run counts.

use super::prelude::*;

/// Manual-only: fetch posters/backdrops/metadata for items missing it.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("metadata.enrich"),
    category: Category::Library,
    schedule: None,
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    let state = &ctx.state;
    if state.config.tmdb_api_key.is_none() {
        ctx.warn("no TMDB API key configured nothing to enrich");
        return Ok(());
    }
    let items = crate::db::list_items(&state.db, None)?;
    let shows = crate::db::list_shows(&state.db, None)?;
    let titles =
        items.iter().filter(|i| !matches!(i.kind, crate::model::Kind::Episode)).count();
    ctx.info(format!("re-enriching {titles} movies/videos and {} shows from TMDB…", shows.len()));

    let summary = crate::services::enrich::run_tracked(
        state,
        &items,
        &shows,
        |done, total| ctx.progress(done, total),
        || ctx.cancelled(),
    );

    let done = summary.resolved + summary.missed + summary.failed;
    if summary.cancelled {
        ctx.warn(format!(
            "cancelled after {done}/{} titles {} enriched, {} without a match, {} failed",
            summary.total, summary.resolved, summary.missed, summary.failed
        ));
        return Ok(());
    }
    ctx.info(format!(
        "enriched {} titles, {} without a TMDB match, {} failed (of {})",
        summary.resolved, summary.missed, summary.failed, summary.total
    ));

    // Freshly-resolved cast / overview / localized titles are now persisted
    // rebuild the search index so they become searchable.
    ctx.info("rebuilding search index…");
    match state.search.reindex_from_db(&state.db) {
        Ok(()) => ctx.info("search index rebuilt"),
        Err(e) => ctx.error(format!("search reindex failed: {e:#}")),
    }
    Ok(())
}
