//! Pipeline stage `probe`: ffprobe each file (codecs, duration, tracks, embedded
//! chapters) and persist it. Wraps [`crate::infra::probe::probe_one`]. The
//! detached scan-time probe pass still runs for immediate results + the
//! `/api/status` activity panel; this stage is the incremental, observable,
//! retriable view over the same `files.probed` state (a no-op on already-probed
//! files, so the two coexist safely).

use anyhow::Result;

use crate::model::Category;
use crate::services::jobs::{Builtin, JobContext, JobKey};
use crate::services::pipeline::stage::Stage;
use crate::state::SharedState;

pub const STAGE: Stage = Stage {
    short: "probe",
    key: "pipeline.probe",
    subject_kind: "file",
    concurrency: 4,
    pause_for_playback: false,
    enumerate,
    process,
};

/// Nightly + manual. Not chained after a scan: the detached scan-time pass covers
/// that, and racing it would double-ffprobe the same files.
pub const SPEC: Builtin = Builtin {
    key: JobKey("pipeline.probe"),
    category: Category::Pipeline,
    schedule: Some("0 1 * * *"),
    triggers: &[],
    run,
};

fn run(ctx: &JobContext) -> Result<()> {
    crate::services::pipeline::dispatcher::run(&STAGE, ctx)
}

/// Every file, signed by `mtime:size` (a replaced file re-probes).
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    crate::db::all_file_sigs(&state.db)
}

fn process(ctx: &JobContext, file_id: &str) -> Result<()> {
    let Some((abs_path, item_id, probed)) = crate::db::probe_target(&ctx.state.db, file_id)? else {
        return Ok(()); // file row gone since enumerate; nothing to do
    };
    if probed {
        return Ok(()); // already probed (here or by the scan-time pass)
    }
    crate::infra::probe::probe_one(
        &ctx.state.db,
        ctx.state.ffprobe_available,
        &ctx.state.events,
        file_id,
        &abs_path,
        &item_id,
    )
}
