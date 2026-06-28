//! Live playback-session registry — the data behind the admin dashboard's
//! "En cours de lecture" panel.
//!
//! Direct-play streams are plain range requests with no server-side session, so
//! clients **heartbeat** their playback state to `POST /api/playback/ping`. Each
//! ping (keyed by a client-generated session id) refreshes an in-memory record;
//! records that stop pinging are reaped after [`SESSION_TTL`] and appended to the
//! `play_history` log for the analytics panels. The registry is process-local
//! (cleared on restart), which is exactly right for "what's playing right now".
//!
//! Split into the session store/lifecycle ([`registry`]), the item → display
//! [`snapshot`] derivation, and LAN/WAN [`network`] classification.

mod network;
mod registry;
mod snapshot;

pub use network::*;
pub use registry::*;
