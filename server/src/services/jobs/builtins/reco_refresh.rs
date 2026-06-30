//! `recommendations.refresh` refresh the in-memory embedding snapshot that
//! powers "For You" / similar / themed rows, so the home sections reflect the
//! latest catalog + art.

use super::prelude::*;

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    ctx.info("refreshing recommendation vectors from the database…");
    ctx.state.vectors.refresh_if_stale(&ctx.state.db)?;
    ctx.info("recommendation vectors are up to date");
    Ok(())
}
