//! `recommendations.reembed` manual-only. Re-embed every title from its
//! **stored** metadata with the active embedder for after an embedder switch
//! (e.g. enabling MiniLM, 256→384-dim), WITHOUT re-hitting TMDB. Until this runs,
//! recommendations are empty whenever the stored vector dimension no longer
//! matches the embedder. Refreshes the in-memory vector cache when done.

use super::prelude::*;

/// Manual-only: recompute content embeddings (heavy; run on demand).
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("recommendations.reembed"),
    category: Category::Recommendations,
    schedule: None,
    triggers: &[],
    run,
};

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    use luma_domain::build_doc;
    use crate::infra::events::ServerEvent;
    use crate::model::Kind;

    let state = &ctx.state;
    let embedder = state.embedder.clone();
    let target = embedder.dim();
    // Skip vectors already at the active dim → an embedder switch (or a re-run
    // after an interrupted pass) only touches what's stale.
    let current = crate::db::vector_dims(&state.db)?;
    let (items, shows) = crate::db::index_snapshot(&state.db)?;
    // Movies/loose videos + shows carry metadata; episodes inherit (no vector).
    let movies: Vec<&crate::model::MediaItem> =
        items.iter().filter(|i| !matches!(i.kind, Kind::Episode)).collect();
    let total = movies.len() + shows.len();
    ctx.info(format!("re-embedding to dim {target} ({total} titles; skipping any already at {target})"));

    let mut done = 0usize;
    let mut embedded = 0usize;
    let mut skipped = 0usize;
    let mut embed_one = |id: &str, title: &str, year: Option<u32>, meta: Option<&crate::model::Metadata>| {
        done += 1;
        ctx.progress(done, total);
        if current.get(id).copied() == Some(target) {
            skipped += 1; // already current leave it
            return;
        }
        if let Some(meta) = meta {
            let vec = embedder.embed(&build_doc(title, year, meta));
            match crate::db::set_item_vector(&state.db, id, &vec) {
                Ok(()) => embedded += 1,
                Err(e) => ctx.error(format!("{id}: failed to store vector: {e}")),
            }
        }
    };

    for m in movies {
        if ctx.cancelled() {
            ctx.warn("cancellation requested stopping");
            return Ok(());
        }
        embed_one(&m.id, &m.title, m.year, m.metadata.as_ref());
    }
    for s in &shows {
        if ctx.cancelled() {
            ctx.warn("cancellation requested stopping");
            return Ok(());
        }
        embed_one(&s.id, &s.title, s.year, s.metadata.as_ref());
    }

    ctx.info(format!("re-embedded {embedded} titles, skipped {skipped} already at dim {target}"));
    state.vectors.refresh_if_stale(&state.db)?;
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(())
}
