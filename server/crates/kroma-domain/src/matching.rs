//! Pure TMDB candidate matching: normalize titles, score search hits against the
//! `(title, year)` a filename parsed to, and pick the best one *or reject them
//! all*.
//!
//! TMDB search is fuzzy and orders by its own popularity heuristic, so the first
//! result is regularly the wrong title (generic names like "It" or "Frozen"), and
//! a year-filtered search returns nothing at all when the filename carries the
//! production year instead of the release year. Scoring here lets the client
//! widen the search and still pick sensibly, and lets the "fix the match" UI show
//! *why* a candidate ranked where it did.
//!
//! Zero I/O: the HTTP half lives in the engine's `infra::metadata::search`.

/// One TMDB search hit, reduced to what scoring needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub tmdb_id: u64,
    /// Localized title (`title` for movies, `name` for shows).
    pub title: String,
    /// Original-language title, often the only one that matches a scene release.
    pub original_title: String,
    pub year: Option<u32>,
    /// TMDB `vote_count`: the tiebreaker between two equally-titled candidates
    /// (a remake vs. the well-known original).
    pub votes: u32,
}

/// What the filename parsed to, i.e. what we are trying to match.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    pub title: &'a str,
    pub year: Option<u32>,
}

/// Below this, we would rather record a miss than store a wrong poster: a bad
/// match is worse than none, because nothing downstream re-questions it.
pub const MIN_SCORE: f32 = 0.35;

/// Title similarity carries most of the weight; the rest is the year signal.
const SIM_WEIGHT: f32 = 0.75;
/// Same year: near-conclusive when the title also roughly matches, and the thing
/// that rescues a correct hit TMDB found through an alternative title we cannot
/// see (a French filename resolving to an English `title`/`original_title` pair).
const YEAR_EXACT: f32 = 0.25;
/// Off by one: release year vs. festival/production year, extremely common.
const YEAR_NEAR: f32 = 0.10;
/// Years that genuinely disagree: almost always a different title entirely.
const YEAR_FAR: f32 = -0.35;
/// Tiny nudge so a well-known title outranks an obscure namesake; capped low
/// enough that it can never overturn a title or year signal.
const VOTES_WEIGHT: f32 = 0.03;
const VOTES_CAP: u32 = 2000;

/// Score one candidate in `0.0..=1.0`. See [`MIN_SCORE`] for the accept cutoff.
pub fn score(query: &Query, candidate: &Candidate) -> f32 {
    let sim = similarity(query.title, &candidate.title)
        .max(similarity(query.title, &candidate.original_title));
    let year_adj = match (query.year, candidate.year) {
        (Some(a), Some(b)) if a == b => YEAR_EXACT,
        (Some(a), Some(b)) if a.abs_diff(b) <= 1 => YEAR_NEAR,
        (Some(_), Some(_)) => YEAR_FAR,
        // One side has no year: no evidence either way, so neither bonus nor
        // penalty (the title then has to carry the match on its own).
        _ => 0.0,
    };
    let votes = VOTES_WEIGHT * (candidate.votes.min(VOTES_CAP) as f32 / VOTES_CAP as f32);
    (SIM_WEIGHT * sim + year_adj + votes).clamp(0.0, 1.0)
}

/// The best candidate and its score, or `None` when nothing clears [`MIN_SCORE`].
pub fn pick_best<'a>(query: &Query, candidates: &'a [Candidate]) -> Option<(&'a Candidate, f32)> {
    candidates
        .iter()
        .map(|c| (c, score(query, c)))
        .filter(|&(_, s)| s >= MIN_SCORE)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Sorensen-Dice coefficient over the character bigrams of the two normalized
/// titles, in `0.0..=1.0`. Chosen over edit distance because it is length- and
/// word-order-tolerant: a missing subtitle degrades gracefully instead of falling
/// off a cliff, and a transposed word costs far less than it would in Levenshtein.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (mut left, right) = (bigrams(&a), bigrams(&b));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let total = left.len() + right.len();
    // Consume each match so a repeated bigram cannot be counted twice.
    let mut hits = 0usize;
    for bg in right {
        if let Some(pos) = left.iter().position(|x| *x == bg) {
            left.swap_remove(pos);
            hits += 1;
        }
    }
    (2.0 * hits as f32) / total as f32
}

/// Fold a title to its comparable form: lowercase, accents stripped, every run of
/// punctuation reduced to one space, leading article dropped.
pub fn normalize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            continue;
        }
        match fold(ch) {
            Some(s) => out.push_str(s),
            // Punctuation / symbols become a separator rather than vanishing, so
            // "spider-man" and "spider man" agree.
            None if !out.ends_with(' ') => out.push(' '),
            None => {}
        }
    }
    strip_article(out.trim())
}

/// Lowercase + de-accent one non-ASCII-alphanumeric char; `None` for anything
/// that is not a letter. Only Latin-1 / Latin-A is folded, which covers every
/// language the catalog realistically carries with no unicode dependency.
fn fold(ch: char) -> Option<&'static str> {
    Some(match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'ç' | 'Ç' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
        'ñ' | 'Ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
        'ý' | 'ÿ' | 'Ý' => "y",
        'æ' | 'Æ' => "ae",
        'œ' | 'Œ' => "oe",
        'ß' => "ss",
        // Combining diacritical marks: a decomposed (NFD) accent, e.g. "é" stored
        // as `e` + U+0301. macOS filenames are NFD, so titles parsed from disk
        // carry these. Drop the mark the ASCII base letter already precedes it;
        // without this the mark would fold to a space and split the word.
        '\u{0300}'..='\u{036F}' => "",
        _ => return None,
    })
}

/// Strip decomposed (NFD) combining marks, leaving precomposed accents intact.
/// The lightest touch that makes an NFD title (macOS filenames) searchable: it
/// turns `e` + U+0301 into `e` without otherwise altering case, punctuation or a
/// precomposed `é`. Use it for a provider query where [`normalize`]'s fuller
/// folding (lowercasing, article stripping) would be too lossy.
pub fn strip_combining(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\u{0300}'..='\u{036F}')).collect()
}

/// Articles a catalog title may or may not carry ("The Matrix" vs "Matrix", "Le
/// Fabuleux destin..." vs "Fabuleux destin..."). Dropping a leading one makes the
/// two forms comparable. Both sides are normalized first, so `l'` is already `l `.
const ARTICLES: [&str; 12] =
    ["the ", "a ", "an ", "le ", "la ", "les ", "l ", "un ", "une ", "der ", "die ", "das "];

fn strip_article(s: &str) -> String {
    ARTICLES
        .iter()
        .find_map(|art| s.strip_prefix(art))
        .unwrap_or(s)
        .to_string()
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: u64, title: &str, year: Option<u32>) -> Candidate {
        Candidate {
            tmdb_id: id,
            title: title.to_string(),
            original_title: title.to_string(),
            year,
            votes: 0,
        }
    }

    #[test]
    fn normalize_folds_case_accents_and_punctuation() {
        assert_eq!(normalize("Amélie"), "amelie");
        assert_eq!(normalize("Spider-Man: No Way Home"), "spider man no way home");
        assert_eq!(normalize("  WALL·E  "), "wall e");
        assert_eq!(normalize("Fast & Furious"), "fast furious");
    }

    #[test]
    fn normalize_drops_a_leading_article() {
        assert_eq!(normalize("The Matrix"), "matrix");
        assert_eq!(normalize("L'Auberge espagnole"), "auberge espagnole");
        // Only a *leading* article, and only as a whole word.
        assert_eq!(normalize("Theodore"), "theodore");
    }

    #[test]
    fn normalize_drops_decomposed_combining_marks() {
        // macOS filenames are NFD: "é" arrives as `e` + U+0301. The mark must be
        // dropped, not folded to a word-splitting space ("de tective").
        assert_eq!(normalize("de\u{0301}tective"), "detective");
        assert_eq!(normalize("Ame\u{0301}lie"), "amelie");
        // Decomposed and precomposed forms fold identically.
        assert_eq!(normalize("Ame\u{0301}lie"), normalize("Amélie"));
    }

    #[test]
    fn strip_combining_removes_marks_but_keeps_precomposed() {
        // NFD "Amélie" (e + U+0301) loses the mark; a precomposed é is untouched.
        assert_eq!(strip_combining("Ame\u{0301}lie"), "Amelie");
        assert_eq!(strip_combining("Amélie"), "Amélie");
        assert_eq!(strip_combining("Ace Ventura"), "Ace Ventura");
    }

    #[test]
    fn similarity_is_one_for_equivalent_titles_and_zero_for_empty() {
        assert_eq!(similarity("The Matrix", "Matrix"), 1.0);
        assert_eq!(similarity("Amélie", "amelie"), 1.0);
        assert_eq!(similarity("", "Matrix"), 0.0);
        // A single-char title has no bigrams; nothing to compare against.
        assert_eq!(similarity("A", "Matrix"), 0.0);
    }

    #[test]
    fn similarity_degrades_gracefully_on_a_missing_subtitle() {
        let s = similarity("Blade Runner", "Blade Runner 2049");
        assert!(s > 0.8, "expected a high partial score, got {s}");
        assert!(s < 1.0);
    }

    #[test]
    fn similarity_is_low_for_unrelated_titles() {
        assert!(similarity("The Matrix", "Frozen") < 0.3);
    }

    #[test]
    fn exact_title_and_year_scores_near_one() {
        let q = Query { title: "The Matrix", year: Some(1999) };
        assert!(score(&q, &cand(603, "The Matrix", Some(1999))) > 0.99);
    }

    #[test]
    fn a_matching_year_lifts_a_partial_title_over_the_bar() {
        // Filenames drop or mangle subtitles constantly; the year is what makes
        // the remainder trustworthy enough to accept.
        let q = Query { title: "Blade Runner", year: Some(2017) };
        let s = score(&q, &cand(335984, "Blade Runner 2049", Some(2017)));
        assert!(s > 0.8, "expected a confident match, got {s}");
    }

    #[test]
    fn an_unrecognizable_title_is_rejected_even_on_an_exact_year() {
        // TMDB sometimes answers through an alternative title we never see, so
        // neither `title` nor `original_title` resembles the query. We choose to
        // record a miss: a wrong poster is invisible and nothing downstream
        // re-questions it, whereas a miss is visible and manually fixable.
        let q = Query { title: "Les Evades", year: Some(1994) };
        assert!(score(&q, &cand(278, "The Shawshank Redemption", Some(1994))) < MIN_SCORE);
    }

    #[test]
    fn a_wrong_year_sinks_an_otherwise_plausible_title() {
        let q = Query { title: "It", year: Some(2017) };
        assert!(score(&q, &cand(1, "It Follows", Some(2014))) < MIN_SCORE);
    }

    #[test]
    fn pick_best_prefers_the_right_year_over_tmdb_ordering() {
        // What TMDB returns first for "It" is not what the file is.
        let q = Query { title: "It", year: Some(1990) };
        let candidates = vec![cand(474350, "It", Some(2017)), cand(437, "It", Some(1990))];
        let (best, _) = pick_best(&q, &candidates).expect("a match");
        assert_eq!(best.tmdb_id, 437);
    }

    #[test]
    fn pick_best_rejects_everything_when_nothing_is_close() {
        let q = Query { title: "Some Obscure Documentary", year: None };
        assert!(pick_best(&q, &[cand(1, "Frozen", Some(2013))]).is_none());
    }

    #[test]
    fn pick_best_matches_on_the_original_title() {
        let q = Query { title: "La Haine", year: None };
        let c = Candidate {
            tmdb_id: 406,
            title: "Hate".to_string(),
            original_title: "La Haine".to_string(),
            year: Some(1995),
            votes: 0,
        };
        assert!(pick_best(&q, &[c]).is_some());
    }

    #[test]
    fn votes_break_a_tie_between_identical_titles() {
        let q = Query { title: "Titan", year: None };
        let obscure = Candidate { votes: 3, ..cand(1, "Titan", None) };
        let famous = Candidate { votes: 5000, ..cand(2, "Titan", None) };
        let candidates = [obscure, famous];
        let (best, _) = pick_best(&q, &candidates).expect("a match");
        assert_eq!(best.tmdb_id, 2);
    }

    #[test]
    fn score_stays_within_bounds() {
        let q = Query { title: "X", year: Some(2022) };
        let s = score(&q, &Candidate { votes: u32::MAX, ..cand(1, "Y", Some(1900)) });
        assert!((0.0..=1.0).contains(&s), "score {s} out of range");
    }
}
