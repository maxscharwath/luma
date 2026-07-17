//! Pipeline stage `embed`: compute the content embedding (for "For You" / similar
//! rows) per title from its stored metadata, using the active embedder. Depends on
//! `metadata` (only titles that already have metadata are enumerated). Signed by
//! the embedder's dimension, so switching models re-embeds everything; a vector
//! already at the current dim is a no-op.

use anyhow::{anyhow, Result};

use kroma_domain::build_doc;
use crate::model::{Category, Kind, Metadata};
use crate::services::jobs::{Builtin, JobContext, JobKey, Trigger};
use crate::services::pipeline::stage::Stage;
use crate::state::SharedState;

pub const STAGE: Stage = Stage {
    short: "embed",
    key: "pipeline.embed",
    subject_kind: "item",
    concurrency: 4,
    // BERT embedding is in-process CPU work; yield it to live playback too.
    pause_for_playback: true,
    enumerate,
    process,
};

/// Nightly + after the metadata stage (fresh metadata -> fresh embedding) + manual.
pub const SPEC: Builtin = Builtin {
    key: JobKey("pipeline.embed"),
    category: Category::Pipeline,
    schedule: Some("45 4 * * *"),
    triggers: &[Trigger::AfterJob(JobKey("pipeline.metadata"))],
    run,
};

fn run(ctx: &JobContext) -> Result<()> {
    crate::services::pipeline::dispatcher::run(&STAGE, ctx)
}

/// Every movie/show that already has metadata, signed by the active embedder's
/// dimension so a model switch re-queues them all.
fn enumerate(state: &SharedState) -> Result<Vec<(String, String)>> {
    let sig = state.embedder.dim().to_string();
    let (items, shows) = crate::db::index_snapshot(&state.db)?;
    let mut out = Vec::new();
    for i in items {
        if !matches!(i.kind, Kind::Episode) && i.metadata.is_some() {
            out.push((i.id, sig.clone()));
        }
    }
    for s in shows {
        if s.metadata.is_some() {
            out.push((s.id, sig.clone()));
        }
    }
    Ok(out)
}

fn process(ctx: &JobContext, id: &str) -> Result<()> {
    let embedder = ctx.state.embedder.clone();
    let target = embedder.dim();
    // Already at the active dim? nothing to do. Single-row lookup (not the whole
    // `item_vectors` table) so a full re-embed stays O(N), not O(N^2).
    if crate::db::vector_dim(&ctx.state.db, id)? == Some(target) {
        return Ok(());
    }
    let Some((title, year, meta)) = title_year_meta(&ctx.state, id)? else {
        return Ok(()); // gone, or no metadata yet (waiting on the metadata stage)
    };
    let vec = embedder.embed(&build_doc(&title, year, &meta));
    crate::db::set_item_vector(&ctx.state.db, id, &vec).map_err(|e| anyhow!(e))
}

/// `(title, year, metadata)` for a movie or show id, or `None` when it has no
/// metadata to embed yet.
fn title_year_meta(
    state: &SharedState,
    id: &str,
) -> Result<Option<(String, Option<u32>, Metadata)>> {
    if let Some(item) = crate::db::get_item(&state.db, id)? {
        return Ok(item.metadata.map(|m| (item.title, item.year, m)));
    }
    if let Some(detail) = crate::db::get_show(&state.db, id)? {
        let show = detail.show;
        return Ok(show.metadata.map(|m| (show.title, show.year, m)));
    }
    Ok(None)
}
