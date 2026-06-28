//! Playback heartbeat endpoints. Clients (web/TV/mobile) `POST /api/playback/ping`
//! every few seconds while a `<video>` is playing so the admin dashboard can show
//! live "En cours de lecture" sessions; `POST /api/playback/stop` ends one
//! cleanly. Both require a session (the catalogue is public, but a session
//! belongs to a user).

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::handlers::query;
use crate::auth::AuthUser;
use crate::db;
use crate::events::ServerEvent;
use crate::playback::{self, Ping};
use crate::settings;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct PingBody {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs", default)]
    pub duration_ms: Option<i64>,
    /// `playing` | `paused`. Defaults to playing.
    #[serde(default = "default_state")]
    pub state: String,
    /// `direct` | `transcode`. Defaults to direct.
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub player: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub audio: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
}

fn default_state() -> String {
    "playing".into()
}
fn default_mode() -> String {
    "direct".into()
}

/// `POST /api/playback/ping` (Bearer) → 204. Upserts the caller's live session.
pub async fn ping(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PingBody>,
) -> Response {
    // An admin just terminated this session — refuse the heartbeat (410) instead
    // of recreating it. The client treats 410 as "stop now".
    if state.playback.is_recently_terminated(&body.session_id) {
        return StatusCode::GONE.into_response();
    }

    let ip = client_ip(&headers, &addr);
    let network = playback::classify_network(&ip, &settings::local_networks(&state.settings));

    // Build the item snapshot only on the first beat of a session.
    let item = if state.playback.contains(&body.session_id) {
        None
    } else {
        let id = body.item_id.clone();
        match query(&state.db, move |pool| db::get_item(&pool, &id)).await {
            Ok(i) => i,
            Err(_) => None,
        }
    };

    let ping = Ping {
        session_id: body.session_id,
        item_id: body.item_id,
        position_ms: body.position_ms.max(0),
        duration_ms: body.duration_ms,
        state: body.state,
        mode: body.mode,
        player: body.player.unwrap_or_else(|| "LUMA".into()),
        device: body.device.unwrap_or_else(|| "Appareil".into()),
        audio: body.audio,
        subtitle: body.subtitle,
    };

    let is_new = state.playback.upsert(
        ping,
        Some(user.id.clone()),
        user.username.clone(),
        ip,
        network,
        item.as_ref(),
    );

    // Keep the user's last-seen fresh (best-effort).
    let uid = user.id.clone();
    let _ = query(&state.db, move |pool| {
        let _ = db::touch_last_seen(&pool, &uid);
        Ok(())
    })
    .await;

    let count = state.playback.list().len();
    state.events.publish(if is_new {
        ServerEvent::PlaybackStarted { count }
    } else {
        ServerEvent::PlaybackUpdated { count }
    });
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct StopBody {
    #[serde(rename = "sessionId")]
    pub session_id: String,
}

/// `POST /api/playback/stop` (Bearer) → 204. Ends a session and logs it to
/// history immediately (rather than waiting for the reaper).
pub async fn stop(
    State(state): State<SharedState>,
    AuthUser(_user): AuthUser,
    Json(body): Json<StopBody>,
) -> Response {
    if let Some(session) = state.playback.remove(&body.session_id) {
        let _ = query(&state.db, move |pool| {
            playback::record(&pool, &session);
            Ok(())
        })
        .await;
    }
    let count = state.playback.list().len();
    state.events.publish(ServerEvent::PlaybackStopped { count });
    StatusCode::NO_CONTENT.into_response()
}

/// Best client IP: first `X-Forwarded-For` hop (when behind a reverse proxy like
/// the Synology one), else the direct socket peer.
fn client_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let first = first.trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    addr.ip().to_string()
}
