//! Playback progress types: a saved position and a "continue watching" entry.

use serde::{Deserialize, Serialize};

use crate::media::MediaItem;

/// One row of a user's playback progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// A "continue watching" entry: the resumable item plus where to resume from.
#[derive(Debug, Clone, Serialize)]
pub struct ContinueItem {
    pub item: MediaItem,
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// The episode to play to continue a show (`GET /api/shows/:id/up-next`): the
/// episode plus whether it has a saved resume position (drives the "Reprendre"
/// vs "Lecture" button label).
#[derive(Debug, Clone, Serialize)]
pub struct UpNext {
    pub item: MediaItem,
    pub resume: bool,
}
