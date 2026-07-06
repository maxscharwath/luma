//! Real-time event bus.
//!
//! A [`tokio::sync::broadcast`] channel fans server events out to every
//! connected WebSocket client (`GET /api/events`). The server publishes when the
//! library changes a scan starts/finishes, or background TMDB enrichment
//! resolves art for a title so clients update live instead of needing a
//! refresh/relaunch. Publishing is cheap and non-blocking; with no subscribers
//! it's a no-op.

use serde::Serialize;
use tokio::sync::broadcast;

/// A server-pushed event. Serialized as `{ "type": "...", ...fields }`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    /// Sent once on connect so the client knows the stream is live.
    #[serde(rename = "hello")]
    Hello { version: &'static str },
    #[serde(rename = "scan.started")]
    ScanStarted,
    #[serde(rename = "scan.completed")]
    ScanCompleted {
        items: usize,
        shows: usize,
        libraries: usize,
    },
    /// The catalog changed wholesale clients should refetch lists.
    #[serde(rename = "library.updated")]
    LibraryUpdated,
    /// One movie/episode gained metadata (e.g. poster resolved).
    #[serde(rename = "item.updated")]
    ItemUpdated { id: String },
    /// One show gained metadata.
    #[serde(rename = "show.updated")]
    ShowUpdated { id: String },
    /// Background enrichment progress.
    #[serde(rename = "enrich.progress")]
    EnrichProgress { done: usize, total: usize },
    #[serde(rename = "enrich.completed")]
    EnrichCompleted { resolved: usize, total: usize },
    /// Background per-file probing (phase 2) progress.
    #[serde(rename = "probe.progress")]
    ProbeProgress { done: usize, total: usize },
    #[serde(rename = "probe.completed")]
    ProbeCompleted { total: usize },
    /// A playback session started `count` is the new active-session total.
    #[serde(rename = "playback.started")]
    PlaybackStarted { count: usize },
    /// A live playback session updated (state/position changed).
    #[serde(rename = "playback.updated")]
    PlaybackUpdated { count: usize },
    /// One or more playback sessions ended (stopped or reaped).
    #[serde(rename = "playback.stopped")]
    PlaybackStopped { count: usize },
    /// An admin terminated a playback session: the owning client must stop and
    /// show `message` (empty → the client shows a localized default).
    #[serde(rename = "playback.terminate")]
    PlaybackTerminate {
        #[serde(rename = "sessionId")]
        session_id: String,
        message: String,
    },
    /// Server settings changed via the admin console.
    #[serde(rename = "settings.updated")]
    SettingsUpdated,
    /// A background job run started.
    #[serde(rename = "job.started")]
    JobStarted {
        key: String,
        #[serde(rename = "runId")]
        run_id: String,
    },
    /// A running job reported progress (`total == 0` → indeterminate).
    #[serde(rename = "job.progress")]
    JobProgress {
        key: String,
        #[serde(rename = "runId")]
        run_id: String,
        done: usize,
        total: usize,
    },
    /// A running job appended a log line.
    #[serde(rename = "job.log")]
    JobLog {
        #[serde(rename = "runId")]
        run_id: String,
        level: &'static str,
        message: String,
    },
    /// A job run finished (`status`: success | failed | cancelled).
    #[serde(rename = "job.finished")]
    JobFinished {
        key: String,
        #[serde(rename = "runId")]
        run_id: String,
        status: String,
    },
    /// Per-element pipeline health changed (a stage drained a batch). Throttled
    /// and carries only the aggregate per-stage counts, so the admin Pipeline
    /// dashboard updates live without polling the ledger.
    #[serde(rename = "pipeline.stats")]
    PipelineStats {
        stages: Vec<crate::model::StageStat>,
    },
    /// A media request changed state (created / approved / denied / became
    /// available...). Low-frequency: clients refetch their request lists on it.
    #[serde(rename = "request.updated")]
    RequestUpdated { id: String, status: String },
    /// Live download progress (~one frame per active torrent per monitor
    /// tick). High-frequency: the admin shell SKIPS it for its tick (like
    /// job.progress); pages wanting smooth bars consume it on their own
    /// stream.
    #[serde(rename = "download.progress")]
    DownloadProgress {
        id: String,
        #[serde(rename = "requestId")]
        request_id: Option<String>,
        progress: f64,
        #[serde(rename = "downBps")]
        down_bps: u64,
        #[serde(rename = "upBps")]
        up_bps: u64,
        peers: u32,
        #[serde(rename = "peersSeen")]
        peers_seen: u32,
        state: String,
    },
    /// A download finished (import follows).
    #[serde(rename = "download.completed")]
    DownloadCompleted { id: String, title: String },
    /// The VPN kill-switch state changed.
    #[serde(rename = "vpn.status")]
    VpnStatus {
        connected: bool,
        #[serde(rename = "exitIp")]
        exit_ip: Option<String>,
        paused: bool,
    },
}

/// Cheap-to-clone handle to the broadcast channel. The channel carries the
/// event pre-serialized as JSON (`Arc<str>`): one `serde_json::to_string` at
/// publish time instead of one per subscriber per message, and a zero-cost
/// no-op (not even serialization) while nobody is connected.
#[derive(Clone)]
pub struct Bus {
    tx: broadcast::Sender<std::sync::Arc<str>>,
}

impl Bus {
    pub fn new() -> Self {
        // Capacity bounds how far a slow client can lag before it drops events.
        let (tx, _rx) = broadcast::channel(512);
        Self { tx }
    }

    /// Fan an event out to all subscribers. No-op when there are none.
    pub fn publish(&self, event: ServerEvent) {
        if self.tx.receiver_count() == 0 {
            return;
        }
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.tx.send(json.into());
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<std::sync::Arc<str>> {
        self.tx.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}
