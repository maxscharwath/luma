//! Live playback-session registry — the data behind the admin dashboard's
//! "En cours de lecture" panel.
//!
//! Direct-play streams are plain range requests with no server-side session, so
//! clients **heartbeat** their playback state to `POST /api/playback/ping`. Each
//! ping (keyed by a client-generated session id) refreshes an in-memory record;
//! records that stop pinging are reaped after [`SESSION_TTL`] and appended to the
//! `play_history` log for the analytics panels. The registry is process-local
//! (cleared on restart), which is exactly right for "what's playing right now".

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use crate::db::Pool;
use crate::events::{Bus, ServerEvent};
use crate::model::MediaItem;

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

/// Derived, display-ready snapshot of an item for a session card.
#[derive(Default)]
struct Snapshot {
    title: String,
    year: Option<u32>,
    kind: String,
    show_title: Option<String>,
    season: Option<u32>,
    episode: Option<u32>,
    video_label: String,
    audio_label: String,
    bitrate: f64,
}

fn snapshot(item: &MediaItem) -> Snapshot {
    let video_label = item
        .video
        .as_ref()
        .map(|v| {
            let res = resolution_label(v.width);
            let codec = video_codec_label(&v.codec);
            if v.hdr {
                format!("{res} HDR · {codec}")
            } else {
                format!("{res} · {codec}")
            }
        })
        .unwrap_or_else(|| "—".into());

    let audio_label = item
        .audio
        .as_ref()
        .map(|a| {
            let ch = channels_label(a.channels);
            let codec = a.codec.to_uppercase();
            format!("{ch} · {codec}")
        })
        .unwrap_or_else(|| "—".into());

    Snapshot {
        title: item.title.clone(),
        year: item.year,
        kind: kind_str(&item.kind).to_string(),
        show_title: item.show_title.clone(),
        season: item.season,
        episode: item.episode,
        video_label,
        audio_label,
        bitrate: bitrate_mbps(item),
    }
}

fn kind_str(k: &crate::model::Kind) -> &'static str {
    match k {
        crate::model::Kind::Movie => "movie",
        crate::model::Kind::Episode => "episode",
        crate::model::Kind::Video => "video",
    }
}

fn resolution_label(width: Option<u32>) -> &'static str {
    match width.unwrap_or(0) {
        w if w >= 3000 => "4K",
        w if w >= 1900 => "1080p",
        w if w >= 1200 => "720p",
        w if w > 0 => "SD",
        _ => "—",
    }
}

fn video_codec_label(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "hevc" | "h265" => "H.265".into(),
        "h264" | "avc" => "H.264".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        other => other.to_uppercase(),
    }
}

fn channels_label(ch: Option<u32>) -> &'static str {
    match ch.unwrap_or(0) {
        8 => "7.1",
        7 => "6.1",
        6 => "5.1",
        2 => "Stéréo",
        1 => "Mono",
        _ => "Audio",
    }
}

/// Approx stream bitrate in Mb/s from the representative file size ÷ duration.
fn bitrate_mbps(item: &MediaItem) -> f64 {
    let dur_s = item.duration_ms.unwrap_or(0) as f64 / 1000.0;
    if dur_s <= 0.0 {
        return 0.0;
    }
    let size = item
        .default_file_id
        .as_ref()
        .and_then(|id| item.files.iter().find(|f| &f.id == id))
        .or_else(|| item.files.first())
        .and_then(|f| f.size)
        .unwrap_or(0) as f64;
    if size <= 0.0 {
        return 0.0;
    }
    let mbps = size * 8.0 / dur_s / 1_000_000.0;
    (mbps * 10.0).round() / 10.0
}

fn unix_now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// Classify a client IP as `LAN` or `WAN` against the configured local networks
/// (CIDR `a.b.c.d/n` or a bare prefix like `192.168.`). Loopback is always LAN.
pub fn classify_network(ip: &str, local_nets: &[String]) -> String {
    let Ok(addr) = ip.parse::<IpAddr>() else {
        return "WAN".into();
    };
    if addr.is_loopback() {
        return "LAN".into();
    }
    // RFC1918 / link-local are LAN regardless of config.
    if is_private(&addr) {
        return "LAN".into();
    }
    for net in local_nets {
        if cidr_contains(net, &addr) {
            return "LAN".into();
        }
    }
    "WAN".into()
}

fn is_private(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// Minimal IPv4 CIDR / prefix match. Accepts `a.b.c.d/n` and bare `a.b.` prefixes.
fn cidr_contains(net: &str, addr: &IpAddr) -> bool {
    let IpAddr::V4(ip) = addr else { return false };
    let ip = u32::from(*ip);
    if let Some((base, bits)) = net.split_once('/') {
        let Ok(base_ip) = base.trim().parse::<std::net::Ipv4Addr>() else {
            return false;
        };
        let Ok(bits) = bits.trim().parse::<u32>() else {
            return false;
        };
        if bits == 0 {
            return true;
        }
        if bits > 32 {
            return false;
        }
        let mask = u32::MAX << (32 - bits);
        (u32::from(base_ip) & mask) == (ip & mask)
    } else {
        // Bare prefix string match on the dotted form.
        std::net::Ipv4Addr::from(ip).to_string().starts_with(net.trim())
    }
}
