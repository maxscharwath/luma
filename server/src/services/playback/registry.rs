//! The live-session store: the in-memory heartbeat map, its upsert/list/reap
//! lifecycle, and appending ended sessions to the play-history log.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use crate::db::Pool;
use crate::infra::events::{Bus, ServerEvent};
use crate::model::MediaItem;

use super::snapshot::snapshot;

/// A session is considered ended once no ping arrives for this long. Clients
/// heartbeat every ~10s, so this tolerates a couple of missed beats.
const SESSION_TTL: Duration = Duration::from_secs(30);
/// How often the reaper sweeps for stale sessions.
const REAP_INTERVAL: Duration = Duration::from_secs(10);

/// What a client reports on each heartbeat.
pub struct Ping {
    pub session_id: String,
    pub item_id: String,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    /// `playing` | `paused`.
    pub state: String,
    /// `direct` | `transcode`.
    pub mode: String,
    pub player: String,
    pub device: String,
    pub audio: Option<String>,
    pub subtitle: Option<String>,
}

/// A live playback session, serialized for the admin API. Field names feed the
/// dashboard's "En cours de lecture" card directly.
#[derive(Clone, Serialize, TS)]
#[ts(export, rename = "PlaybackSession")]
pub struct Session {
    pub id: String,
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub username: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    pub title: String,
    pub year: Option<u32>,
    pub kind: String,
    #[serde(rename = "showTitle", skip_serializing_if = "Option::is_none")]
    pub show_title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    #[serde(rename = "videoLabel")]
    pub video_label: String,
    #[serde(rename = "audioLabel")]
    pub audio_label: String,
    pub subtitle: String,
    /// Approx stream bitrate in Mb/s (from file size ÷ duration).
    pub bitrate: f64,
    /// `direct` | `transcode`.
    pub mode: String,
    pub player: String,
    pub device: String,
    /// `LAN` | `WAN`.
    pub network: String,
    pub ip: String,
    /// `playing` | `paused`.
    pub state: String,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
    /// Unix-seconds the session started (server clock).
    #[serde(rename = "startedAt")]
    pub started_at: i64,
    /// Internal: last heartbeat (for TTL). Skipped from JSON.
    #[serde(skip)]
    last_seen: Instant,
}

/// How long a terminated session id is remembered so its in-flight heartbeats
/// can't immediately re-register it before the client processes the stop event.
const TERMINATE_GRACE: Duration = Duration::from_secs(60);

/// Shared, cheap-to-clone handle to the live-session map.
#[derive(Clone)]
pub struct Registry {
    inner: Arc<RwLock<HashMap<String, Session>>>,
    /// session id → when it was terminated, so re-pings within the grace window
    /// are rejected instead of recreating the session.
    terminated: Arc<RwLock<HashMap<String, Instant>>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            inner: Arc::new(RwLock::new(HashMap::new())),
            terminated: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Admin-terminate a session: drop it and remember the id for a grace window
    /// so its next heartbeat won't recreate it. Returns the removed session (for
    /// history) if it was live.
    pub fn terminate(&self, session_id: &str) -> Option<Session> {
        self.terminated
            .write()
            .unwrap()
            .insert(session_id.to_string(), Instant::now());
        self.inner.write().unwrap().remove(session_id)
    }

    /// Whether `session_id` was terminated within the grace window (so a ping for
    /// it should be refused, not recreated).
    pub fn is_recently_terminated(&self, session_id: &str) -> bool {
        let mut map = self.terminated.write().unwrap();
        map.retain(|_, at| at.elapsed() < TERMINATE_GRACE);
        map.contains_key(session_id)
    }

    /// Upsert a heartbeat. `snapshot` (title/streams) is built by the caller from
    /// the item on first sight; subsequent pings just refresh position/state.
    pub fn upsert(
        &self,
        ping: Ping,
        user_id: Option<String>,
        username: String,
        ip: String,
        network: String,
        item: Option<&MediaItem>,
    ) -> bool {
        let now = Instant::now();
        let mut map = self.inner.write().unwrap();
        let is_new = !map.contains_key(&ping.session_id);
        let entry = map.entry(ping.session_id.clone()).or_insert_with(|| {
            let snap = item.map(snapshot).unwrap_or_default();
            Session {
                id: ping.session_id.clone(),
                user_id: user_id.clone(),
                username: username.clone(),
                item_id: ping.item_id.clone(),
                title: snap.title,
                year: snap.year,
                kind: snap.kind,
                show_title: snap.show_title,
                season: snap.season,
                episode: snap.episode,
                video_label: snap.video_label,
                audio_label: snap.audio_label,
                subtitle: ping.subtitle.clone().unwrap_or_else(|| "Aucun".into()),
                bitrate: snap.bitrate,
                mode: ping.mode.clone(),
                player: ping.player.clone(),
                device: ping.device.clone(),
                network: network.clone(),
                ip: ip.clone(),
                state: ping.state.clone(),
                position_ms: ping.position_ms,
                duration_ms: ping.duration_ms,
                started_at: unix_now(),
                last_seen: now,
            }
        });
        // Refresh the volatile fields on every beat.
        entry.position_ms = ping.position_ms;
        if ping.duration_ms.is_some() {
            entry.duration_ms = ping.duration_ms;
        }
        entry.state = ping.state;
        entry.mode = ping.mode;
        entry.network = network;
        entry.ip = ip;
        if let Some(a) = ping.audio {
            entry.audio_label = a;
        }
        if let Some(s) = ping.subtitle {
            entry.subtitle = s;
        }
        entry.last_seen = now;
        is_new
    }

    /// Whether a session id is already tracked (so the caller can skip the
    /// per-ping item lookup once a session's snapshot is built).
    pub fn contains(&self, session_id: &str) -> bool {
        self.inner.read().unwrap().contains_key(session_id)
    }

    /// Remove a session explicitly (client signalled stop). Returns it so the
    /// caller can record history.
    pub fn remove(&self, session_id: &str) -> Option<Session> {
        self.inner.write().unwrap().remove(session_id)
    }

    /// Snapshot all live (non-stale) sessions, newest first.
    pub fn list(&self) -> Vec<Session> {
        let mut v: Vec<Session> = self
            .inner
            .read()
            .unwrap()
            .values()
            .filter(|s| s.last_seen.elapsed() < SESSION_TTL)
            .cloned()
            .collect();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    /// Whether a given user currently has a live session (the "online" flag).
    pub fn user_online(&self, user_id: &str) -> bool {
        self.inner
            .read()
            .unwrap()
            .values()
            .any(|s| s.user_id.as_deref() == Some(user_id) && s.last_seen.elapsed() < SESSION_TTL)
    }

    /// Drain stale sessions, returning them for history recording.
    fn drain_stale(&self) -> Vec<Session> {
        let mut map = self.inner.write().unwrap();
        let stale: Vec<String> = map
            .iter()
            .filter(|(_, s)| s.last_seen.elapsed() >= SESSION_TTL)
            .map(|(k, _)| k.clone())
            .collect();
        stale.into_iter().filter_map(|k| map.remove(&k)).collect()
    }

    /// Spawn the background reaper: evict stale sessions and log each to history.
    pub fn spawn_reaper(&self, pool: Pool, events: Bus) {
        let reg = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(REAP_INTERVAL).await;
                let ended = reg.drain_stale();
                if ended.is_empty() {
                    continue;
                }
                for s in &ended {
                    record(&pool, s);
                }
                events.publish(ServerEvent::PlaybackStopped {
                    count: reg.list().len(),
                });
            }
        });
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Append one ended session to the play-history log (best-effort).
pub fn record(pool: &Pool, s: &Session) {
    let ended = unix_now();
    let watched = ((ended - s.started_at).max(0)) * 1000;
    let _ = crate::db::record_play(
        pool,
        s.user_id.as_deref(),
        Some(&s.username),
        Some(&s.item_id),
        &s.kind,
        &s.title,
        None,
        s.started_at,
        ended,
        watched,
    );
}

fn unix_now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
