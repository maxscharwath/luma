//! `acquisition.import` move completed downloads into the library (hardlink
//! or copy, Plex-style naming) and chain a scan. Triggered by the downloads
//! monitor on completion; the hourly cron catches anything it missed (e.g. an
//! import that failed on a transient filesystem error).

use anyhow::Result;
use luma_module_sdk::engine::model::Category;
use luma_module_sdk::engine::services::jobs::{Builtin, JobContext, JobKey};

pub const SPEC: Builtin = Builtin {
    key: JobKey("acquisition.import"),
    category: Category::Acquisition,
    schedule: Some("10 * * * *"),
    triggers: &[],
    run,
};

pub fn run(ctx: &JobContext) -> Result<()> {
    if super::acquisition_disabled(ctx) {
        return Ok(());
    }
    let summary = crate::import::import_pass(&ctx.state, &|line| ctx.info(line))?;
    if summary.imported == 0 && summary.failed == 0 {
        ctx.info("nothing to import");
    } else {
        ctx.info(format!(
            "imported {} downloads ({} files), {} failed",
            summary.imported, summary.files, summary.failed
        ));
    }
    Ok(())
}
