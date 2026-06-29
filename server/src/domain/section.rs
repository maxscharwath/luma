//! A home-screen section: a titled, ranked rail of items produced by the section
//! generator ([`crate::services::sections`]). The client renders these
//! generically — it doesn't know what each section *means*, just how to draw a
//! titled row of cards. Adding/retuning sections is therefore a server-only change.

use serde::Serialize;
use ts_rs::TS;

use crate::domain::media::MediaItem;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Section {
    /// Stable-ish key for list rendering / focus restoration (e.g.
    /// `"themed:heist"`, `"for-you"`).
    pub id: String,
    /// Localized, ready-to-display heading (the server resolves i18n, so clients
    /// stay generic).
    pub title: String,
    /// Optional secondary line, e.g. "Parce que vous avez regardé Mad Max".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The rail's items, already capped and de-duplicated against earlier rows.
    pub items: Vec<MediaItem>,
}
