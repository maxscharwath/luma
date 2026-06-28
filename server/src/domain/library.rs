//! Library types: a scanned library root and its derived classification.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Library classification, derived from the kinds of items it holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LibraryKind {
    Movies,
    Shows,
    Mixed,
}

/// A scanned library root.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub kind: LibraryKind,
    pub path: String,
    #[serde(rename = "itemCount")]
    pub item_count: usize,
}
