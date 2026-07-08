//! Admin-console types: the members table row, per-user watch stats and the
//! raw history/library aggregates that back the dashboard.

use serde::Serialize;

use crate::domain::accounts::Permission;
use crate::domain::media::Kind;

/// One account as surfaced to the admin "Membres & partage" table. Unlike
/// [`User`] this carries the email, a derived role, last-activity and a live
/// `online` flag (set at request time from the playback registry).
#[derive(Debug, Clone, Serialize)]
pub struct AdminUser {
    pub id: String,
    pub email: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub permissions: Vec<Permission>,
    /// Derived display role: "Propriétaire" | "Membre" | "Restreint".
    pub role: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastSeen", skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// Whether the user is currently streaming (filled from the playback registry).
    pub online: bool,
}

/// Aggregated per-user watch stats over a window (the dashboard "Top des
/// utilisateurs" cards).
#[derive(Debug, Clone, Serialize)]
pub struct TopUser {
    pub username: String,
    pub plays: i64,
    #[serde(rename = "watchedMs")]
    pub watched_ms: i64,
    #[serde(rename = "filmsMs")]
    pub films_ms: i64,
    #[serde(rename = "tvMs")]
    pub tv_ms: i64,
}

/// One raw play-history record (used to bucket the weekly "Historique de
/// lecture" chart server-side).
#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub ended_at: i64,
    pub kind: Kind,
    pub watched_ms: i64,
}

/// Per-library aggregate (item count + total bytes on disk) for the storage and
/// libraries admin pages.
#[derive(Debug, Clone)]
pub struct LibraryStat {
    pub id: String,
    pub item_count: i64,
    pub total_bytes: i64,
}
