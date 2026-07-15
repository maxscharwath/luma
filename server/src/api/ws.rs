//! `GET /api/events` a WebSocket that streams live [`ServerEvent`]s to a client
//! (scan progress, library/metadata updates). Clients hold it open and update
//! their UI in place; the connection survives the lifetime of the app.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use crate::infra::events::ServerEvent;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// `GET /api/events` (WebSocket upgrade for the live event bus).
pub fn routes() -> Router<SharedState> {
    Router::new().route("/events", get(events))
}

pub async fn events(State(state): State<SharedState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| pump(socket, state))
}

async fn pump(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.events.subscribe();

    // Greet so the client can confirm the stream is live. Serialization of a
    // fixed struct can't realistically fail, but if it ever did we'd rather drop
    // the connection than send an empty frame.
    let Ok(hello) = serde_json::to_string(&ServerEvent::Hello {
        version: env!("CARGO_PKG_VERSION"),
    }) else {
        return;
    };
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }

    // Periodic ping so a half-open socket (client vanished without a Close frame)
    // is detected as a failed send rather than lingering forever.
    let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    keepalive.reset(); // skip the immediate first tick

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                // Already serialized at publish time; per-subscriber cost is a copy.
                Ok(json) => {
                    if socket.send(Message::Text(json.to_string().into())).await.is_err() {
                        break; // client gone
                    }
                }
                // Slow client fell behind; skip the dropped events and continue.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                // We don't expect client messages; just detect disconnect.
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                _ => {}
            },
            _ = keepalive.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break; // client gone
                }
            }
        }
    }
}
