//! Sonarr/Radarr-style file naming: render a path template against a title's
//! facts. Supports the common tokens so an admin can paste the format they
//! already use on their NAS:
//!
//!   `{Title}` `{Movie Title}` `{Series Title}`   the title
//!   `{Year}` `{Release Year}`                     release year
//!   `{season:00}` `{episode:00}`                  numbers, zero-padded per spec
//!   `{Episode Title}`                             episode title
//!   `{Quality}` `{Quality Full}`                  Source-Resolution (Bluray-1080p)
//!   `{Resolution}` `{Codec}` `{Source}`           individual quality parts
//!
//! Unknown tokens render empty; the result is cleaned (collapsed whitespace,
//! dropped empty `()` / dangling ` - `) and sanitized for the filesystem.

use std::path::PathBuf;

use crate::services::settings::Settings;

/// The facts a template renders against.
#[derive(Debug, Clone, Default)]
pub struct NameContext {
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    /// `1080p`, `2160p`, ...
    pub resolution: Option<String>,
    /// `x265` / `HEVC`, `x264`, ...
    pub codec: Option<String>,
    /// `Bluray`, `WEB-DL`, `HDTV`, ...
    pub source: Option<String>,
}

impl NameContext {
    /// Sonarr's `{Quality Full}`: `Source-Resolution` when both are known
    /// (`Bluray-1080p`), else whichever is present.
    fn quality_full(&self) -> String {
        match (self.source.as_deref(), self.resolution.as_deref()) {
            (Some(s), Some(r)) => format!("{s}-{r}"),
            (Some(s), None) => s.to_string(),
            (None, Some(r)) => r.to_string(),
            (None, None) => String::new(),
        }
    }
}

/// The five templates, resolved from settings (with Radarr/Sonarr defaults).
#[derive(Debug, Clone)]
pub struct NamingTemplates {
    pub movie_folder: String,
    pub movie_file: String,
    pub series_folder: String,
    pub season_folder: String,
    pub episode_file: String,
}

pub const DEFAULT_MOVIE_FOLDER: &str = "{Title} ({Year})";
pub const DEFAULT_MOVIE_FILE: &str = "{Title} ({Year}) {Quality Full}";
pub const DEFAULT_SERIES_FOLDER: &str = "{Title} ({Year})";
pub const DEFAULT_SEASON_FOLDER: &str = "Season {season:00}";
pub const DEFAULT_EPISODE_FILE: &str = "{Title} - S{season:00}E{episode:00} - {Episode Title} {Quality Full}";

impl NamingTemplates {
    pub fn from_settings(settings: &Settings) -> Self {
        let g = |key: &str, default: &str| {
            let v = settings.get_str(key, default);
            if v.trim().is_empty() {
                default.to_string()
            } else {
                v
            }
        };
        Self {
            movie_folder: g("namingMovieFolder", DEFAULT_MOVIE_FOLDER),
            movie_file: g("namingMovieFile", DEFAULT_MOVIE_FILE),
            series_folder: g("namingSeriesFolder", DEFAULT_SERIES_FOLDER),
            season_folder: g("namingSeasonFolder", DEFAULT_SEASON_FOLDER),
            episode_file: g("namingEpisodeFile", DEFAULT_EPISODE_FILE),
        }
    }

    /// `<movie folder>/<movie file>.<ext>` (folder omitted if its template is
    /// empty, so files can live at the library root).
    pub fn movie_rel_path(&self, ctx: &NameContext, ext: &str) -> PathBuf {
        let file = format!("{}.{ext}", render(&self.movie_file, ctx));
        match sanitize(&render(&self.movie_folder, ctx)) {
            folder if folder.is_empty() => PathBuf::from(file),
            folder => PathBuf::from(folder).join(file),
        }
    }

    /// `<series folder>/<season folder>/<episode file>.<ext>`.
    pub fn episode_rel_path(&self, ctx: &NameContext, ext: &str) -> PathBuf {
        let file = format!("{}.{ext}", render(&self.episode_file, ctx));
        let mut p = PathBuf::from(sanitize(&render(&self.series_folder, ctx)));
        let season_folder = sanitize(&render(&self.season_folder, ctx));
        if !season_folder.is_empty() {
            p.push(season_folder);
        }
        p.push(file);
        p
    }
}

/// Render one template against `ctx`, cleaned + sanitized.
pub fn render(template: &str, ctx: &NameContext) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut inner = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                inner.push(c2);
            }
            out.push_str(&resolve_token(&inner, ctx));
        } else {
            out.push(c);
        }
    }
    cleanup(&out)
}

fn resolve_token(inner: &str, ctx: &NameContext) -> String {
    let (name, spec) = match inner.split_once(':') {
        Some((n, s)) => (n, Some(s)),
        None => (inner, None),
    };
    // Normalize the token name: drop spaces/punctuation, lowercase, so
    // `{Series Title}` and `{seriestitle}` are the same token.
    let key: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    // Zero-pad width from the spec (`00` => 2, `000` => 3).
    let width = spec.map(|s| s.chars().filter(|&c| c == '0').count().max(1)).unwrap_or(1);
    let pad = |n: u32| if width > 1 { format!("{n:0width$}") } else { n.to_string() };

    match key.as_str() {
        "title" | "movietitle" | "seriestitle" | "cleantitle" | "moviecleantitle"
        | "seriescleantitle" | "titleyear" => ctx.title.clone(),
        "year" | "releaseyear" => ctx.year.map(|y| y.to_string()).unwrap_or_default(),
        "season" | "seasonnumber" => ctx.season.map(pad).unwrap_or_default(),
        "episode" | "episodenumber" => ctx.episode.map(pad).unwrap_or_default(),
        "episodetitle" => ctx.episode_title.clone().unwrap_or_default(),
        "quality" | "qualityfull" => ctx.quality_full(),
        "qualitytitle" | "resolution" => ctx.resolution.clone().unwrap_or_default(),
        "codec" | "videocodec" | "mediainfovideocodec" => ctx.codec.clone().unwrap_or_default(),
        "source" => ctx.source.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

/// Collapse whitespace, drop empty `()` and dangling ` - ` separators left by
/// missing tokens.
fn cleanup(s: &str) -> String {
    // Collapse runs of whitespace.
    let mut r = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Empty parens/brackets from a missing year etc.
    for empty in ["( )", "()", "[ ]", "[]", "- -"] {
        while r.contains(empty) {
            r = r.replace(empty, "-");
        }
    }
    r = r.split_whitespace().collect::<Vec<_>>().join(" ");
    // Drop empty segments around the ` - ` separator (missing episode title...).
    let joined = r.split(" - ").map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" - ");
    joined.trim().trim_matches('-').trim().to_string()
}

/// Quality strings (resolution, codec, source) from a parsed release name, in
/// the spellings Sonarr/Radarr use (`1080p`, `x265`, `Bluray`).
pub fn quality_from_parsed(
    parsed: &luma_release::ParsedRelease,
) -> (Option<String>, Option<String>, Option<String>) {
    use luma_release::{Codec, Res, Source};
    let res = parsed.resolution.map(|r| match r {
        Res::R720 => "720p",
        Res::R1080 => "1080p",
        Res::R2160 => "2160p",
    });
    let codec = parsed.codec.map(|c| match c {
        Codec::Hevc => "x265",
        Codec::H264 => "x264",
        Codec::Av1 => "AV1",
        Codec::Xvid => "Xvid",
    });
    let source = parsed.source.map(|s| match s {
        Source::Remux => "Remux",
        Source::BluRay => "Bluray",
        Source::WebDl => "WEBDL",
        Source::WebRip => "WEBRip",
        Source::Hdtv => "HDTV",
        Source::Cam => "Cam",
    });
    (res.map(str::to_string), codec.map(str::to_string), source.map(str::to_string))
}

/// Resolution label (`1080p`) from a probed pixel width, for library files
/// whose quality comes from ffprobe rather than a release name.
pub fn resolution_from_width(width: Option<i64>) -> Option<String> {
    match width? {
        w if w >= 3400 => Some("2160p".into()),
        w if w >= 1700 => Some("1080p".into()),
        w if w >= 1200 => Some("720p".into()),
        w if w >= 640 => Some("480p".into()),
        _ => None,
    }
}

/// Codec label (`x265`) from a probed codec name.
pub fn codec_label(codec: Option<&str>) -> Option<String> {
    match codec?.to_ascii_lowercase().as_str() {
        "hevc" | "h265" | "x265" => Some("x265".into()),
        "h264" | "avc" | "x264" => Some("x264".into()),
        "av1" => Some("AV1".into()),
        other => Some(other.to_string()),
    }
}

/// Strip filesystem-hostile characters from a rendered path component.
pub fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c => c,
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie_ctx() -> NameContext {
        NameContext {
            title: "The Matrix".into(),
            year: Some(1999),
            resolution: Some("1080p".into()),
            source: Some("Bluray".into()),
            ..Default::default()
        }
    }

    fn episode_ctx() -> NameContext {
        NameContext {
            title: "Breaking Bad".into(),
            year: Some(2008),
            season: Some(1),
            episode: Some(2),
            episode_title: Some("Cat's in the Bag...".into()),
            resolution: Some("720p".into()),
            source: Some("HDTV".into()),
            ..Default::default()
        }
    }

    #[test]
    fn radarr_default_movie_format() {
        let tpl = NamingTemplates {
            movie_folder: DEFAULT_MOVIE_FOLDER.into(),
            movie_file: DEFAULT_MOVIE_FILE.into(),
            series_folder: String::new(),
            season_folder: String::new(),
            episode_file: String::new(),
        };
        let p = tpl.movie_rel_path(&movie_ctx(), "mkv");
        assert_eq!(p.to_str().unwrap(), "The Matrix (1999)/The Matrix (1999) Bluray-1080p.mkv");
    }

    #[test]
    fn sonarr_default_episode_format() {
        let tpl = NamingTemplates {
            movie_folder: String::new(),
            movie_file: String::new(),
            series_folder: DEFAULT_SERIES_FOLDER.into(),
            season_folder: DEFAULT_SEASON_FOLDER.into(),
            episode_file: DEFAULT_EPISODE_FILE.into(),
        };
        let p = tpl.episode_rel_path(&episode_ctx(), "mkv");
        assert_eq!(
            p.to_str().unwrap(),
            "Breaking Bad (2008)/Season 01/Breaking Bad - S01E02 - Cat's in the Bag... HDTV-720p.mkv"
        );
    }

    #[test]
    fn missing_tokens_clean_up() {
        // No year, no quality, no episode title.
        let ctx = NameContext {
            title: "Show".into(),
            season: Some(3),
            episode: Some(7),
            ..Default::default()
        };
        assert_eq!(render("{Title} ({Year})", &ctx), "Show");
        assert_eq!(
            render("{Title} - S{season:00}E{episode:00} - {Episode Title} {Quality Full}", &ctx),
            "Show - S03E07"
        );
    }

    #[test]
    fn padding_respects_spec() {
        let ctx = NameContext { season: Some(1), episode: Some(5), ..Default::default() };
        assert_eq!(render("S{season:00}E{episode:00}", &ctx), "S01E05");
        assert_eq!(render("S{season:000}E{episode}", &ctx), "S001E5");
        assert_eq!(render("{season}x{episode:00}", &ctx), "1x05");
    }

    #[test]
    fn token_aliases_and_sanitize() {
        let ctx = NameContext {
            title: "Face/Off".into(),
            year: Some(1997),
            resolution: Some("2160p".into()),
            ..Default::default()
        };
        // Radarr token spellings + a title with a slash.
        let out = render("{Movie Title} ({Release Year}) {Resolution}", &ctx);
        assert_eq!(out, "Face/Off (1997) 2160p");
        assert_eq!(sanitize(&out), "Face Off (1997) 2160p");
    }
}
