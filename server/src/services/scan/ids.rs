//! Stable logical-id derivation (so the same movie/episode collapses to one
//! item across files) and best-effort edition labelling from a filename.

use sha2::{Digest, Sha256};

// ----- logical ids ------------------------------------------------------------

/// Stable movie logical id: same title+year → one item.
pub(super) fn movie_logical_id(lib_id: &str, title: &str, year: Option<u32>) -> String {
    let norm = normalize_title(title);
    let year = year.map(|y| y.to_string()).unwrap_or_default();
    short_hash(&format!("{lib_id}|movie|{norm}|{year}"))
}

/// Stable episode logical id: same show/season/episode → one item.
pub(super) fn episode_logical_id(show_id: &str, season: u32, episode: u32) -> String {
    short_hash(&format!("{show_id}|{season}|{episode}"))
}

fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Stable show id from library + normalised show title.
pub(super) fn show_key(lib_id: &str, show_title: &str) -> String {
    let norm = normalize_title(show_title);
    short_hash(&format!("{lib_id}|show|{norm}"))
}

// ----- edition detection ------------------------------------------------------

/// Best-effort edition label from a filename. Keep it simple: scan for a known
/// set of edition/quality tokens and return the first match (preferring cut
/// labels over resolution/source). `None` when nothing notable is present.
pub(super) fn detect_edition(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    // (needle, label) cut/edition labels first, then source/quality.
    const TABLE: &[(&str, &str)] = &[
        ("director's cut", "Director's Cut"),
        ("directors cut", "Director's Cut"),
        ("director.cut", "Director's Cut"),
        ("extended", "Extended"),
        ("uncut", "Uncut"),
        ("unrated", "Unrated"),
        ("theatrical", "Theatrical"),
        ("remastered", "Remastered"),
        ("imax", "IMAX"),
        ("remux", "Remux"),
        ("2160p", "4K"),
        ("4k", "4K"),
        ("uhd", "4K"),
        ("1080p", "1080p"),
        ("720p", "720p"),
        ("480p", "480p"),
    ];
    TABLE
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, label)| label.to_string())
}

/// `hex(sha256(input))[..16]` stable, short, collision-resistant enough.
pub fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}
