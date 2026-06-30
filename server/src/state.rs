//! Process-wide application state. The library lives in SQLite; this just holds
//! the connection pool, resolved config, and the ffprobe-availability flag.

use std::sync::Arc;

use crate::services::activity;
use crate::config::Config;
use crate::db::Pool;
use crate::infra::embed::{self, Embedder};
use crate::infra::events::Bus;
use crate::infra::metadata;
use crate::infra::metrics::Metrics;
use crate::services::jobs::JobManager;
use crate::services::playback::Registry;
use crate::services::quickconnect::{self, QuickConnect};
use crate::services::search::SearchEngine;
use crate::services::sections::VectorCache;
use crate::services::settings::Settings;
use crate::infra::hls;

pub struct AppState {
    pub config: Config,
    /// Whether the `ffprobe` binary was found at startup.
    pub ffprobe_available: bool,
    pub db: Pool,
    /// Persisted, runtime-editable server settings (admin console).
    pub settings: Settings,
    /// In-memory TMDB lookup cache, shared across requests and the background
    /// enrichment threads (hence `Arc`).
    pub metadata_cache: Arc<metadata::Cache>,
    /// Real-time event bus fanned out to WebSocket clients.
    pub events: Bus,
    /// Live scan/enrichment status snapshot (served at `/api/status`).
    pub activity: activity::Shared,
    /// On-demand HLS engine: keyframe-indexed complete-VOD playlists + cached
    /// stream-copy fMP4 segments (video copy, audio copy or AAC) for browsers
    /// that can't direct-play the container/audio, and seamless language switch.
    pub hls: hls::HlsEngine,
    /// In-flight Quick Connect device-pairing requests.
    pub quickconnect: QuickConnect,
    /// Live playback sessions (the dashboard's "En cours de lecture" panel).
    pub playback: Registry,
    /// Rolling CPU / RAM / bandwidth metrics (the dashboard charts).
    pub metrics: Metrics,
    /// Content embedder, built once at startup (the MiniLM backend loads a model;
    /// the default lexical one is free). Used to embed titles during enrichment
    /// and free-text queries for the `/api/themed` row.
    pub embedder: Arc<dyn Embedder>,
    /// In-RAM full-text search index (keyword/typo-tolerant title search behind
    /// `/api/search`). Rebuilt from SQLite on scan/enrich. Internally synchronized.
    pub search: Arc<SearchEngine>,
    /// In-RAM snapshot of every title's embedding, powering the home-screen
    /// section generator without re-reading SQLite per request. Self-reloads when
    /// the vectors change (see [`crate::services::sections::VectorCache`]).
    pub vectors: Arc<VectorCache>,
    /// Background job registry + cron scheduler (admin "Tâches" console). Built
    /// at startup with the built-in jobs; the scheduler is spawned in `main`.
    pub jobs: Arc<JobManager>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: Config, ffprobe_available: bool, db: Pool, settings: Settings) -> SharedState {
        let hls = hls::HlsEngine::new(&config.data_dir, crate::services::settings::max_transcodes(&settings));
        // Build the job registry: register the built-ins, then overlay any
        // persisted schedule overrides. The cron loop is spawned in `main`.
        let mut jobs = JobManager::new();
        crate::services::jobs::register_all(&mut jobs);
        jobs.load_schedules(&db);
        // Any run left `running` belongs to a previous process that died mid-job;
        // mark it failed so it doesn't show as forever-running in the console.
        let _ = crate::db::reconcile_running_runs(&db);
        Arc::new(AppState {
            config,
            ffprobe_available,
            db,
            settings,
            metadata_cache: Arc::new(metadata::Cache::new()),
            events: Bus::new(),
            activity: activity::new(),
            hls,
            quickconnect: quickconnect::new(),
            playback: Registry::new(),
            metrics: Metrics::new(),
            embedder: embed::default_embedder(),
            search: Arc::new(SearchEngine::new().expect("init search index")),
            vectors: Arc::new(VectorCache::new()),
            jobs: Arc::new(jobs),
        })
    }
}
