//! `search.reindex` rebuild the in-RAM full-text search index from the database.

use super::prelude::*;

/// Manual, plus chained after the `metadata` stage: that stage rewrites the very
/// fields the index is built from (catalog title on a corrected/pinned match,
/// localized titles, overviews, cast), so a rematch correction or the nightly
/// pass must refresh the index or the new title stays unsearchable. The other
/// two metadata-writing paths already reindex themselves (the scan-time enrich
/// coordinator and the `metadata.enrich` admin job); this covers the third.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("search.reindex"),
    category: Category::Library,
    schedule: None,
    triggers: &[Trigger::AfterJob(JobKey("pipeline.metadata"))],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    ctx.info("rebuilding the search index from the database…");
    ctx.state.search.reindex_from_db(&ctx.state.db)?;
    ctx.info("search index rebuilt");
    Ok(())
}
