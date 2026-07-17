//! `search.reindex` rebuild the in-RAM full-text search index from the database.

use super::prelude::*;

/// Manual-only: rebuild the full-text search index from the catalog.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("search.reindex"),
    category: Category::Library,
    schedule: None,
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    ctx.info("rebuilding the search index from the database…");
    ctx.state.search.reindex_from_db(&ctx.state.db)?;
    ctx.info("search index rebuilt");
    Ok(())
}
