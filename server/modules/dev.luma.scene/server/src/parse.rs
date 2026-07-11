//! Scene/P2P release-name parsing: tokenize on separators, then classify each
//! token (resolution, codec, source, season/episode markers, flags); the title
//! is everything before the first structural marker. No regex crate: the
//! token vocabulary is small and hand-matching keeps the crate dependency-free.

use crate::{Codec, ParsedRelease, Res, Source};

pub fn parse_release_name(name: &str) -> ParsedRelease {
    let (body, group) = split_group(name);
    let tokens: Vec<String> = body
        .split(['.', ' ', '_', '[', ']', '(', ')', '{', '}', ','])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();

    let mut out = ParsedRelease { group, ..ParsedRelease::default() };
    // Index of the first structural token; the title stops there.
    let mut first_marker: Option<usize> = None;
    let mark = |slot: &mut Option<usize>, i: usize| {
        if slot.is_none() || i < slot.unwrap() {
            *slot = Some(i);
        }
    };

    for (i, raw) in tokens.iter().enumerate() {
        let t = raw.to_ascii_lowercase();

        if let Some(res) = parse_resolution(&t) {
            out.resolution = Some(out.resolution.map_or(res, |r| r.max(res)));
            mark(&mut first_marker, i);
            continue;
        }
        if let Some(codec) = parse_codec(&t) {
            out.codec = Some(codec);
            mark(&mut first_marker, i);
            continue;
        }
        if let Some(source) = parse_source(&t, tokens.get(i + 1).map(String::as_str)) {
            // "WEB" alone is weaker than an explicit WEBRip seen elsewhere.
            if out.source.is_none() || source != Source::WebDl {
                out.source = Some(source);
            }
            mark(&mut first_marker, i);
            continue;
        }
        if let Some((season, ep, ep_end, full)) = parse_episode_marker(&t) {
            out.season = out.season.or(Some(season));
            if let Some(e) = ep {
                out.episode = out.episode.or(Some(e));
                out.episode_end = out.episode_end.or(ep_end);
            } else if full {
                out.full_season = true;
            }
            mark(&mut first_marker, i);
            continue;
        }
        match t.as_str() {
            "proper" => out.proper = true,
            "repack" | "rerip" => out.repack = true,
            "hdr" | "hdr10" | "hdr10+" => out.hdr = true,
            "dv" | "dovi" => out.dolby_vision = true,
            "vision" if i > 0 && tokens[i - 1].eq_ignore_ascii_case("dolby") => {
                out.dolby_vision = true;
            }
            "complete" | "integrale" | "intégrale" => out.full_season = true,
            "season" | "saison" => {
                if let Some(n) = tokens.get(i + 1).and_then(|n| n.parse::<u32>().ok()) {
                    if n <= 100 {
                        out.season = out.season.or(Some(n));
                        out.full_season = out.episode.is_none();
                        mark(&mut first_marker, i);
                    }
                }
            }
            _ => {
                if let Some(year) = parse_year(&t) {
                    // Keep the LAST year-looking token ("2001 A Space Odyssey
                    // 1968" must not lock onto 2001), but never one at index 0.
                    if i > 0 {
                        out.year = Some(year);
                        mark(&mut first_marker, i);
                    }
                }
            }
        }
    }

    // A season with episodes is not a full-season pack even if COMPLETE-style
    // words appeared.
    if out.episode.is_some() {
        out.full_season = false;
    }

    let title_end = first_marker.unwrap_or(tokens.len());
    out.title = tokens[..title_end].join(" ");
    out
}

/// Split a trailing `-GROUP` tag off the release name. The group is the text
/// after the LAST hyphen when it looks like a tag (alphanumeric, short, no
/// spaces) and is not itself a known token ("WEB-DL", "HDR10-PLUS"...).
fn split_group(name: &str) -> (String, Option<String>) {
    let trimmed = name.trim();
    if let Some((body, tail)) = trimmed.rsplit_once('-') {
        let tag = tail.trim();
        let lower = tag.to_ascii_lowercase();
        let known = parse_codec(&lower).is_some()
            || parse_source(&lower, None).is_some()
            || parse_resolution(&lower).is_some()
            || matches!(lower.as_str(), "dl" | "rip" | "plus" | "e" | "hd");
        let taggy = !tag.is_empty()
            && tag.len() <= 20
            && tag.chars().all(|c| c.is_ascii_alphanumeric())
            && tag.chars().any(|c| c.is_ascii_alphabetic())
            && !known;
        if taggy {
            return (body.to_string(), Some(tag.to_string()));
        }
    }
    (trimmed.to_string(), None)
}

fn parse_resolution(t: &str) -> Option<Res> {
    match t {
        "2160p" | "4k" | "uhd" => Some(Res::R2160),
        "1080p" | "1080i" => Some(Res::R1080),
        "720p" => Some(Res::R720),
        // "1920x1080" style: dimensions, not an SxE marker (see the guard in
        // parse_episode_marker too).
        _ => match t.split_once('x') {
            Some((w, h)) => {
                let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
                match (w, h) {
                    (3840.., _) | (_, 2000..) => Some(Res::R2160),
                    (1900.., _) | (_, 1000..) => Some(Res::R1080),
                    (1200.., _) | (_, 700..) => Some(Res::R720),
                    _ => None,
                }
            }
            None => None,
        },
    }
}

fn parse_codec(t: &str) -> Option<Codec> {
    match t {
        "x265" | "h265" | "h.265" | "hevc" => Some(Codec::Hevc),
        "x264" | "h264" | "h.264" | "avc" => Some(Codec::H264),
        "av1" => Some(Codec::Av1),
        "xvid" | "divx" => Some(Codec::Xvid),
        _ => None,
    }
}

fn parse_source(t: &str, next: Option<&str>) -> Option<Source> {
    match t {
        "remux" => Some(Source::Remux),
        "bluray" | "blu-ray" | "bdrip" | "brrip" | "bdremux" => Some(Source::BluRay),
        "web-dl" | "webdl" => Some(Source::WebDl),
        "webrip" | "web-rip" => Some(Source::WebRip),
        // Bare "WEB": WEB-DL unless the next token says otherwise ("WEB DL" /
        // "WEB RIP" split across tokens).
        "web" => match next.map(str::to_ascii_lowercase).as_deref() {
            Some("rip") => Some(Source::WebRip),
            _ => Some(Source::WebDl),
        },
        "hdtv" | "pdtv" | "dsr" => Some(Source::Hdtv),
        "cam" | "hdcam" | "camrip" | "ts" | "telesync" | "hdts" | "telecine" | "screener"
        | "dvdscr" | "workprint" => Some(Source::Cam),
        _ => None,
    }
}

/// `s01e02`, `s01e01-e03` / `s01e01e02`, bare `s01`, `1x02` (guarded against
/// `1920x1080`). Returns `(season, episode, episode_end, full_season)`.
fn parse_episode_marker(t: &str) -> Option<(u32, Option<u32>, Option<u32>, bool)> {
    if let Some(rest) = t.strip_prefix('s') {
        // sNN / sNNeMM / sNNeMM-eKK / sNNeMMeKK
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || digits.len() > 2 {
            return None;
        }
        let season: u32 = digits.parse().ok()?;
        let tail = &rest[digits.len()..];
        if tail.is_empty() {
            return Some((season, None, None, true));
        }
        let mut eps: Vec<u32> = Vec::new();
        for part in tail.split(['e', '-']) {
            if part.is_empty() {
                continue;
            }
            if !part.chars().all(|c| c.is_ascii_digit()) || part.len() > 3 {
                return None;
            }
            eps.push(part.parse().ok()?);
        }
        if !tail.starts_with('e') || eps.is_empty() {
            return None;
        }
        let first = eps[0];
        let last = eps.last().copied().filter(|&l| l > first);
        return Some((season, Some(first), last, false));
    }
    // NxMM: small left side only, so 1920x1080 stays a resolution.
    if let Some((s, e)) = t.split_once('x') {
        if !s.is_empty()
            && !e.is_empty()
            && s.chars().all(|c| c.is_ascii_digit())
            && e.chars().all(|c| c.is_ascii_digit())
            && s.len() <= 2
            && (2..=3).contains(&e.len())
        {
            let season: u32 = s.parse().ok()?;
            let episode: u32 = e.parse().ok()?;
            if season <= 50 && episode <= 999 {
                return Some((season, Some(episode), None, false));
            }
        }
    }
    None
}

fn parse_year(t: &str) -> Option<u32> {
    if t.len() != 4 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let y: u32 = t.parse().ok()?;
    (1900..=2035).contains(&y).then_some(y)
}
