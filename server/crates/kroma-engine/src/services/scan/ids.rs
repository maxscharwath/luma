//! Stable logical-id derivation (so the same movie/episode collapses to one
//! item across files) and best-effort edition labelling from a filename.

// ----- logical ids ------------------------------------------------------------

/// Stable movie logical id: same title+year → one item.
pub fn movie_logical_id(lib_id: &str, title: &str, year: Option<u32>) -> String {
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
/// Re-exported from kroma-primitives.
pub use kroma_primitives::short_hash;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_logical_id_normalizes_and_is_field_sensitive() {
        let a = movie_logical_id("lib1", "The Matrix", Some(1999));
        // Case + inner whitespace are normalized, so the same movie collapses.
        assert_eq!(a, movie_logical_id("lib1", "the   MATRIX", Some(1999)));
        // Year, title and library all participate in the identity.
        assert_ne!(a, movie_logical_id("lib1", "The Matrix", Some(2003)));
        assert_ne!(a, movie_logical_id("lib2", "The Matrix", Some(1999)));
        assert_ne!(a, movie_logical_id("lib1", "The Matrix", None));
        // short_hash yields a stable 16-hex-char id.
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn episode_logical_id_keys_on_show_season_episode() {
        let a = episode_logical_id("show1", 1, 2);
        assert_eq!(a, episode_logical_id("show1", 1, 2));
        assert_ne!(a, episode_logical_id("show1", 1, 3));
        assert_ne!(a, episode_logical_id("show1", 2, 2));
        assert_ne!(a, episode_logical_id("show2", 1, 2));
    }

    #[test]
    fn show_key_normalizes_title_and_scopes_by_library() {
        assert_eq!(show_key("lib1", "Breaking Bad"), show_key("lib1", "breaking   bad"));
        assert_ne!(show_key("lib1", "Breaking Bad"), show_key("lib2", "Breaking Bad"));
    }

    #[test]
    fn normalize_title_lowercases_and_collapses_whitespace() {
        assert_eq!(normalize_title("  The   Matrix  "), "the matrix");
        assert_eq!(normalize_title("HELLO"), "hello");
        assert_eq!(normalize_title("a\tb\nc"), "a b c");
        assert_eq!(normalize_title(""), "");
    }

    #[test]
    fn detect_edition_prefers_cut_labels_then_quality() {
        // Cut/edition labels win when both a cut label and a quality token appear.
        assert_eq!(
            detect_edition("Aliens Extended Edition 2160p.mkv").as_deref(),
            Some("Extended")
        );
        assert_eq!(
            detect_edition("Blade Runner Director's Cut 1080p.mkv").as_deref(),
            Some("Director's Cut")
        );
        // Quality tokens map to their canonical labels.
        assert_eq!(detect_edition("Film 2160p BluRay.mkv").as_deref(), Some("4K"));
        assert_eq!(detect_edition("Film UHD.mkv").as_deref(), Some("4K"));
        assert_eq!(detect_edition("Film 1080p.mkv").as_deref(), Some("1080p"));
        assert_eq!(detect_edition("Show 720p WEB.mkv").as_deref(), Some("720p"));
        assert_eq!(detect_edition("Movie IMAX.mkv").as_deref(), Some("IMAX"));
        assert_eq!(detect_edition("Movie REMUX.mkv").as_deref(), Some("Remux"));
        assert_eq!(detect_edition("Movie Unrated.mkv").as_deref(), Some("Unrated"));
        // Nothing notable -> None.
        assert_eq!(detect_edition("Plain Movie.mkv"), None);
    }
}
