//! Playback heartbeat + progress endpoints. Clients (web/TV/mobile) `POST
//! /api/playback/ping` every few seconds while a `<video>` is playing so the
//! admin dashboard can show live "En cours de lecture" sessions; `POST
//! /api/playback/stop` ends one cleanly. The `/progress` + `/continue` handlers
//! persist resume positions per user. All require a session (the catalogue is
//! public, but a session belongs to a user).

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::db;
use crate::infra::events::ServerEvent;
use crate::services::playback::{self, Ping};
use crate::services::settings;
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
        (query(&state.db, move |pool| db::get_item(&pool, &id)).await).unwrap_or_default()
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

// ----- progress / resume ------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgressBody {
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
}

/// `PUT /api/progress/:id` (Bearer) `{ positionMs, durationMs }` → 204.
pub async fn save_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
    Json(body): Json<ProgressBody>,
) -> Response {
    let pos = body.position_ms.max(0);
    match query(&state.db, move |pool| db::upsert_progress(&pool, &user.id, &item_id, pos, body.duration_ms))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

/// `DELETE /api/progress/:id` (Bearer) → 204 (finished / removed from Continue).
pub async fn delete_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
) -> Response {
    match query(&state.db, move |pool| db::delete_progress(&pool, &user.id, &item_id)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/progress/:id` (Bearer) → `ProgressEntry | null` for one item, so the
/// player can resume without fetching the whole list.
pub async fn get_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
) -> Response {
    match query(&state.db, move |pool| db::get_progress(&pool, &user.id, &item_id)).await {
        Ok(entry) => Json(entry).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/progress` (Bearer) → `ProgressEntry[]` (all saved positions).
pub async fn list_progress(State(state): State<SharedState>, AuthUser(user): AuthUser) -> Response {
    match query(&state.db, move |pool| db::list_progress(&pool, &user.id)).await {
        Ok(p) => Json(p).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/continue` (Bearer) → `ContinueItem[]` (resumable, newest first).
pub async fn continue_watching(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Response {
    match query(&state.db, move |pool| db::continue_watching(&pool, &user.id)).await {
        Ok(items) => Json(items).into_response(),
        Err(resp) => resp,
    }
}
