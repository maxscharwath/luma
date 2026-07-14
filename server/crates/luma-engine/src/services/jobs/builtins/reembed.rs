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

    let mut embedded = 0usize;
    let mut skipped = 0usize;
    // Collect the titles that need a fresh vector (id + its document), skipping
    // any already at the active dim. Then embed in CHUNKS: with the embedder now
    // out-of-process (the dev.luma.vector .lmod), one `embed_batch` per chunk is
    // a single round-trip, versus one IPC per title (thousands) for `embed`.
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut consider = |id: &str, title: &str, year: Option<u32>, meta: Option<&crate::model::Metadata>| {
        if current.get(id).copied() == Some(target) {
            skipped += 1; // already current leave it
        } else if let Some(meta) = meta {
            pending.push((id.to_string(), build_doc(title, year, meta)));
        }
    };
    for m in movies {
        consider(&m.id, &m.title, m.year, m.metadata.as_ref());
    }
    for s in &shows {
        consider(&s.id, &s.title, s.year, s.metadata.as_ref());
    }

    const CHUNK: usize = 128;
    let mut done = skipped;
    for chunk in pending.chunks(CHUNK) {
        if ctx.cancelled() {
            ctx.warn("cancellation requested stopping");
            return Ok(());
        }
        let docs: Vec<String> = chunk.iter().map(|(_, doc)| doc.clone()).collect();
        // On an absent sidecar `embed_batch` returns empty → the zip is empty and
        // nothing is stored (graceful no-op), matching the old NoopEmbedder path.
        for ((id, _), vec) in chunk.iter().zip(embedder.embed_batch(&docs)) {
            match crate::db::set_item_vector(&state.db, id, &vec) {
                Ok(()) => embedded += 1,
                Err(e) => ctx.error(format!("{id}: failed to store vector: {e}")),
            }
        }
        done += chunk.len();
        ctx.progress(done, total);
    }

    ctx.info(format!("re-embedded {embedded} titles, skipped {skipped} already at dim {target}"));
    state.vectors.refresh_if_stale(&state.db)?;
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(())
}
