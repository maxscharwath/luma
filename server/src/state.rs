//! Process-wide application state. The library lives in SQLite; this just holds
//! the connection pool, resolved config, and the ffprobe-availability flag.

use std::sync::Arc;

use crate::activity;
use crate::config::Config;
use crate::db::Pool;
use crate::events::Bus;
use crate::metadata;
use crate::metrics::Metrics;
use crate::playback::Registry;
use crate::quickconnect::{self, QuickConnect};
use crate::settings::Settings;
use crate::transcode;

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
    /// On-demand HLS audio-transcode sessions (video copy + AAC) for browsers
    /// that can't decode the source audio codec.
    pub transcode: transcode::Sessions,
    /// In-flight Quick Connect device-pairing requests.
    pub quickconnect: QuickConnect,
    /// Live playback sessions (the dashboard's "En cours de lecture" panel).
    pub playback: Registry,
    /// Rolling CPU / RAM / bandwidth metrics (the dashboard charts).
    pub metrics: Metrics,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: Config, ffprobe_available: bool, db: Pool, settings: Settings) -> SharedState {
        let transcode = transcode::Sessions::new(&config.data_dir);
        Arc::new(AppState {
            config,
            ffprobe_available,
            db,
            settings,
            metadata_cache: Arc::new(metadata::Cache::new()),
            events: Bus::new(),
            activity: activity::new(),
            transcode,
            quickconnect: quickconnect::new(),
            playback: Registry::new(),
            metrics: Metrics::new(),
        })
    }
}
