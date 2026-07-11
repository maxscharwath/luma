//! Process-wide application state. The library lives in SQLite; this just holds
//! the connection pool, resolved config, and the ffprobe-availability flag.

use std::sync::{Arc, RwLock};

use luma_module_wasm::WasmHost;

use crate::services::activity;
use crate::config::Config;
use crate::db::Pool;
use crate::services::downloads::DownloadManager;
use crate::services::vpn::Vpn;
use crate::infra::embed::{self, Embedder};
use crate::infra::events::Bus;
use crate::infra::metadata;
use crate::infra::metrics::Metrics;
use crate::infra::storyboard::Storyboard;
use crate::services::jobs::JobManager;
use crate::services::playback::Registry;
use crate::services::quickconnect::{self, QuickConnect};
use crate::services::remote::RemoteAccess;
use crate::services::search::SearchEngine;
use crate::services::sections::VectorCache;
use crate::services::settings::Settings;
use crate::services::subtitles::GenRegistry;
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
    /// Scrub-bar preview sprite sheets (YouTube-style hover thumbnails), built
    /// once per file with one ffmpeg pass and cached on disk.
    pub storyboard: Storyboard,
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
    /// In-flight on-device subtitle generations (Whisper / translate), tracked so
    /// the player can poll live progress + ETA and cancel.
    pub subtitle_gen: Arc<GenRegistry>,
    /// Managed Cloudflare Tunnel connector (optional, off by default). Supervises a
    /// `cloudflared` child when enabled so a box with no tunnel gets a public HTTPS
    /// endpoint without port-forwarding. See [`crate::services::remote`].
    pub remote: Arc<RemoteAccess>,
    /// The acquisition stack's download manager: embedded torrent engine
    /// lifecycle, grabs ledger, kill-switch gate. The monitor task is spawned
    /// in `main` next to the other reapers. See [`crate::services::downloads`].
    pub downloads: Arc<DownloadManager>,
    /// Managed WireGuard-to-SOCKS5 bridge (wireproxy) for torrent traffic,
    /// the Proton VPN path. See [`crate::services::vpn`].
    pub vpn: Arc<Vpn>,
    /// Runtime-loaded (WASM) modules installed under `<data>/modules`. Behind an
    /// `RwLock` so the admin store can install / uninstall them live. Their
    /// manifests are merged into `GET /api/modules` and their HTTP is proxied at
    /// `/api/plugin/<id>/*`.
    pub wasm: Arc<RwLock<WasmHost>>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: Config, ffprobe_available: bool, db: Pool, settings: Settings) -> SharedState {
        let hls = hls::HlsEngine::new(
            &config.data_dir,
            crate::services::settings::max_transcodes(&settings),
            crate::services::settings::transcode_cache_limit_bytes(&settings),
        );
        let storyboard = Storyboard::new(&config.data_dir);
        // Built before the struct literal moves `config`: the connector locates a
        // server-provided `cloudflared` relative to the data dir.
        let remote = RemoteAccess::new(config.data_dir.clone());
        let downloads = DownloadManager::new(&config.data_dir);
        let vpn = Vpn::new(config.data_dir.clone());
        // Load any runtime-installed WASM modules from disk (best-effort).
        let wasm = Arc::new(RwLock::new(WasmHost::load_all(&config.data_dir.join("modules"))));
        // Seed the process-wide ffmpeg concurrency budget from the setting so the
        // very first background pass already honors it (updated live on write).
        crate::infra::ffmpeg_gate::set_capacity(crate::services::settings::media_workers(&settings));
        // Build the job registry: register the built-ins, then overlay any
        // persisted schedule overrides. The cron loop is spawned in `main`.
        let mut jobs = JobManager::new();
        crate::services::jobs::register_all(&mut jobs);
        jobs.load_schedules(&db);
        // Restore the persisted global pipeline-pause so a box rebooted while held
        // stays held until an admin resumes (visible in the Pipeline console).
        jobs.set_pipeline_paused(settings.get_bool("pipelinePaused", false));
        // Any run left `running` belongs to a previous process that died mid-job;
        // mark it failed so it doesn't show as forever-running in the console.
        let _ = crate::db::reconcile_running_runs(&db);
        // Likewise, reset any pipeline ledger task stranded `running` by that
        // crash back to `pending` so its stage picks it up again.
        crate::services::pipeline::recover_on_boot(&db);
        Arc::new(AppState {
            config,
            ffprobe_available,
            db,
            settings,
            metadata_cache: Arc::new(metadata::Cache::new()),
            events: Bus::new(),
            activity: activity::new(),
            hls,
            storyboard,
            quickconnect: quickconnect::new(),
            playback: Registry::new(),
            metrics: Metrics::new(),
            embedder: embed::default_embedder(),
            search: Arc::new(SearchEngine::new().expect("init search index")),
            vectors: Arc::new(VectorCache::new()),
            jobs: Arc::new(jobs),
            subtitle_gen: Arc::new(GenRegistry::default()),
            remote,
            downloads,
            vpn,
            wasm,
        })
    }
}
