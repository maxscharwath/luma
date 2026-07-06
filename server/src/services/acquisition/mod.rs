//! Acquisition orchestration: the quality profile from settings, per-indexer
//! capability caching, and the search pipelines (interactive here via
//! [`search`]; the automatic wanted-list pass rides the downloads milestone).

pub mod auto;
pub mod import;
pub mod search;

use std::collections::HashMap;
use std::sync::Mutex;

use luma_release::{Profile, Res};
use luma_torznab::{Caps, IndexerEndpoint};

use crate::db::IndexerRow;
use crate::services::jobs::now_ms;
use crate::state::SharedState;

const GB: u64 = 1_073_741_824;

/// Build the decision engine's profile from the admin settings.
pub fn profile_from_settings(state: &SharedState) -> Profile {
    let s = &state.settings;
    let resolution = match s.get_str("acqResolution", "1080p").as_str() {
        "720p" => Res::R720,
        "2160p" => Res::R2160,
        _ => Res::R1080,
    };
    let list = |key: &str| -> Vec<String> {
        s.get_str(key, "")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect()
    };
    Profile {
        resolution,
        prefer_hevc: s.get_bool("acqPreferHevc", true),
        min_seeders: s.get_i64("acqMinSeeders", 2).max(0) as u32,
        max_size_bytes_movie: (s.get_i64("acqMaxSizeGbMovie", 15).max(0) as u64) * GB,
        max_size_bytes_episode: (s.get_i64("acqMaxSizeGbEpisode", 3).max(0) as u64) * GB,
        required_keywords: list("acqRequiredKeywords"),
        forbidden_keywords: list("acqForbiddenKeywords"),
    }
}

pub fn endpoint_of(row: &IndexerRow) -> IndexerEndpoint {
    IndexerEndpoint {
        url: row.url.clone(),
        api_key: row.api_key.clone(),
        categories: row.categories.clone(),
    }
}

/// Process-wide `t=caps` cache: capabilities are static per indexer, and every
/// search would otherwise pay an extra round-trip per indexer. Keyed by
/// indexer id + url (so re-pointing an indexer refreshes).
static CAPS_CACHE: Mutex<Option<HashMap<String, Caps>>> = Mutex::new(None);

pub fn indexer_caps(state: &SharedState, row: &IndexerRow) -> anyhow::Result<Caps> {
    let key = format!("{}|{}", row.id, row.url);
    if let Some(caps) = CAPS_CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(caps);
    }
    let result = luma_torznab::caps(&endpoint_of(row));
    match &result {
        Ok(_) => {
            let _ = crate::db::note_indexer_result(&state.db, &row.id, true, None, now_ms());
        }
        Err(e) => {
            let _ =
                crate::db::note_indexer_result(&state.db, &row.id, false, Some(&format!("{e:#}")), now_ms());
        }
    }
    let caps = result?;
    CAPS_CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(key, caps.clone());
    Ok(caps)
}

/// Drop a cached capability entry (config changed / manual test forces a
/// fresh probe).
pub fn invalidate_caps(indexer_id: &str) {
    if let Some(map) = CAPS_CACHE.lock().unwrap().as_mut() {
        map.retain(|k, _| !k.starts_with(&format!("{indexer_id}|")));
    }
}
