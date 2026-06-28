//! Real-time event bus.
//!
//! A [`tokio::sync::broadcast`] channel fans server events out to every
//! connected WebSocket client (`GET /api/events`). The server publishes when the
//! library changes — a scan starts/finishes, or background TMDB enrichment
//! resolves art for a title — so clients update live instead of needing a
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
    /// The catalog changed wholesale — clients should refetch lists.
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
    /// A playback session started — `count` is the new active-session total.
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
}

/// Cheap-to-clone handle to the broadcast channel.
#[derive(Clone)]
pub struct Bus {
    tx: broadcast::Sender<ServerEvent>,
}

impl Bus {
    pub fn new() -> Self {
        // Capacity bounds how far a slow client can lag before it drops events.
        let (tx, _rx) = broadcast::channel(512);
        Self { tx }
    }

    /// Fan an event out to all subscribers. No-op when there are none.
    pub fn publish(&self, event: ServerEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}
