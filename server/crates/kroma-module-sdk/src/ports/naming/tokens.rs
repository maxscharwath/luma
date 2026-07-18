//! The `{...}` token vocabulary: resolve one token against a [`NameContext`],
//! honoring the optional `:spec` suffix (zero-pad width for numbers, byte
//! truncation for strings, `EN+DE` language filter for MediaInfo tokens).

use super::NameContext;

/// Characters that may decorate a token inside the braces (`{[Quality Full]}`,
/// `{-Release Group}`, `{Edition Tags }`); the decoration is emitted only when
/// the token resolves to a non-empty value, matching Radarr's presets.
const DECO: &[char] = &['[', ']', '(', ')', '-', '_', '.', ' '];

/// Render one `{...}` token (the text between the braces) against `ctx`, peeling
/// any leading/trailing decoration and re-attaching it only when the token has
/// a value.
pub fn resolve_token(inner: &str, ctx: &NameContext) -> String {
    let prefix: String = inner.chars().take_while(|c| DECO.contains(c)).collect();
    let suffix: String =
        inner.chars().rev().take_while(|c| DECO.contains(c)).collect::<Vec<_>>().into_iter().rev().collect();
    // DECO chars are all ASCII, so byte offsets equal char counts here.
    let core = &inner[prefix.len()..inner.len() - suffix.len()];
    let value = resolve_core(core, ctx);
    if value.is_empty() {
        String::new()
    } else {
        format!("{prefix}{value}{suffix}")
    }
}

/// Resolve the bare token (no decoration) to its value.
fn resolve_core(inner: &str, ctx: &NameContext) -> String {
    let (name, spec) = match inner.split_once(':') {
        Some((n, s)) => (n, Some(s)),
        None => (inner, None),
    };
    // Normalize the token name: drop spaces/punctuation, lowercase, so
    // `{Movie Title}`, `{Movie.Title}` and `{movietitle}` are the same token.
    let key: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();

    // Number tokens: an all-zeros spec is a zero-pad width (`00` => 2).
    let pad_width = spec.filter(|s| !s.is_empty() && s.chars().all(|c| c == '0')).map(str::len).unwrap_or(1);
    let pad = |n: u32| if pad_width > 1 { format!("{n:0pad_width$}") } else { n.to_string() };
    match key.as_str() {
        "season" | "seasonnumber" => return ctx.season.map(pad).unwrap_or_default(),
        "episode" | "episodenumber" => return ctx.episode.map(pad).unwrap_or_default(),
        "year" | "releaseyear" => return ctx.year.map(|y| y.to_string()).unwrap_or_default(),
        "tmdbid" => return ctx.tmdb_id.map(|x| x.to_string()).unwrap_or_default(),
        _ => {}
    }

    // MediaInfo language tokens honor the `:EN+DE` include / `-DE` exclude spec.
    match key.as_str() {
        "mediainfoaudiolanguages" => return langs(&ctx.audio_languages, spec, false),
        "mediainfoaudiolanguagesall" => return langs(&ctx.audio_languages, spec, true),
        "mediainfosubtitlelanguages" => return langs(&ctx.subtitle_languages, spec, true),
        _ => {}
    }

    // String tokens (a signed-integer spec truncates them to N bytes).
    let value = match key.as_str() {
        "title" | "movietitle" | "seriestitle" | "titleyear" => ctx.title.clone(),
        "cleantitle" | "moviecleantitle" | "seriescleantitle" => clean_title(&ctx.title),
        "titlethe" | "movietitlethe" | "seriestitlethe" => title_the(&ctx.title),
        "cleantitlethe" | "moviecleantitlethe" | "seriescleantitlethe" => clean_title(&title_the(&ctx.title)),
        "titlefirstcharacter" | "movietitlefirstcharacter" | "seriestitlefirstcharacter" => {
            first_character(&ctx.title)
        }
        "episodetitle" => ctx.episode_title.clone().unwrap_or_default(),
        "quality" | "qualityfull" => ctx.quality_full(),
        "qualitytitle" => ctx.quality_title(),
        "resolution" => ctx.resolution.clone().unwrap_or_default(),
        "codec" | "videocodec" | "mediainfovideocodec" => ctx.codec.clone().unwrap_or_default(),
        "source" => ctx.source.clone().unwrap_or_default(),
        "releasegroup" => ctx.release_group.clone().unwrap_or_default(),
        "edition" | "editiontags" => ctx.edition.clone().unwrap_or_default(),
        "imdbid" => ctx.imdb_id.clone().unwrap_or_default(),
        "mediainfoaudiocodec" => ctx.audio_codec.clone().unwrap_or_default(),
        "mediainfoaudiochannels" => ctx.audio_channels.clone().unwrap_or_default(),
        "mediainfovideobitdepth" => ctx.video_bit_depth.map(|d| d.to_string()).unwrap_or_default(),
        "mediainfovideodynamicrange" | "mediainfovideodynamicrangetype" => {
            ctx.dynamic_range.clone().unwrap_or_default()
        }
        _ => String::new(),
    };

    match spec.and_then(|s| s.parse::<i32>().ok()) {
        Some(n) if n != 0 => truncate(&value, n),
        _ => value,
    }
}

/// Radarr's CleanTitle: drop apostrophes/quotes and turn the punctuation that
/// would clutter a filename into spaces, keeping the words.
fn clean_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        match c {
            '\'' | '"' | '`' | '\u{2019}' | '\u{2018}' => {} // dropped, no gap
            ',' | ':' | ';' | '!' | '?' | '.' | '*' | '|' | '<' | '>' | '/' | '\\' => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "The Matrix" -> "Matrix, The"; titles without a leading article are left
/// unchanged.
fn title_the(title: &str) -> String {
    for article in ["The ", "A ", "An "] {
        if let Some(rest) = title.strip_prefix(article) {
            return format!("{}, {}", rest.trim_start(), article.trim_end());
        }
    }
    title.to_string()
}

/// The first alphanumeric character of the sort title, upper-cased (for
/// `A/`, `B/`, `0/` folder buckets).
fn first_character(title: &str) -> String {
    title_the(title)
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

/// Render a `[EN+FR]` language tag with Radarr's include/exclude filter and the
/// "hide a sole-English audio track" rule (footnote 2 in the token modal).
fn langs(all: &[String], spec: Option<&str>, keep_sole_english: bool) -> String {
    let mut list: Vec<String> = all.to_vec();
    match spec.filter(|s| !s.is_empty()) {
        Some(spec) => {
            let (mut include, mut exclude): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
            for tok in spec.split('+') {
                match tok.strip_prefix('-') {
                    Some(x) if !x.is_empty() => exclude.push(x.to_uppercase()),
                    _ if !tok.is_empty() => include.push(tok.to_uppercase()),
                    _ => {}
                }
            }
            if !include.is_empty() {
                list.retain(|l| include.contains(l));
            }
            if !exclude.is_empty() {
                list.retain(|l| !exclude.contains(l));
            }
        }
        // No spec: hide the tag entirely when the only audio language is English.
        None if !keep_sole_english && list == ["EN"] => return String::new(),
        None => {}
    }
    if list.is_empty() {
        String::new()
    } else {
        format!("[{}]", list.join("+"))
    }
}

/// Truncate `s` to `max_bytes` bytes including a trailing `...` (Radarr's
/// `{Token:30}`). A negative width keeps the END of the string (`{Token:-30}`),
/// with a leading ellipsis. Respects UTF-8 char boundaries. No-op when already
/// within budget.
fn truncate(s: &str, max_bytes: i32) -> String {
    let budget = max_bytes.unsigned_abs() as usize;
    if s.len() <= budget {
        return s.to_string();
    }
    const ELLIPSIS: &str = "...";
    if budget <= ELLIPSIS.len() {
        return ELLIPSIS[..budget].to_string();
    }
    let keep = budget - ELLIPSIS.len();
    if max_bytes >= 0 {
        let end = floor_char_boundary(s, keep);
        format!("{}{ELLIPSIS}", &s[..end])
    } else {
        let start = ceil_char_boundary(s, s.len() - keep);
        format!("{ELLIPSIS}{}", &s[start..])
    }
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::super::render;
    use super::*;

    fn ctx() -> NameContext {
        NameContext {
            title: "The Matrix".into(),
            year: Some(1999),
            resolution: Some("2160p".into()),
            codec: Some("x265".into()),
            source: Some("Bluray".into()),
            release_group: Some("EVOLVE".into()),
            edition: Some("IMAX".into()),
            imdb_id: Some("tt0133093".into()),
            tmdb_id: Some(603),
            audio_codec: Some("EAC3".into()),
            audio_channels: Some("5.1".into()),
            video_bit_depth: Some(10),
            dynamic_range: Some("DV".into()),
            audio_languages: vec!["EN".into(), "FR".into()],
            subtitle_languages: vec!["FR".into()],
            ..Default::default()
        }
    }

    #[test]
    fn clean_title_and_the() {
        assert_eq!(clean_title("Mission: Impossible"), "Mission Impossible");
        assert_eq!(clean_title("Marvel's Avengers"), "Marvels Avengers");
        assert_eq!(title_the("The Matrix"), "Matrix, The");
        assert_eq!(title_the("A Serious Man"), "Serious Man, A");
        assert_eq!(title_the("Inception"), "Inception");
        assert_eq!(first_character("The Matrix"), "M");
    }

    #[test]
    fn radarr_tokens_render() {
        let c = ctx();
        assert_eq!(render("{Movie CleanTitle}", &c), "The Matrix");
        assert_eq!(render("{Movie TitleThe}", &c), "Matrix, The");
        assert_eq!(render("{ImdbId}", &c), "tt0133093");
        assert_eq!(render("{TmdbId}", &c), "603");
        assert_eq!(render("[{MediaInfo VideoCodec}]", &c), "[x265]");
        assert_eq!(render("{MediaInfo AudioCodec} {MediaInfo AudioChannels}", &c), "EAC3 5.1");
        assert_eq!(render("{MediaInfo VideoDynamicRange}", &c), "DV");
        assert_eq!(render("{MediaInfo VideoBitDepth}bit", &c), "10bit");
        assert_eq!(render("{MediaInfo AudioLanguages}", &c), "[EN+FR]");
        assert_eq!(render("{MediaInfo SubtitleLanguages}", &c), "[FR]");
        assert_eq!(render("[{Edition Tags}]", &c), "[IMAX]");
        // In-brace decoration is emitted only when the token has a value.
        assert_eq!(render("{Movie Title}{-Release Group}", &c), "The Matrix-EVOLVE");
        assert_eq!(render("{Movie Title}{[Quality Full]}", &c), "The Matrix[Bluray-2160p]");
        let no_group = NameContext { release_group: None, ..c };
        assert_eq!(render("{Movie Title}{-Release Group}", &no_group), "The Matrix");
    }

    #[test]
    fn quality_full_with_proper() {
        let c = NameContext {
            source: Some("Bluray".into()),
            resolution: Some("1080p".into()),
            proper: true,
            ..Default::default()
        };
        assert_eq!(render("{Quality Full}", &c), "Bluray-1080p Proper");
        assert_eq!(render("{Quality Title}", &c), "Bluray-1080p");
    }

    #[test]
    fn language_filter_and_sole_english() {
        let both = ["EN".to_string(), "FR".to_string()];
        assert_eq!(langs(&both, Some("FR"), false), "[FR]");
        assert_eq!(langs(&both, Some("-EN"), false), "[FR]");
        // Sole English is hidden for AudioLanguages but kept for AudioLanguagesAll.
        assert_eq!(langs(&["EN".to_string()], None, false), "");
        assert_eq!(langs(&["EN".to_string()], None, true), "[EN]");
    }

    #[test]
    fn truncation_keeps_boundaries() {
        assert_eq!(truncate("A Very Long Movie Title Here", 13), "A Very Lon...");
        assert_eq!(truncate("A Very Long Movie Title Here", -13), "...Title Here");
        assert_eq!(truncate("Short", 30), "Short");
        // Accented chars must not be split mid-byte.
        let out = truncate("Amélie Poulain Deluxe", 8);
        assert!(out.is_char_boundary(out.len()) && out.ends_with("..."));
    }

    #[test]
    fn truncation_tiny_budget_is_partial_ellipsis() {
        // Budget at or below the ellipsis length yields a (possibly partial) ellipsis.
        assert_eq!(truncate("abcdef", 3), "...");
        assert_eq!(truncate("abcdef", 2), "..");
        assert_eq!(truncate("abcdef", 1), ".");
        // A negative tail keep respects UTF-8 boundaries too.
        let tail = truncate("héllo wörld tail", -7);
        assert!(tail.starts_with("...") && tail.is_char_boundary(tail.len()));
    }

    #[test]
    fn title_the_handles_all_articles() {
        assert_eq!(title_the("An Officer and a Gentleman"), "Officer and a Gentleman, An");
        // No leading article: unchanged.
        assert_eq!(title_the("Blade Runner"), "Blade Runner");
        // First-character bucket uses the sort title (article moved to the end).
        assert_eq!(first_character("A Bug's Life"), "B");
        assert_eq!(first_character("2001: A Space Odyssey"), "2");
        assert_eq!(first_character(""), "");
    }

    #[test]
    fn clean_title_drops_curly_quotes_without_gaps() {
        assert_eq!(clean_title("It\u{2019}s Complicated"), "Its Complicated");
        assert_eq!(clean_title("Who? What! Why."), "Who What Why");
    }

    #[test]
    fn number_token_without_pad_and_unknown_token() {
        let c = ctx();
        // No pad spec => plain number.
        assert_eq!(render("S{season}", &NameContext { season: Some(4), ..Default::default() }), "S4");
        // Unknown token resolves to empty (and its decoration is dropped).
        assert_eq!(render("{Totally Unknown}", &c), "");
        assert_eq!(render("[{Totally Unknown}]", &c), "");
        // Year with no value renders empty.
        assert_eq!(render("{Release Year}", &NameContext::default()), "");
    }

    #[test]
    fn language_tokens_all_and_subtitle_spec() {
        // AudioLanguagesAll keeps a sole-English track (unlike AudioLanguages).
        let sole_en = NameContext { audio_languages: vec!["EN".into()], ..Default::default() };
        assert_eq!(render("{MediaInfo AudioLanguages}", &sole_en), "");
        assert_eq!(render("{MediaInfo AudioLanguagesAll}", &sole_en), "[EN]");
        // Subtitle languages honor an include+exclude spec.
        let subs = NameContext {
            subtitle_languages: vec!["EN".into(), "FR".into(), "DE".into()],
            ..Default::default()
        };
        assert_eq!(render("{MediaInfo SubtitleLanguages:EN+FR}", &subs), "[EN+FR]");
        assert_eq!(render("{MediaInfo SubtitleLanguages:-EN}", &subs), "[FR+DE]");
    }

    #[test]
    fn langs_include_and_exclude_together() {
        let all = ["EN".to_string(), "FR".to_string(), "DE".to_string()];
        // Include narrows, exclude then removes from the narrowed set.
        assert_eq!(langs(&all, Some("EN+FR+-FR"), true), "[EN]");
        // An empty filtered list yields no tag at all.
        assert_eq!(langs(&all, Some("JA"), true), "");
        // Empty spec is treated as no spec.
        assert_eq!(langs(&all, Some(""), true), "[EN+FR+DE]");
    }

    #[test]
    fn string_token_truncation_spec() {
        let c = NameContext { title: "A Very Long Movie Title Here".into(), ..Default::default() };
        assert_eq!(render("{Movie Title:13}", &c), "A Very Lon...");
        // A zero spec is not a truncation (and not a pad): value unchanged.
        assert_eq!(render("{Movie Title:0}", &c), "A Very Long Movie Title Here");
    }
}
