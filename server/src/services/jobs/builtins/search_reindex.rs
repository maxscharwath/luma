//! `search.reindex` rebuild the in-RAM full-text search index from the database.

use super::prelude::*;

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    ctx.info("rebuilding the search index from the database…");
    ctx.state.search.reindex_from_db(&ctx.state.db)?;
    ctx.info("search index rebuilt");
    Ok(())
}
