//! `acquisition.search` the automatic wanted-list pass: search enabled
//! indexers for every due wanted row (aired, still wanted, least recently
//! searched first) and grab the best accepted release per target. Fired by
//! the cron and immediately after a request is approved.

crate::jobs::acquisition_job! {
    key: "acquisition.search",
    schedule: Some("*/30 * * * *"),
    triggers: &[],
    run: |ctx| {
        let summary = crate::auto::auto_search_pass(
            &ctx.state,
            &|line| ctx.info(line),
            &|| ctx.cancelled(),
        )?;
        for e in summary.errors.iter().take(10) {
            ctx.warn(e.clone());
        }
        ctx.info(format!(
            "searched {} targets across {} requests, grabbed {}",
            summary.targets, summary.requests, summary.grabbed
        ));
        Ok(())
    }
}
