//! Title cleaning and the release-junk tokenizer: turn a raw filename/folder
//! fragment into a clean movie/show/episode title and pull out the year.

/// "Hard" release tokens (lowercase): unambiguous scene/source/codec markers.
/// Nobody titles a film `BluRay` or `x265`, so the first one reliably ends the
/// real title.
const HARD_TOKENS: &[&str] = &[
    "4k", "uhd", "bluray", "blu", "brrip", "bdrip", "bdremux", "webrip", "webdl", "hdtv", "sdtv",
    "pdtv", "dvdrip", "dvdscr", "dvd", "remux", "hdrip", "x264", "x265", "h264", "h265", "hevc",
    "avc", "xvid", "divx", "av1", "mpeg2", "vc1", "aac", "ac3", "eac3", "dts", "truehd", "atmos",
    "ddp", "dd5", "flac", "opus", "hdr", "hdr10", "hdr10plus", "dv", "dovi", "sdr", "10bit", "8bit",
    "vostfr",
];

/// "Soft" tokens: real dictionary words that also appear as release tags
/// (`FRENCH` dub, `EXTENDED`/`UNCUT` cut, `IMAX`…). They end the title **only**
/// when they sit directly inside the trailing release run (adjacent to a hard
/// marker), so legitimate titles like "The French Dispatch" and "Uncut Gems"
/// survive intact.
const SOFT_TOKENS: &[&str] = &[
    "french", "truefrench", "subfrench", "vff", "vof", "vfq", "multi", "extended", "unrated",
    "uncut", "imax", "proper", "repack", "remastered", "remaster", "theatrical", "integrale",
];

/// Clean a movie/show title: drop a trailing `(year)` and any release metadata,
/// normalise separators, collapse whitespace.
///
/// A parenthesised `(YYYY)` is treated as the authoritative year cut, so a title
/// that legitimately contains a number "Blade Runner 2049 (2017)" keeps the
/// number and loses only the real year.
pub fn clean_title(raw: &str) -> String {
    // normalize_separators preserves byte length (ASCII punctuation → space), so
    // a paren index found in `raw` is valid in `spaced`.
    let spaced = normalize_separators(raw);

    // A parenthesised `(YYYY)` is the authoritative title boundary: cut there and
    // ignore dictionary-word release tags that precede it. This keeps "The French
    // Dispatch (2021)" whole and lets "Blade Runner 2049 (2017)" keep its number.
    if let Some((i, _)) = paren_year(raw) {
        return finalize(&spaced[..i]);
    }

    // No parenthesised year: the title ends at the earliest of a bare 4-digit
    // year or the start of the trailing release run.
    let cut = [find_year_index(&spaced), release_cut_index(&spaced)]
        .into_iter()
        .flatten()
        .min();
    let title = match cut {
        Some(i) => finalize(&spaced[..i]),
        None => finalize(&spaced),
    };

    // A *leading* year (home video "2018 - LaserGame - Indian Forest") would
    // truncate the title to nothing; recover the text that follows it instead.
    if title.is_empty() {
        if let Some(i) = find_year_index(&spaced) {
            let after = finalize(&spaced[i + 4..]);
            if !after.is_empty() {
                return clean_title(&after);
            }
        }
    }
    title
}

/// A parenthesised year `(YYYY)`; returns the index of the `(` and the year.
fn paren_year(raw: &str) -> Option<(usize, u32)> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i + 6 <= b.len() {
        if b[i] == b'(' && b[i + 5] == b')' && b[i + 1..i + 5].iter().all(u8::is_ascii_digit) {
            // Digits are verified ASCII above, so fold them directly no UTF-8
            // decode / parse / unreachable fallback.
            let y = (b[i + 1] - b'0') as u32 * 1000
                + (b[i + 2] - b'0') as u32 * 100
                + (b[i + 3] - b'0') as u32 * 10
                + (b[i + 4] - b'0') as u32;
            if (1900..=2099).contains(&y) {
                return Some((i, y));
            }
        }
        i += 1;
    }
    None
}

/// Clean an episode title (text after the marker). No year-cut episode names
/// can legitimately contain numbers but release junk is still stripped.
pub(super) fn clean_episode_title(raw: &str) -> String {
    let spaced = normalize_separators(raw);
    let end = release_cut_index(&spaced).unwrap_or(spaced.len());
    finalize(&spaced[..end])
}

fn normalize_separators(raw: &str) -> String {
    raw.replace(['.', '_'], " ")
        .replace(['[', ']', '{', '}', '(', ')'], " ")
}

fn finalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .trim_matches('-')
        .trim()
        .to_string()
}

/// Byte index where the trailing release run begins, or `None`.
///
/// A hard marker (`1080p`, `BluRay`, `x265`, …) opens the run; soft dictionary
/// words (`FRENCH`, `EXTENDED`, `UNCUT`, …) are absorbed only when they sit
/// directly before it. So "Movie FRENCH 1080p" drops both words, while "The
/// French Dispatch" and "Uncut Gems" (no adjacent hard marker) keep theirs.
fn release_cut_index(s: &str) -> Option<usize> {
    let mut off = 0usize;
    let words: Vec<(usize, &str)> = s
        .split(' ')
        .map(|w| {
            let at = off;
            off += w.len() + 1; // +1 for the space we split on
            (at, w)
        })
        .collect();

    let hard = words.iter().position(|(_, w)| is_hard_word(w))?;
    let mut start = hard;
    while start > 0 && is_soft_word(words[start - 1].1) {
        start -= 1;
    }
    Some(words[start].0)
}

/// Strip surrounding punctuation and keep the part before a `-` (so `BluRay-1080p`
/// and `AAC-trailer` reduce to their leading token), lowercased.
fn token_head(word: &str) -> String {
    let w = word
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_ascii_lowercase();
    w.split('-').next().unwrap_or(&w).to_string()
}

/// A resolution token like `720p` / `1080p` / `2160p` / `1080i`.
fn is_resolution(head: &str) -> bool {
    head.strip_suffix(|c| c == 'p' || c == 'i')
        .map(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false)
}

fn is_hard_word(word: &str) -> bool {
    let head = token_head(word);
    !head.is_empty() && (HARD_TOKENS.contains(&head.as_str()) || is_resolution(&head))
}

fn is_soft_word(word: &str) -> bool {
    let head = token_head(word);
    !head.is_empty() && SOFT_TOKENS.contains(&head.as_str())
}

/// Find the byte index of a standalone 4-digit year (1900–2099).
pub fn find_year_index(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit() {
            let boundary_before = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let boundary_after = i + 4 == bytes.len() || !bytes[i + 4].is_ascii_alphanumeric();
            if boundary_before && boundary_after && is_plausible_year(&s[i..i + 4]) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn is_plausible_year(chunk: &str) -> bool {
    chunk
        .parse::<u32>()
        .map(|y| (1900..=2099).contains(&y))
        .unwrap_or(false)
}

/// Best-effort year parse from a name. A parenthesised `(YYYY)` wins over a bare
/// 4-digit number so "Blade Runner 2049 (2017)" resolves to 2017.
pub fn parse_year(name: &str) -> Option<u32> {
    if let Some((_, y)) = paren_year(name) {
        return Some(y);
    }
    let s = normalize_separators(name);
    let idx = find_year_index(&s)?;
    s[idx..idx + 4].parse::<u32>().ok()
}
