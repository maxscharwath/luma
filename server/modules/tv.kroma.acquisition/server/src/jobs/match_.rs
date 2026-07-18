//! `acquisition.match` flip media requests to available / partially
//! available once their titles exist in the local catalog. Chained after every
//! `library.scan` (imports and manual additions alike), plus a daily safety-net
//! cron: the tmdbId a request matches on is written by ENRICHMENT, which lags
//! the scan itself, so the cron catches titles that resolved in between.

use kroma_module_sdk::engine::services::jobs::{JobKey, Trigger};

crate::jobs::acquisition_job! {
    key: "acquisition.match",
    schedule: Some("30 5 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("library.scan"))],
    run: |ctx| {
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
}
