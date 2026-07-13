//! Sonarr/Radarr-style file naming: render a path template against a title's
//! facts. The token vocabulary + resolution lives in [`tokens`]; this module
//! owns the [`NameContext`], the five templates, and path assembly.
//!
//! Supported tokens (unknown ones render empty):
//!
//!   `{Movie Title}` `{Series Title}`             the title
//!   `{Movie CleanTitle}` `{Movie TitleThe}`      cleaned / "Title, The"
//!   `{Movie TitleFirstCharacter}`                first character (folder buckets)
//!   `{Release Year}`                             release year
//!   `{season:00}` `{episode:00}`                 numbers, zero-padded per spec
//!   `{Episode Title}`                            episode title
//!   `{Quality Full}` `{Quality Title}`           Bluray-1080p (+ Proper/Repack)
//!   `{Resolution}` `{MediaInfo VideoCodec}`      individual quality parts
//!   `{MediaInfo VideoBitDepth}` `{... VideoDynamicRange}`
//!   `{MediaInfo AudioCodec}` `{... AudioChannels}`
//!   `{MediaInfo AudioLanguages}` `{... SubtitleLanguages}`
//!   `{Release Group}` `{Edition Tags}`          release group / edition
//!   `{ImdbId}` `{TmdbId}`                         external ids
//!
//! String tokens accept a `:N` / `:-N` byte-truncation spec; MediaInfo language
//! tokens accept a `:EN+DE` include / `-DE` exclude filter. The result is
//! cleaned (collapsed whitespace, dropped empty `()`/`[]`/dangling ` - `) and
//! every path component is sanitized for the filesystem.

use std::path::PathBuf;

use crate::engine::services::settings::Settings;

mod tokens;

/// The facts a template renders against. Populated at import time (from the
/// parsed release name) and at bulk-rename time (from the probed streams +
/// TMDB metadata), so some fields are only present on one of the two paths.
#[derive(Debug, Clone, Default)]
pub struct NameContext {
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    /// `1080p`, `2160p`, ...
    pub resolution: Option<String>,
    /// `x265` / `x264`, ... (video codec label).
    pub codec: Option<String>,
    /// `Bluray`, `WEBDL`, `HDTV`, ...
    pub source: Option<String>,
    /// A `PROPER` / `REPACK` re-release, reflected in `{Quality Full}`.
    pub proper: bool,
    pub repack: bool,
    /// Trailing `-GROUP` from the release name.
    pub release_group: Option<String>,
    /// Best-effort edition label ("Director's Cut", "IMAX", ...).
    pub edition: Option<String>,
    /// External ids from TMDB.
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<u64>,
    // --- MediaInfo (from the probed streams; empty at import) ---
    pub audio_codec: Option<String>,
    /// Channel layout label, e.g. `5.1`.
    pub audio_channels: Option<String>,
    /// Video bit depth, e.g. `10`.
    pub video_bit_depth: Option<u32>,
    /// `HDR` / `DV`, or `None` for SDR.
    pub dynamic_range: Option<String>,
    /// Audio track languages, upper-case 2-letter codes (deduped, in order).
    pub audio_languages: Vec<String>,
    /// Subtitle track languages.
    pub subtitle_languages: Vec<String>,
}

impl NameContext {
    /// `{Quality Full}`: `Source-Resolution` plus a `Proper`/`Repack` suffix.
    fn quality_full(&self) -> String {
        let mut q = self.quality_title();
        if self.proper {
            q.push_str(" Proper");
        } else if self.repack {
            q.push_str(" Repack");
        }
        q.trim().to_string()
    }

    /// `{Quality Title}`: `Source-Resolution`, without the proper/repack tag.
    fn quality_title(&self) -> String {
        match (self.source.as_deref(), self.resolution.as_deref()) {
            (Some(s), Some(r)) => format!("{s}-{r}"),
            (Some(s), None) => s.to_string(),
            (None, Some(r)) => r.to_string(),
            (None, None) => String::new(),
        }
    }
}

/// Case transform applied to every rendered path component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Casing {
    /// Keep the metadata's natural case (no change).
    #[default]
    Default,
    /// `THE MATRIX (1999)`.
    Upper,
    /// `the matrix (1999)`.
    Lower,
}

impl Casing {
    pub fn from_key(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "upper" | "uppercase" => Self::Upper,
            "lower" | "lowercase" => Self::Lower,
            _ => Self::Default,
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Upper => "upper",
            Self::Lower => "lower",
        }
    }

    fn apply(self, s: &str) -> String {
        match self {
            Self::Default => s.to_string(),
            Self::Upper => s.to_uppercase(),
            Self::Lower => s.to_lowercase(),
        }
    }
}

/// The five templates + case transform, resolved from settings (Radarr/Sonarr
/// defaults).
#[derive(Debug, Clone)]
pub struct NamingTemplates {
    pub movie_folder: String,
    pub movie_file: String,
    pub series_folder: String,
    pub season_folder: String,
    pub episode_file: String,
    pub case: Casing,
}

pub const DEFAULT_MOVIE_FOLDER: &str = "{Movie Title} ({Release Year})";
pub const DEFAULT_MOVIE_FILE: &str = "{Movie Title} ({Release Year}) {Quality Full}";
pub const DEFAULT_SERIES_FOLDER: &str = "{Series Title} ({Release Year})";
pub const DEFAULT_SEASON_FOLDER: &str = "Season {season:00}";
pub const DEFAULT_EPISODE_FILE: &str =
    "{Series Title} - S{season:00}E{episode:00} - {Episode Title} {Quality Full}";

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
            case: Casing::from_key(&settings.get_str("namingCase", "default")),
        }
    }

    /// Render one template, apply the case transform (the path builders then
    /// sanitize each component for the filesystem).
    fn styled(&self, template: &str, ctx: &NameContext) -> String {
        self.case.apply(&render(template, ctx))
    }

    /// `<movie folder>/<movie file>.<ext>` (folder omitted if its template is
    /// empty, so files can live at the library root). Every component is
    /// sanitized for the filesystem.
    pub fn movie_rel_path(&self, ctx: &NameContext, ext: &str) -> PathBuf {
        let file = file_component(&self.styled(&self.movie_file, ctx), ext);
        match sanitize(&self.styled(&self.movie_folder, ctx)) {
            folder if folder.is_empty() => PathBuf::from(file),
            folder => PathBuf::from(folder).join(file),
        }
    }

    /// `<series folder>/<season folder>/<episode file>.<ext>`.
    pub fn episode_rel_path(&self, ctx: &NameContext, ext: &str) -> PathBuf {
        let file = file_component(&self.styled(&self.episode_file, ctx), ext);
        let mut p = PathBuf::from(sanitize(&self.styled(&self.series_folder, ctx)));
        let season_folder = sanitize(&self.styled(&self.season_folder, ctx));
        if !season_folder.is_empty() {
            p.push(season_folder);
        }
        p.push(file);
        p
    }
}

/// Sanitized `<name>.<ext>` filename; falls back to the extension alone only if
/// the rendered name is empty (should not happen in practice, title is always
/// present).
fn file_component(rendered: &str, ext: &str) -> String {
    let name = sanitize(rendered);
    if name.is_empty() {
        format!("file.{ext}")
    } else {
        format!("{name}.{ext}")
    }
}

/// Render one template against `ctx`, cleaned (but NOT yet sanitized: the path
/// builders sanitize each component so separators survive rendering).
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
            out.push_str(&tokens::resolve_token(&inner, ctx));
        } else {
            out.push(c);
        }
    }
    cleanup(&out)
}

/// Collapse whitespace, drop empty `()`/`[]` and dangling ` - ` separators left
/// by missing tokens.
fn cleanup(s: &str) -> String {
    let mut r = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Empty parens/brackets from a missing year, language tag, etc.
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
    parsed: &crate::scene::ParsedRelease,
) -> (Option<String>, Option<String>, Option<String>) {
    use crate::scene::{Codec, Res, Source};
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

/// Codec label (`x265`) from a probed video codec name.
pub fn codec_label(codec: Option<&str>) -> Option<String> {
    match codec?.to_ascii_lowercase().as_str() {
        "hevc" | "h265" | "x265" => Some("x265".into()),
        "h264" | "avc" | "x264" => Some("x264".into()),
        "av1" => Some("AV1".into()),
        other => Some(other.to_string()),
    }
}

/// Channel-count -> layout label (`6` -> `5.1`), Radarr-style.
pub fn audio_channels_label(channels: Option<u32>) -> Option<String> {
    Some(
        match channels? {
            0 => return None,
            1 => "1.0",
            2 => "2.0",
            3 => "2.1",
            6 => "5.1",
            7 => "6.1",
            8 => "7.1",
            n => return Some(format!("{n}.0")),
        }
        .to_string(),
    )
}

/// Audio codec label in the spelling scene groups use (`eac3` -> `EAC3`).
pub fn audio_codec_label(codec: Option<&str>) -> Option<String> {
    let c = codec?.to_ascii_lowercase();
    if c.is_empty() {
        return None;
    }
    Some(
        match c.as_str() {
            "aac" => "AAC",
            "ac3" | "ac-3" => "AC3",
            "eac3" | "e-ac-3" => "EAC3",
            "dts" => "DTS",
            "truehd" => "TrueHD",
            "flac" => "FLAC",
            "opus" => "Opus",
            "mp3" => "MP3",
            "vorbis" => "Vorbis",
            other => return Some(other.to_uppercase()),
        }
        .to_string(),
    )
}

/// `HDR` / `DV` label, or `None` for SDR (drives `{MediaInfo VideoDynamicRange}`).
pub fn dynamic_range(hdr: bool, dolby_vision: bool) -> Option<String> {
    if dolby_vision {
        Some("DV".into())
    } else if hdr {
        Some("HDR".into())
    } else {
        None
    }
}

/// Normalize a stream language tag to a 2-letter upper code (`eng` -> `EN`);
/// `None` for undefined/unknown so it drops out of the `[EN+FR]` tag.
pub fn lang_code(lang: &str) -> Option<String> {
    let l = lang.trim().to_ascii_lowercase();
    if l.is_empty() || l == "und" || l == "unknown" || l == "mis" || l == "zxx" {
        return None;
    }
    Some(
        match l.as_str() {
            "eng" | "en" => "EN",
            "fre" | "fra" | "fr" => "FR",
            "ger" | "deu" | "de" => "DE",
            "spa" | "es" => "ES",
            "ita" | "it" => "IT",
            "jpn" | "ja" => "JA",
            "por" | "pt" => "PT",
            "rus" | "ru" => "RU",
            "chi" | "zho" | "zh" => "ZH",
            "kor" | "ko" => "KO",
            "nld" | "dut" | "nl" => "NL",
            other => return Some(other.get(..2).unwrap_or(other).to_uppercase()),
        }
        .to_string(),
    )
}

/// Deduped, order-preserving list of normalized language codes from a stream's
/// raw language tags.
pub fn lang_list<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in raw {
        if let Some(code) = lang_code(tag) {
            if !out.contains(&code) {
                out.push(code);
            }
        }
    }
    out
}

/// Strip filesystem-hostile characters from a rendered path component: the
/// Windows/SMB-reserved set, control characters, and trailing dots/spaces
/// (which Windows and SMB shares silently reject).
pub fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ' '])
        .to_string()
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
            case: Casing::Default,
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
            case: Casing::Default,
        };
        let p = tpl.episode_rel_path(&episode_ctx(), "mkv");
        assert_eq!(
            p.to_str().unwrap(),
            "Breaking Bad (2008)/Season 01/Breaking Bad - S01E02 - Cat's in the Bag... HDTV-720p.mkv"
        );
    }

    #[test]
    fn missing_tokens_clean_up() {
        let ctx = NameContext { title: "Show".into(), season: Some(3), episode: Some(7), ..Default::default() };
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
    fn forbidden_chars_removed_from_filename() {
        // A colon in the title must not survive into the filename component.
        let tpl = NamingTemplates {
            movie_folder: String::new(),
            movie_file: "{Movie Title} ({Release Year})".into(),
            series_folder: String::new(),
            season_folder: String::new(),
            episode_file: String::new(),
            case: Casing::Default,
        };
        let ctx = NameContext { title: "Mission: Impossible".into(), year: Some(1996), ..Default::default() };
        let p = tpl.movie_rel_path(&ctx, "mkv");
        assert_eq!(p.to_str().unwrap(), "Mission Impossible (1996).mkv");
        assert!(!p.to_str().unwrap().contains(':'));
    }

    #[test]
    fn case_transform_applies() {
        let mk = |case: Casing| NamingTemplates {
            movie_folder: String::new(),
            movie_file: "{Movie Title} ({Release Year})".into(),
            series_folder: String::new(),
            season_folder: String::new(),
            episode_file: String::new(),
            case,
        };
        let ctx = NameContext { title: "The Matrix".into(), year: Some(1999), ..Default::default() };
        assert_eq!(mk(Casing::Upper).movie_rel_path(&ctx, "mkv").to_str().unwrap(), "THE MATRIX (1999).mkv");
        assert_eq!(mk(Casing::Lower).movie_rel_path(&ctx, "mkv").to_str().unwrap(), "the matrix (1999).mkv");
        assert_eq!(mk(Casing::Default).movie_rel_path(&ctx, "mkv").to_str().unwrap(), "The Matrix (1999).mkv");
    }

    #[test]
    fn sanitize_strips_reserved_and_trailing() {
        assert_eq!(sanitize("A/B:C*?\"<>|D"), "A B C D");
        assert_eq!(sanitize("Trailing dots..."), "Trailing dots");
        assert_eq!(sanitize("Trailing space "), "Trailing space");
    }
}
