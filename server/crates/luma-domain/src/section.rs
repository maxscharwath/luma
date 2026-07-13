//! A home-screen section: a titled, ranked rail of entries produced by the
//! section generator (`crate::services::sections`). The client renders these
//! generically it doesn't know what each section *means*, just how to draw a
//! titled row of cards. Adding/retuning sections is therefore a server-only change.

use serde::Serialize;

use crate::media::{MediaItem, Show};

#[derive(Debug, Clone, Serialize)]
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
    /// The rail's entries movies *or* shows already capped and de-duplicated
    /// against earlier rows. A `type`-tagged union so the client switches on it
    /// (mirrors `SearchHit`): a movie carries a [`MediaItem`], a show a [`Show`].
    pub items: Vec<SectionItem>,
}

/// One rail entry: a movie/video (a [`MediaItem`]) or a whole show (a [`Show`]).
/// Both are embedded + ranked by the recommender, so a row can mix them.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SectionItem {
    Movie { item: Box<MediaItem> },
    Show { show: Box<Show> },
}

impl SectionItem {
    /// The entry's stable id (item or show id) used for cross-row de-dup.
    pub fn id(&self) -> &str {
        match self {
            SectionItem::Movie { item } => &item.id,
            SectionItem::Show { show } => &show.id,
        }
    }
}
