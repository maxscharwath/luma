//! Plex-style filename / folder parsing.
//!
//! Decides whether a file is a movie or a TV episode, and pulls out the show
//! name, season, episode (incl. multi-episode files), titles and year — using
//! the same cues Plex/Jellyfin rely on:
//!   * `S01E02`, `s1e2`, `S01E02-E03`, `1x02` season/episode markers
//!   * the top-level folder under a library root as the *show* identity
//!     (`Library/Show Name/Season 01/Show - S01E02.mkv`)
//!   * `Movie Title (2017)` movie folders / filenames
//!   * release-junk stripping for clean titles (resolution, source, codec, group)
//!
//! This is pure domain logic: filename → parsed identity, no I/O.

mod marker;
mod title;

use std::path::Path;

use marker::{find_marker, is_season_folder};
use title::clean_episode_title;
pub use title::{clean_title, parse_year};

/// Outcome of parsing one media file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Movie {
        title: String,
        year: Option<u32>,
    },
    Episode {
        show_title: String,
        show_year: Option<u32>,
        season: u32,
        episode: u32,
        /// Last episode for multi-episode files (`S01E02-E03`).
        episode_end: Option<u32>,
        episode_title: Option<String>,
    },
}

/// Parse a media file located at `path`, relative to its library `root`.
pub fn parse(root: &Path, path: &Path) -> Parsed {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");

    // Directory components between the library root and the file.
    let dirs: Vec<String> = path
        .parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .filter_map(|c| c.as_os_str().to_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    if let Some(m) = find_marker(stem) {
        // Show identity: the top-level folder under the library root, else the
        // text before the marker in the filename (flat layout).
        let (show_title, show_year) = match dirs.first() {
            Some(folder) if !is_season_folder(folder) => (clean_title(folder), parse_year(folder)),
            _ => {
                let before = &stem[..m.start];
                (clean_title(before), parse_year(before))
            }
        };
        let show_title = if show_title.is_empty() {
            clean_title(stem)
        } else {
            show_title
        };

        let after = stem.get(m.end..).unwrap_or("");
        let episode_title = {
            let t = clean_episode_title(after);
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        };

        Parsed::Episode {
            show_title,
            show_year,
            season: m.season,
            episode: m.episode,
            episode_end: m.episode_end,
            episode_title,
        }
    } else {
        // Movie: prefer the filename when it carries a year, else fall back to a
        // `Title (Year)` parent folder (the canonical Plex movie layout).
        let parent = dirs.last().map(String::as_str);
        let (title, year) = if let Some(y) = parse_year(stem) {
            (clean_title(stem), Some(y))
        } else if let Some((p, y)) = parent.and_then(|p| parse_year(p).map(|y| (p, y))) {
            (clean_title(p), Some(y))
        } else {
            (clean_title(stem), None)
        };
        Parsed::Movie { title, year }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn p(root: &str, path: &str) -> Parsed {
        parse(Path::new(root), Path::new(path))
    }

    #[test]
    fn movie_in_year_folder() {
        assert_eq!(
            p("/m", "/m/Blade Runner 2049 (2017)/Blade Runner 2049 (2017) 2160p BluRay x265.mkv"),
            Parsed::Movie { title: "Blade Runner 2049".into(), year: Some(2017) }
        );
    }

    #[test]
    fn movie_flat_dotted() {
        assert_eq!(
            p("/m", "/m/The.Matrix.1999.1080p.BluRay.x264-GROUP.mp4"),
            Parsed::Movie { title: "The Matrix".into(), year: Some(1999) }
        );
    }

    #[test]
    fn movie_title_from_folder_when_file_is_generic() {
        assert_eq!(
            p("/m", "/m/Inception (2010)/movie.mkv"),
            Parsed::Movie { title: "Inception".into(), year: Some(2010) }
        );
    }

    #[test]
    fn episode_show_season_layout() {
        assert_eq!(
            p("/tv", "/tv/The Office (2005)/Season 02/The Office - S02E01 - The Dundies.mkv"),
            Parsed::Episode {
                show_title: "The Office".into(),
                show_year: Some(2005),
                season: 2,
                episode: 1,
                episode_end: None,
                episode_title: Some("The Dundies".into()),
            }
        );
    }

    #[test]
    fn episode_multi() {
        match p("/tv", "/tv/Show/Season 1/Show.S01E02-E03.mkv") {
            Parsed::Episode { season, episode, episode_end, .. } => {
                assert_eq!((season, episode, episode_end), (1, 2, Some(3)));
            }
            other => panic!("expected episode, got {other:?}"),
        }
    }

    #[test]
    fn episode_nxnn_flat() {
        match p("/tv", "/tv/Firefly - 1x02 - The Train Job.mkv") {
            Parsed::Episode { show_title, season, episode, .. } => {
                assert_eq!((show_title.as_str(), season, episode), ("Firefly", 1, 2));
            }
            other => panic!("expected episode, got {other:?}"),
        }
    }

    #[test]
    fn resolution_not_mistaken_for_episode() {
        // 1920x1080 must NOT parse as season 1920 / episode 1080.
        assert!(matches!(
            p("/m", "/m/Heat 1995 1920x1080.mkv"),
            Parsed::Movie { .. }
        ));
    }

    #[test]
    fn dictionary_words_survive_in_titles() {
        // "french"/"uncut" are release tags AND real words; the authoritative
        // `(YYYY)` boundary must win so the title is not clipped at them.
        assert_eq!(
            clean_title("The French Dispatch (2021) [EN+FR] Bluray-1080p"),
            "The French Dispatch"
        );
        assert_eq!(clean_title("Uncut Gems (2019) WEBDL-1080p"), "Uncut Gems");
        // Bare-year layout, still must keep the dictionary word.
        assert_eq!(
            clean_title("The French Connection 1971 1080p BluRay"),
            "The French Connection"
        );
    }

    #[test]
    fn french_dub_tag_stripped_when_adjacent_to_junk() {
        // No year: FRENCH sits right before a hard marker → both drop.
        assert_eq!(
            clean_title("Le Fabuleux Destin FRENCH DVDRip XviD"),
            "Le Fabuleux Destin"
        );
    }

    #[test]
    fn leading_year_recovers_title() {
        // A year at the start must not wipe the whole title to "".
        assert_eq!(
            clean_title("2018 - LaserGame - Indian Forest"),
            "LaserGame - Indian Forest"
        );
    }

    #[test]
    fn the_french_dispatch_parses_with_year() {
        assert_eq!(
            p(
                "/m",
                "/m/The French Dispatch (2021)/The French Dispatch (2021) [EN+FR] Bluray-1080p.mkv"
            ),
            Parsed::Movie { title: "The French Dispatch".into(), year: Some(2021) }
        );
    }
}
