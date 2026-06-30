//! Season/episode marker detection (`S01E02`, `s1e2`, `S01E02-E03`, `1x02`) and
//! season-folder recognition the cues that classify a file as a TV episode.

/// A season/episode marker found in a filename stem, plus the byte span it
/// occupies (so the caller can split the stem into show-vs-episode-title parts).
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Marker {
    pub(super) season: u32,
    pub(super) episode: u32,
    pub(super) episode_end: Option<u32>,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Find the first plausible season/episode marker in a filename stem.
pub(super) fn find_marker(stem: &str) -> Option<Marker> {
    let lower = stem.to_ascii_lowercase();
    let b = lower.as_bytes();

    // Form 1: SxxEyy [(-|E)zz]
    let mut i = 0;
    while i < b.len() {
        if b[i] == b's' {
            let (sd, sn) = read_digits(b, i + 1);
            if sd > 0 {
                let j = i + 1 + sd;
                if j < b.len() && b[j] == b'e' {
                    let (ed, en) = read_digits(b, j + 1);
                    if ed > 0 && plausible(sn, en) {
                        let mut k = j + 1 + ed;
                        let mut episode_end = None;
                        // Optional multi-episode tail: "-E03", "-03", "E03".
                        let mut m = k;
                        if m < b.len() && b[m] == b'-' {
                            m += 1;
                        }
                        if m < b.len() && b[m] == b'e' {
                            m += 1;
                        }
                        if m > k {
                            let (ed2, en2) = read_digits(b, m);
                            if ed2 > 0 {
                                episode_end = Some(en2);
                                k = m + ed2;
                            }
                        }
                        return Some(Marker {
                            season: sn,
                            episode: en,
                            episode_end,
                            start: i,
                            end: k,
                        });
                    }
                }
            }
        }
        i += 1;
    }

    // Form 2: NxNN (e.g. 1x02). Bounded season to avoid matching resolutions.
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let (sd, sn) = read_digits(b, i);
            let before_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            if before_ok && sd >= 1 && sn <= 50 {
                let xpos = i + sd;
                if xpos < b.len() && b[xpos] == b'x' {
                    let (ed, en) = read_digits(b, xpos + 1);
                    if ed >= 1 && plausible(sn, en) {
                        return Some(Marker {
                            season: sn,
                            episode: en,
                            episode_end: None,
                            start: i,
                            end: xpos + 1 + ed,
                        });
                    }
                }
            }
            i += sd.max(1);
        } else {
            i += 1;
        }
    }

    None
}

/// Guard against resolutions / wild numbers being read as season/episode.
fn plausible(season: u32, episode: u32) -> bool {
    season <= 100 && episode <= 9999
}

/// Read a run of ASCII digits from `start`; returns (count, parsed value).
fn read_digits(b: &[u8], start: usize) -> (usize, u32) {
    let mut n = 0;
    while start + n < b.len() && b[start + n].is_ascii_digit() {
        n += 1;
    }
    if n == 0 {
        return (0, 0);
    }
    let value: u32 = std::str::from_utf8(&b[start..start + n])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (n, value)
}

/// "Season 01", "Saison 1", "Specials", "S01" → a season folder, not a show.
pub(super) fn is_season_folder(name: &str) -> bool {
    let l = name.trim().to_ascii_lowercase();
    l == "specials"
        || l.starts_with("season")
        || l.starts_with("saison")
        || (l.starts_with('s') && l.len() <= 4 && l[1..].bytes().all(|c| c.is_ascii_digit()))
}
