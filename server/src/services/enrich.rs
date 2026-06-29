//! Background TMDB enrichment.
//!
//! After a scan persists the catalog, resolve poster / backdrop / overview /
//! IDs for every movie and show and write them into the DB. It runs on a small
//! pool of std threads so a large library (thousands of titles) never blocks
//! startup or the `/api/scan` request — the catalog serves immediately and
//! gains art as rows are updated. Reuses the process-wide [`metadata::Cache`] so
//! duplicates and re-scans are cheap.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use tracing::{info, warn};

use crate::services::activity::{self, Shared as Activity};
use crate::db::{self, Pool};
use crate::infra::embed::{self, Embedder};
use crate::infra::events::{Bus, ServerEvent};
use crate::infra::image;
use crate::infra::metadata::{self, Cache, Target};
use crate::model::{Kind, MediaItem, Show};
use crate::services::search::SearchEngine;
use crate::state::SharedState;

/// Max concurrent TMDB lookups. Small enough to stay polite (TMDB allows ~50
/// rps) and to keep SQLite write contention negligible.
const WORKERS: usize = 8;

/// One title to resolve against TMDB and write back.
struct Job {
    id: String,
    target: Target,
    title: String,
    year: Option<u32>,
    is_show: bool,
}

/// Spawn background enrichment for a freshly-scanned catalog, if enabled.
/// Returns immediately; work proceeds on detached threads.
pub fn maybe_spawn(state: &SharedState, items: &[MediaItem], shows: &[Show]) {
    let Some(api_key) = state.config.tmdb_api_key.clone() else {
        return;
    };
    if !state.config.tmdb_enrich {
        return;
    }

    let mut jobs: Vec<Job> = Vec::new();
    for i in items {
        // Episodes inherit their show's metadata; enrich movies/loose videos.
        if matches!(i.kind, Kind::Movie | Kind::Video) {
            jobs.push(Job {
                id: i.id.clone(),
                target: Target::Movie,
                title: i.title.clone(),
                year: i.year,
                is_show: false,
            });
        }
    }
    for s in shows {
        jobs.push(Job {
            id: s.id.clone(),
            target: Target::Tv,
            title: s.title.clone(),
            year: s.year,
            is_show: true,
        });
    }
    if jobs.is_empty() {
        return;
    }

    let total = jobs.len();
    info!(titles = total, "starting background TMDB enrichment");
    activity::enrich_started(&state.activity, total);
    // Reuse the process-wide embedder (built once in AppState) across workers.
    let embedder = state.embedder.clone();
    spawn(
        state.db.clone(),
        state.metadata_cache.clone(),
        api_key,
        state.config.tmdb_language.clone(),
        state.config.data_dir.clone(),
        state.events.clone(),
        state.activity.clone(),
        embedder,
        state.search.clone(),
        jobs,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    pool: Pool,
    cache: Arc<Cache>,
    api_key: String,
    language: String,
    data_dir: PathBuf,
    bus: Bus,
    activity: Activity,
    embedder: Arc<dyn Embedder>,
    search: Arc<SearchEngine>,
    jobs: Vec<Job>,
) {
    let total = jobs.len();
    let queue = Arc::new(Mutex::new(jobs));
    let resolved = Arc::new(AtomicUsize::new(0));

    // A coordinator thread owns the workers and logs a summary on completion,
    // so the caller never blocks.
    thread::spawn(move || {
        let worker_count = WORKERS.min(total.max(1));
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let pool = pool.clone();
            let cache = cache.clone();
            let api_key = api_key.clone();
            let language = language.clone();
            let data_dir = data_dir.clone();
            let queue = queue.clone();
            let resolved = resolved.clone();
            let bus = bus.clone();
            let activity = activity.clone();
            let embedder = embedder.clone();
            handles.push(thread::spawn(move || {
                loop {
                    let job = match queue.lock().unwrap().pop() {
                        Some(j) => j,
                        None => break,
                    };
                    let Some(meta) = metadata::lookup(
                        &cache,
                        &api_key,
                        &language,
                        job.target,
                        &job.title,
                        job.year,
                    ) else {
                        continue;
                    };
                    // Download + transcode poster/backdrop to local WebP.
                    let meta = image::localize(&data_dir, meta);
                    // Embed the title from its (title, year, genres, cast,
                    // overview) for similar-to / themed / "For You" rows.
                    let doc = embed::build_doc(&job.title, job.year, &meta);
                    let vector = embedder.embed(&doc);
                    let write = if job.is_show {
                        db::set_show_metadata(&pool, &job.id, &meta)
                    } else {
                        db::set_item_metadata(&pool, &job.id, &meta)
                    };
                    match write {
                        Ok(()) => {
                            // Best-effort: a vector failure must not drop the art.
                            if let Err(e) = db::set_item_vector(&pool, &job.id, &vector) {
                                warn!(id = %job.id, error = %e, "failed to store embedding");
                            }
                            let done = resolved.fetch_add(1, Ordering::Relaxed) + 1;
                            activity::enrich_progress(&activity, done);
                            // Push a live update so clients can swap in the art.
                            bus.publish(if job.is_show {
                                ServerEvent::ShowUpdated { id: job.id.clone() }
                            } else {
                                ServerEvent::ItemUpdated { id: job.id.clone() }
                            });
                            // Periodic progress for a scan/refresh indicator.
                            if done % 25 == 0 {
                                bus.publish(ServerEvent::EnrichProgress { done, total });
                            }
                        }
                        Err(e) => warn!(id = %job.id, error = %e, "failed to store metadata"),
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let resolved = resolved.load(Ordering::Relaxed);
        activity::enrich_completed(&activity);
        info!(resolved, total, "TMDB enrichment complete");
        bus.publish(ServerEvent::EnrichCompleted { resolved, total });

        // Now that cast / overview / genres / localized titles are persisted,
        // rebuild the search index so they become searchable.
        match search.reindex_from_db(&pool) {
            Ok(()) => info!("search index rebuilt after enrichment"),
            Err(e) => warn!(error = %e, "search reindex after enrichment failed"),
        }
    });
}
