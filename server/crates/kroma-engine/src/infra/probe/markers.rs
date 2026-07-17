//! Derive intro / credits [`MarkerKind`] segments from embedded chapter titles.
//!
//! Many TV/anime releases ship chapters like "Intro", "Opening", "Recap",
//! "Ending", "End Credits". We keyword-match the titles and keep at most one
//! intro and one credits range, with a light position sanity check (intro in the
//! first half, credits in the second) so a stray title can't mislabel.

use super::Chapter;
use crate::model::MarkerKind;

/// Intro/credits ranges (kind, start_ms, end_ms) derived from `chapters`.
/// `duration_ms` (when known) gates the position sanity check.
pub fn markers_from_chapters(
    chapters: &[Chapter],
    duration_ms: Option<u64>,
) -> Vec<(MarkerKind, u64, u64)> {
    let mut out: Vec<(MarkerKind, u64, u64)> = Vec::new();
    for c in chapters {
        let Some(title) = c.title.as_deref() else {
            continue;
        };
        let Some(kind) = classify(title) else {
            continue;
        };
        if !plausible(kind, c, duration_ms) {
            continue;
        }
        // Keep the first chapter of each kind.
        if !out.iter().any(|(k, _, _)| *k == kind) {
            out.push((kind, c.start_ms, c.end_ms));
        }
    }
    out
}

/// Classify a chapter title. Intro is checked first so "opening credits" counts
/// as the opening, not the end credits.
fn classify(title: &str) -> Option<MarkerKind> {
    let t = title.trim().to_lowercase();
    const INTRO: &[&str] = &[
        "intro",
        "opening",
        "main title",
        "title sequence",
        "recap",
        "previously",
        "générique d'ouverture",
        "générique début",
    ];
    const CREDITS: &[&str] = &[
        "credit",
        "ending",
        "end title",
        "outro",
        "closing",
        "générique de fin",
        "générique fin",
    ];
    if INTRO.iter().any(|k| t.contains(k)) || t == "op" {
        return Some(MarkerKind::Intro);
    }
    if CREDITS.iter().any(|k| t.contains(k)) || t == "ed" {
        return Some(MarkerKind::Credits);
    }
    None
}

/// Sanity check: with a known duration, an intro should sit in the first half and
/// credits in the second. Without a duration we trust the title.
fn plausible(kind: MarkerKind, c: &Chapter, duration_ms: Option<u64>) -> bool {
    let Some(dur) = duration_ms.filter(|d| *d > 0) else {
        return true;
    };
    match kind {
        // Intro in the first half (or first 6 min, whichever is larger).
        MarkerKind::Intro => c.start_ms <= (dur / 2).max(360_000),
        // Credits in the last 40%.
        MarkerKind::Credits => c.start_ms >= dur * 6 / 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chap(start_s: u64, end_s: u64, title: &str) -> Chapter {
        Chapter { start_ms: start_s * 1000, end_ms: end_s * 1000, title: Some(title.into()) }
    }

    #[test]
    fn classifies_intro_and_credits() {
        // 24-minute episode (1_440_000 ms).
        let chapters = vec![
            chap(0, 90, "Intro"),
            chap(90, 1300, "Episode"),
            chap(1300, 1440, "End Credits"),
        ];
        let m = markers_from_chapters(&chapters, Some(1_440_000));
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], (MarkerKind::Intro, 0, 90_000));
        assert_eq!(m[1], (MarkerKind::Credits, 1_300_000, 1_440_000));
    }

    #[test]
    fn opening_credits_is_intro_not_credits() {
        let m = markers_from_chapters(&[chap(0, 80, "Opening Credits")], Some(1_400_000));
        assert_eq!(m, vec![(MarkerKind::Intro, 0, 80_000)]);
    }

    #[test]
    fn ignores_misplaced_titles() {
        // A "Credits" chapter at minute 1 is rejected by the position check.
        let m = markers_from_chapters(&[chap(60, 120, "Credits")], Some(1_400_000));
        assert!(m.is_empty());
    }

    #[test]
    fn no_duration_trusts_title() {
        let m = markers_from_chapters(&[chap(0, 60, "Recap")], None);
        assert_eq!(m, vec![(MarkerKind::Intro, 0, 60_000)]);
    }

    #[test]
    fn untitled_or_unknown_chapters_skipped() {
        let chapters = vec![
            Chapter { start_ms: 0, end_ms: 1000, title: None },
            chap(0, 60, "Chapter 1"),
        ];
        assert!(markers_from_chapters(&chapters, Some(600_000)).is_empty());
    }
}
