//! `library.missing` scan the whole library for Sonarr-style missing episodes:
//! for every show with a resolved TMDB id, diff TMDB's aired episode list against
//! what is on disk and record the gaps, so the Wanted/Missing view surfaces
//! series with missing episodes even when they were never requested. Daily by
//! default (a full scan is TMDB-heavy); also runnable on demand from Tâches.

use super::prelude::*;

pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("library.missing"),
    category: Category::Library,
    // Daily at 04:30: after the nightly library scan / enrich, before waking hours.
    schedule: Some("30 4 * * *"),
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    let state = &ctx.state;
    if state.config.tmdb_api_key.is_none() {
        ctx.warn("no TMDB API key configured nothing to scan for missing episodes");
        return Ok(());
    }
    let summary = crate::services::library_missing::scan(
        state,
        &|done, total| ctx.progress(done, total),
        &|| ctx.cancelled(),
    )?;
    ctx.info(format!(
        "scanned {} shows, {} with missing episodes ({} aired episodes not on disk)",
        summary.shows, summary.with_gaps, summary.episodes
    ));
    Ok(())
}
