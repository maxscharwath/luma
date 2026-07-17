//! Process-wide, in-memory result cache for resolved TMDB metadata.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::metadata::Metadata;

/// Process-wide, in-memory result cache keyed by (target, title, year, lang).
/// A cached `None` means "looked up, no match" so misses aren't retried every
/// request. Lives in [`crate::state::AppState`].
#[derive(Default)]
pub struct Cache(Mutex<HashMap<String, Option<Metadata>>>);

impl Cache {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
    pub(super) fn get(&self, key: &str) -> Option<Option<Metadata>> {
        self.0.lock().ok()?.get(key).cloned()
    }
    pub(super) fn put(&self, key: String, value: Option<Metadata>) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(key, value);
        }
    }
    /// Drop every cached lookup so subsequent resolves re-hit TMDB. Used by the
    /// admin "reset metadata" action.
    pub fn clear(&self) {
        if let Ok(mut map) = self.0.lock() {
            map.clear();
        }
    }
}
