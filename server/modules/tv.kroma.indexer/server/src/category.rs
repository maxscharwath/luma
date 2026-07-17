//! Category translation between a tracker's own ids and Newznab/Torznab
//! category numbers (the ids the rest of KROMA's acquisition stack speaks).

use crate::definition::Definition;

/// Map a Cardigann category *name* (`Movies/HD`, `TV/Anime`, `Audio`) to its
/// Newznab id. Sub-categories are exact; a bare parent name maps to the parent
/// bucket; anything unknown falls into `Other` (8000).
pub fn newznab_id(name: &str) -> u32 {
    match name.trim() {
        // Movies (2000)
        "Movies" => 2000,
        "Movies/Foreign" => 2010,
        "Movies/Other" => 2020,
        "Movies/SD" => 2030,
        "Movies/HD" => 2040,
        "Movies/UHD" | "Movies/4K" => 2045,
        "Movies/BluRay" => 2050,
        "Movies/3D" => 2060,
        "Movies/DVD" => 2070,
        "Movies/WEB-DL" => 2080,
        // TV (5000)
        "TV" => 5000,
        "TV/WEB-DL" => 5010,
        "TV/Foreign" => 5020,
        "TV/SD" => 5030,
        "TV/HD" => 5040,
        "TV/UHD" => 5045,
        "TV/Other" => 5050,
        "TV/Sport" => 5060,
        "TV/Anime" => 5070,
        "TV/Documentary" => 5080,
        // Everything else, coarse parent buckets.
        n => match n.split('/').next().unwrap_or("") {
            "Console" => 1000,
            "Audio" => 3000,
            "PC" => 4000,
            "XXX" => 6000,
            "Books" => 7000,
            "Other" => 8000,
            _ => 8000,
        },
    }
}

/// The tracker category ids to request for a set of wanted Newznab ids. A
/// wanted parent (e.g. `2000` Movies) pulls in every tracker category mapped to
/// a sub-bucket of it (`2xxx`); exact ids match too.
pub fn tracker_ids_for(def: &Definition, wanted: &[u32]) -> Vec<String> {
    let buckets: Vec<u32> = wanted.iter().map(|id| id / 1000).collect();
    let mut out = Vec::new();
    for m in &def.caps.categorymappings {
        let nid = newznab_id(&m.cat);
        if wanted.contains(&nid) || buckets.contains(&(nid / 1000)) {
            out.push(m.id.clone());
        }
    }
    // Several Newznab sub-buckets can map to the same tracker id; `dedup` only
    // drops CONSECUTIVE repeats, so sort first to avoid `cat=1,1`.
    out.sort();
    out.dedup();
    out
}

/// Map a tracker's own category id back to a Newznab id, via the definition's
/// mappings.
pub fn newznab_for_tracker_id(def: &Definition, tracker_id: &str) -> Option<u32> {
    def.caps
        .categorymappings
        .iter()
        .find(|m| m.id == tracker_id)
        .map(|m| newznab_id(&m.cat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_newznab() {
        assert_eq!(newznab_id("Movies/HD"), 2040);
        assert_eq!(newznab_id("TV/Anime"), 5070);
        assert_eq!(newznab_id("Movies"), 2000);
        assert_eq!(newznab_id("Audio/Lossless"), 3000);
        assert_eq!(newznab_id("Something/Weird"), 8000);
    }
}
