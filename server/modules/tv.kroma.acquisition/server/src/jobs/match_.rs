//! `acquisition.match` flip media requests to available / partially
//! available once their titles exist in the local catalog. Chained after every
//! `library.scan` (imports and manual additions alike), plus a daily safety-net
//! cron: the tmdbId a request matches on is written by ENRICHMENT, which lags
//! the scan itself, so the cron catches titles that resolved in between.

use anyhow::Result;
use kroma_module_sdk::engine::model::Category;
use kroma_module_sdk::engine::services::jobs::{Builtin, JobContext, JobKey, Trigger};

pub const SPEC: Builtin = Builtin {
    key: JobKey("acquisition.match"),
    category: Category::Acquisition,
    schedule: Some("30 5 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("library.scan"))],
    run,
};

pub fn run(ctx: &JobContext) -> Result<()> {
    if super::acquisition_disabled(ctx) {
        return Ok(());
    }
    let summary = kroma_module_sdk::engine::services::requests::availability_pass(&ctx.state)?;
    if summary.checked == 0 {
        ctx.info("no open requests to match");
    } else {
        ctx.info(format!(
            "matched {} open requests against the catalog, {} changed state",
            summary.checked, summary.changed
        ));
    }
    Ok(())
}
