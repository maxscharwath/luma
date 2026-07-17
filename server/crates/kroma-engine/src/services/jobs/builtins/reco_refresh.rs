//! `recommendations.refresh` refresh the in-memory embedding snapshot that
//! powers "For You" / similar / themed rows, so the home sections reflect the
//! latest catalog + art.

use super::prelude::*;

/// Nightly: rebuild the recommendation indexes from the current library.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("recommendations.refresh"),
    category: Category::Recommendations,
    schedule: Some("0 5 * * *"),
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    ctx.info("refreshing recommendation vectors from the database…");
    ctx.state.vectors.refresh_if_stale(&ctx.state.db)?;
    ctx.info("recommendation vectors are up to date");
    Ok(())
}
