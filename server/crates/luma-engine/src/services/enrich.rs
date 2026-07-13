//! Background TMDB enrichment.
//!
//! After a scan persists the catalog, resolve poster / backdrop / overview /
//! IDs for every movie and show and write them into the DB. It runs on a small
//! pool of std threads so a large library (thousands of titles) never blocks
//! startup or the `/api/scan` request the catalog serves immediately and
//! gains art as rows are updated. Reuses the process-wide [`metadata::Cache`] so
//! duplicates and re-scans are cheap.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use tracing::{info, warn};

use crate::services::activity::{self, Shared as Activity};
use crate::db::{self, Pool};
use crate::ports::Embedder;
use crate::infra::events::{Bus, ServerEvent};
use crate::infra::image;
use crate::infra::metadata::{self, Cache, Target};
use crate::infra::theme;
use crate::model::{Kind, MediaItem, Metadata, Show};
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
    /// The TMDB id we already have on file (`Some` once a title is enriched).
    /// When set, the worker skips the (costly, rate-limited) title re-lookup: a
    /// movie is simply done, and a show jumps straight to the still-incremental
    /// per-season episode/cast pass so newly-added seasons are filled without
    /// re-fetching the whole show. `None` means a full first-time enrichment.
    resolved_tmdb: Option<u64>,
}

/// The shared, cloneable bundle a worker needs to resolve one title. Cloning is
/// cheap every field is an `Arc`/handle or small owned value.
#[derive(Clone)]
struct Engine {
    pool: Pool,
    cache: Arc<Cache>,
    api_key: String,
    language: String,
    data_dir: PathBuf,
    theme_songs: bool,
    bus: Bus,
    embedder: Arc<dyn Embedder>,
}

/// Live tallies shared across the worker pool. `processed` counts every attempt
/// (resolved + missed + failed) so a progress bar can reach 100%.
#[derive(Default)]
struct Counters {
    processed: AtomicUsize,
    resolved: AtomicUsize,
    missed: AtomicUsize,
    failed: AtomicUsize,
    /// Workers that have drained the queue and returned.
    finished: AtomicUsize,
}

/// Bumps `Counters::finished` on drop, so a worker is counted done on every exit
/// path including an unwinding panic in `process_job`. `run_tracked`'s poll loop
/// waits for `finished == worker_count`; without this a panicking worker would
/// never increment it and hang the job forever (uncancellable, unretriggerable).
struct FinishGuard<'a>(&'a Counters);
impl Drop for FinishGuard<'_> {
    fn drop(&mut self) {
        self.0.finished.fetch_add(1, Ordering::Relaxed);
    }
}

/// Outcome of a tracked enrichment run, surfaced in the job's logs.
pub struct EnrichSummary {
    pub total: usize,
    pub resolved: usize,
    pub missed: usize,
    pub failed: usize,
    pub cancelled: bool,
}

/// Episodes inherit their show's metadata; enrich movies/loose videos + shows.
///
/// Incremental by default: an already-enriched movie/video is dropped entirely,
/// and an already-enriched show is still enqueued (carrying its known TMDB id)
/// so its per-season episode pass runs, but without re-resolving the show. So a
/// re-run only does genuinely missing work: a full catalog re-fetch is an
/// explicit reset (`db::reset_all_metadata`), not the steady-state cost.
fn build_jobs(items: &[MediaItem], shows: &[Show]) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    for i in items {
        if matches!(i.kind, Kind::Movie | Kind::Video) && i.metadata.is_none() {
            jobs.push(Job {
                id: i.id.clone(),
                target: Target::Movie,
                title: i.title.clone(),
                year: i.year,
                is_show: false,
                resolved_tmdb: None,
            });
        }
    }
    for s in shows {
        // Always enqueue shows even when already enriched: a show that resolved
        // last week may have gained a season this week, and `enrich_episodes`
        // (itself incremental) fills only the new stills/cast. `resolved_tmdb`
        // lets the worker skip the show-level re-lookup for those.
        let resolved_tmdb = s.metadata.as_ref().map(|m| m.tmdb_id).filter(|&id| id != 0);
        jobs.push(Job {
            id: s.id.clone(),
            target: Target::Tv,
            title: s.title.clone(),
            year: s.year,
            is_show: true,
            resolved_tmdb,
        });
    }
    jobs
}

fn engine_for(state: &SharedState, api_key: String) -> Engine {
    Engine {
        pool: state.db.clone(),
        cache: state.metadata_cache.clone(),
        api_key,
        language: crate::services::settings::metadata_language(&state.settings, &state.config),
        data_dir: state.config.data_dir.clone(),
        theme_songs: crate::services::settings::theme_songs_enabled(&state.settings),
        bus: state.events.clone(),
        // Reuse the process-wide embedder (built once in AppState) across workers.
        embedder: state.embedder.clone(),
    }
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
    let jobs = build_jobs(items, shows);
    if jobs.is_empty() {
        return;
    }
    let total = jobs.len();
    info!(titles = total, "starting background TMDB enrichment");
    activity::enrich_started(&state.activity, total);
    spawn(engine_for(state, api_key), state.activity.clone(), state.search.clone(), jobs);
}

/// A [`Metadata`] with every field empty the base for the lightweight metadata
/// we attach to episodes (still) and use to localize season cast photos.
fn blank_metadata() -> Metadata {
    Metadata {
        provider: "tmdb",
        tmdb_id: 0,
        imdb_id: None,
        title: None,
        tagline: None,
        overview: None,
        release_date: None,
        genres: Vec::new(),
        rating: None,
        poster_url: None,
        backdrop_url: None,
        logo_url: None,
        theme_url: None,
        cast: Vec::new(),
        crew: Vec::new(),
        keywords: Vec::new(),
        tvdb_id: None,
        tmdb_url: String::new(),
    }
}

/// Episode-level metadata carrying just the per-episode still (in `backdrop_url`,
/// resolved via the existing `backdropFor`), title and overview.
fn episode_metadata(art: &metadata::EpisodeArt) -> Metadata {
    Metadata {
        title: art.name.clone(),
        overview: art.overview.clone(),
        release_date: art.air_date.clone(),
        rating: art.rating,
        backdrop_url: art.still_url.clone(),
        ..blank_metadata()
    }
}

/// Fetch + store per-episode stills AND the season cast for a show that just
/// resolved (one TMDB call per season). Best-effort never blocks the show art.
/// Seasons whose episodes already have a backdrop AND whose cast is stored are
/// skipped, so re-scans don't refetch.
// Threads the enrichment context; a struct would just move the noise.
#[allow(clippy::too_many_arguments)]
fn enrich_episodes(
    pool: &Pool,
    api_key: &str,
    language: &str,
    langs: &[&str],
    data_dir: &Path,
    bus: &Bus,
    show_id: &str,
    tv_id: u64,
) {
    use db::translations::{self, TransData};
    let Ok(Some(detail)) = db::get_show(pool, show_id) else { return };
    let have_cast = db::seasons_with_cast(pool, show_id).unwrap_or_default();
    for season in &detail.seasons {
        let missing: Vec<&MediaItem> = season
            .episodes
            .iter()
            .filter(|e| e.metadata.as_ref().and_then(|m| m.backdrop_url.as_ref()).is_none())
            .collect();
        let needs_cast = !have_cast.contains(&season.number);
        if missing.is_empty() && !needs_cast {
            continue;
        }
        // One TMDB call per language: localized episode text + character names.
        let per_lang = metadata::season_episodes_multi(api_key, langs, tv_id, season.number);
        if per_lang.is_empty() {
            continue;
        }
        let primary_key = primary_lang(&per_lang, language);
        let data = &per_lang[&primary_key];

        // Per-episode stills (legacy blob) from the primary language, as before.
        if !missing.is_empty() && !data.episodes.is_empty() {
            let by_num: std::collections::HashMap<u32, &metadata::EpisodeArt> =
                data.episodes.iter().map(|a| (a.episode, a)).collect();
            for ep in &missing {
                let Some(num) = ep.episode else { continue };
                let Some(art) = by_num.get(&num) else { continue };
                if art.still_url.is_none() && art.overview.is_none() {
                    continue;
                }
                let meta = image::localize(data_dir, episode_metadata(art));
                match db::set_item_metadata(pool, &ep.id, &meta) {
                    Ok(()) => bus.publish(ServerEvent::ItemUpdated { id: ep.id.clone() }),
                    Err(e) => warn!(id = %ep.id, error = %e, "failed to store episode metadata"),
                }
            }
        }

        // Per-language episode text (title + overview) into the translation cache,
        // for every episode of the season (the still stays invariant on the blob).
        for (lang, sdata) in &per_lang {
            let by_num: std::collections::HashMap<u32, &metadata::EpisodeArt> =
                sdata.episodes.iter().map(|a| (a.episode, a)).collect();
            for ep in &season.episodes {
                let Some(num) = ep.episode else { continue };
                let Some(art) = by_num.get(&num) else { continue };
                let td = TransData { title: art.name.clone(), overview: art.overview.clone(), ..Default::default() };
                if !td.is_empty() {
                    let _ = translations::put(pool, "episode", &ep.id, lang, translations::TMDB, &td);
                }
            }
        }

        // Season cast: localize the primary-language photos (legacy season_meta
        // blob), then store per-language character names in the translation cache
        // aligned by index to that stored cast (TMDB keeps cast order across langs).
        if needs_cast && !data.cast.is_empty() {
            let carrier =
                image::localize(data_dir, Metadata { cast: data.cast.clone(), ..blank_metadata() });
            if let Err(e) = db::set_season_cast(pool, show_id, season.number, &carrier.cast) {
                warn!(show = %show_id, season = season.number, error = %e, "failed to store season cast");
            }
            let sc_id = format!("{show_id}:{}", season.number);
            for (lang, sdata) in &per_lang {
                if sdata.cast.is_empty() {
                    continue;
                }
                let characters: Vec<Option<String>> =
                    sdata.cast.iter().map(|c| c.character.clone()).collect();
                let td = TransData { characters, ..Default::default() };
                let _ = translations::put(pool, "season_cast", &sc_id, lang, translations::TMDB, &td);
            }
        }
    }
}

/// Resolve one title against TMDB and write it back, updating `counters` and
/// publishing a live update so clients swap in the art. With `activity` present
/// (scan path) it also drives the global enrich indicator; tracked runs pass
/// `None` and report progress through the job console instead.
fn process_job(eng: &Engine, counters: &Counters, total: usize, activity: Option<&Activity>, job: Job) {
    // Already enriched: don't re-resolve the title (TMDB is rate-limited). A show
    // still runs its incremental per-season pass to fill any newly-added
    // episodes' stills/cast; a movie has no sub-work and is simply counted done.
    // Base codes we resolve + store a language row for (single source of truth).
    let langs: Vec<&str> = crate::i18n::SUPPORTED_LOCALES.to_vec();
    if let Some(tmdb_id) = job.resolved_tmdb {
        if job.is_show {
            enrich_episodes(
                &eng.pool, &eng.api_key, &eng.language, &langs, &eng.data_dir, &eng.bus, &job.id,
                tmdb_id,
            );
        }
        counters.resolved.fetch_add(1, Ordering::Relaxed);
        bump(eng, counters, total, activity);
        return;
    }
    // Resolve the title once, then fetch its details in every supported language.
    let Some(resolved) = metadata::lookup_all(
        &eng.cache, &eng.api_key, &eng.language, &langs, job.target, &job.title, job.year,
    ) else {
        counters.missed.fetch_add(1, Ordering::Relaxed);
        bump(eng, counters, total, activity);
        return;
    };
    // The primary language backs the legacy `metadata` blob, the embedding, and
    // the localized art we keep as invariant household base code, else English,
    // else any. `by_lang` is non-empty by construction.
    let primary_key = primary_lang(&resolved.by_lang, &eng.language);
    let Some(meta) = resolved.by_lang.get(&primary_key).cloned() else {
        counters.missed.fetch_add(1, Ordering::Relaxed);
        bump(eng, counters, total, activity);
        return;
    };
    // Download + transcode poster/backdrop (+ cast photos) to local WebP invariant,
    // so done once on the primary and shared by every language row.
    let meta = image::localize(&eng.data_dir, meta);
    // Download the show's theme song when the feature is on (TV only; movies
    // carry no tvdb_id, so it's a no-op for them). Disabled → theme_url stays
    // None, so a re-scan also clears any theme cached while it was enabled.
    let meta = if eng.theme_songs { theme::localize(&eng.data_dir, meta) } else { meta };
    // Embed the title from its (title, year, genres, cast, overview) for
    // similar-to / themed / "For You" rows.
    let doc = luma_domain::build_doc(&job.title, job.year, &meta);
    let vector = eng.embedder.embed(&doc);
    let write = if job.is_show {
        db::set_show_metadata(&eng.pool, &job.id, &meta)
    } else {
        db::set_item_metadata(&eng.pool, &job.id, &meta)
    };
    match write {
        Ok(()) => {
            // Dual-write the language-agnostic cache: the invariant core once, plus
            // a translation row per supported language (title/overview/genres/
            // characters). Best-effort a failure here must not drop the blob/art.
            let kind = if job.is_show { db::metadata_core::SHOW } else { db::metadata_core::ITEM };
            if let Err(e) = db::store_localized(&eng.pool, kind, &job.id, &meta, &resolved.by_lang) {
                warn!(id = %job.id, error = %e, "failed to store localized metadata cache");
            }
            // Best-effort: a vector failure must not drop the art.
            if let Err(e) = db::set_item_vector(&eng.pool, &job.id, &vector) {
                warn!(id = %job.id, error = %e, "failed to store embedding");
            }
            // Per-episode stills (+ per-language episode title/overview) for shows,
            // once the show itself has resolved. Best-effort.
            if job.is_show && meta.tmdb_id != 0 {
                enrich_episodes(
                    &eng.pool, &eng.api_key, &eng.language, &langs, &eng.data_dir, &eng.bus,
                    &job.id, meta.tmdb_id,
                );
            }
            counters.resolved.fetch_add(1, Ordering::Relaxed);
            eng.bus.publish(if job.is_show {
                ServerEvent::ShowUpdated { id: job.id.clone() }
            } else {
                ServerEvent::ItemUpdated { id: job.id.clone() }
            });
        }
        Err(e) => {
            counters.failed.fetch_add(1, Ordering::Relaxed);
            warn!(id = %job.id, error = %e, "failed to store metadata");
        }
    }
    bump(eng, counters, total, activity);
}

/// Pick the primary language key from a per-language map: the household base code
/// (e.g. `"fr"` from `"fr-FR"`) if present, else English, else any. The map is
/// assumed non-empty (the caller guarantees at least one fetched language).
fn primary_lang<T>(map: &std::collections::HashMap<String, T>, household: &str) -> String {
    let base = household.split(['-', '_']).next().unwrap_or("en");
    if map.contains_key(base) {
        base.to_string()
    } else if map.contains_key("en") {
        "en".to_string()
    } else {
        map.keys().next().cloned().unwrap_or_default()
    }
}

/// Advance the processed counter and, on the scan path, feed the global enrich
/// indicator (activity panel + a periodic bus event).
fn bump(eng: &Engine, counters: &Counters, total: usize, activity: Option<&Activity>) {
    let done = counters.processed.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(activity) = activity {
        activity::enrich_progress(activity, done);
        if done.is_multiple_of(25) {
            eng.bus.publish(ServerEvent::EnrichProgress { done, total });
        }
    }
}

/// Detached, fire-and-forget enrichment for the scan path. A coordinator thread
/// owns the workers and rebuilds the search index on completion, so the caller
/// never blocks.
fn spawn(eng: Engine, activity: Activity, search: Arc<SearchEngine>, jobs: Vec<Job>) {
    let total = jobs.len();
    let queue = Arc::new(Mutex::new(jobs));
    let counters = Arc::new(Counters::default());
    thread::spawn(move || {
        let worker_count = WORKERS.min(total.max(1));
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let (eng, queue, counters, activity) =
                (eng.clone(), queue.clone(), counters.clone(), activity.clone());
            handles.push(thread::spawn(move || loop {
                let job = match queue.lock().unwrap().pop() {
                    Some(j) => j,
                    None => break,
                };
                process_job(&eng, &counters, total, Some(&activity), job);
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let resolved = counters.resolved.load(Ordering::Relaxed);
        activity::enrich_completed(&activity);
        info!(resolved, total, "TMDB enrichment complete");
        eng.bus.publish(ServerEvent::EnrichCompleted { resolved, total });
        // Now that cast / overview / genres / localized titles are persisted,
        // rebuild the search index so they become searchable.
        match search.reindex_from_db(&eng.pool) {
            Ok(()) => info!("search index rebuilt after enrichment"),
            Err(e) => warn!(error = %e, "search reindex after enrichment failed"),
        }
    });
}

/// Enrich ONE title (movie/video or show) the unit of work of the
/// `pipeline.metadata` stage. Idempotent: an already-enriched movie is a no-op,
/// an already-enriched show only runs its incremental per-season episode pass, and
/// a TMDB *miss* returns `Ok` (the ledger records it done, so it is not retried
/// every run). Returns `Err` only on a real write failure.
pub fn enrich_one(state: &SharedState, id: &str, is_show: bool) -> anyhow::Result<()> {
    let Some(api_key) = state.config.tmdb_api_key.clone() else {
        return Ok(());
    };
    let job = if is_show {
        let Some(show) = db::get_show(&state.db, id)?.map(|d| d.show) else {
            return Ok(());
        };
        Job {
            id: show.id.clone(),
            target: Target::Tv,
            title: show.title.clone(),
            year: show.year,
            is_show: true,
            resolved_tmdb: show.metadata.as_ref().map(|m| m.tmdb_id).filter(|&i| i != 0),
        }
    } else {
        let Some(item) = db::get_item(&state.db, id)? else {
            return Ok(());
        };
        Job {
            id: item.id.clone(),
            target: Target::Movie,
            title: item.title.clone(),
            year: item.year,
            is_show: false,
            // Prefer an already-resolved id; else adopt a pinned id from an
            // acquisition (so the real TMDB movie is fetched, not a title guess).
            resolved_tmdb: item
                .metadata
                .as_ref()
                .map(|m| m.tmdb_id)
                .filter(|&i| i != 0)
                .or_else(|| state.db.get().ok().and_then(|c| db::tmdb_hint(&c, &item.id).ok().flatten())),
        }
    };
    let eng = engine_for(state, api_key);
    let counters = Counters::default();
    process_job(&eng, &counters, 1, None, job);
    if counters.failed.load(Ordering::Relaxed) > 0 {
        anyhow::bail!("failed to store metadata for {id}");
    }
    Ok(())
}

/// Re-enrich the catalog synchronously (blocking the caller) so a job can track
/// real progress, duration and per-run counts. Reports via `progress(done,
/// total)` and stops early when `cancelled()` returns true. Unlike
/// [`maybe_spawn`] this ignores the `tmdb_enrich` toggle it's an explicit
/// admin action but no-ops without an API key or titles.
pub fn run_tracked(
    state: &SharedState,
    items: &[MediaItem],
    shows: &[Show],
    progress: impl Fn(usize, usize),
    cancelled: impl Fn() -> bool,
) -> EnrichSummary {
    let jobs = build_jobs(items, shows);
    let total = jobs.len();
    let Some(api_key) = state.config.tmdb_api_key.clone() else {
        return EnrichSummary { total, resolved: 0, missed: 0, failed: 0, cancelled: false };
    };
    if total == 0 {
        return EnrichSummary { total, resolved: 0, missed: 0, failed: 0, cancelled: false };
    }
    let eng = engine_for(state, api_key);
    let queue = Arc::new(Mutex::new(jobs));
    let counters = Arc::new(Counters::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_count = WORKERS.min(total.max(1));
    let mut handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let (eng, queue, counters, cancel) =
            (eng.clone(), queue.clone(), counters.clone(), cancel.clone());
        handles.push(thread::spawn(move || {
            // Mark this worker finished on EVERY exit path, including a panic in
            // process_job: the poll loop below terminates on `finished ==
            // worker_count`, so a bare `fetch_add` after the loop would be skipped
            // by an unwinding worker and hang the (uncancellable) job forever.
            let _done = FinishGuard(&counters);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let job = match queue.lock().unwrap().pop() {
                    Some(j) => j,
                    None => break,
                };
                process_job(&eng, &counters, total, None, job);
            }
        }));
    }
    // Poll progress + propagate cancellation from this (blocking) job thread.
    loop {
        thread::sleep(std::time::Duration::from_millis(250));
        progress(counters.processed.load(Ordering::Relaxed), total);
        if cancelled() {
            cancel.store(true, Ordering::Relaxed);
        }
        if counters.finished.load(Ordering::Relaxed) >= worker_count {
            break;
        }
    }
    for h in handles {
        let _ = h.join();
    }
    progress(counters.processed.load(Ordering::Relaxed), total);
    EnrichSummary {
        total,
        resolved: counters.resolved.load(Ordering::Relaxed),
        missed: counters.missed.load(Ordering::Relaxed),
        failed: counters.failed.load(Ordering::Relaxed),
        cancelled: cancel.load(Ordering::Relaxed),
    }
}
