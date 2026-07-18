//! `acquisition.import` move completed downloads into the library (hardlink
//! or copy, Plex-style naming) and chain a scan. Triggered by the downloads
//! monitor on completion; the hourly cron catches anything it missed (e.g. an
//! import that failed on a transient filesystem error).

crate::jobs::acquisition_job! {
    key: "acquisition.import",
    schedule: Some("10 * * * *"),
    triggers: &[],
    run: |ctx| {
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
}
