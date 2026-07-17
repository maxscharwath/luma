//! Query construction.
//!
//! Each query token is matched fuzzily (typo tolerance) and by prefix (matches
//! while you're still typing/dictating) across every weighted text field. The
//! per-field matches are OR'd together; the per-token clauses are AND'd, so every
//! word must land somewhere "tom hardy" only matches a title whose cast (or
//! title) covers both words.

use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query};
use tantivy::schema::Field;
use tantivy::Term;

use super::schema::Fields;

/// Field weights: a title hit outranks an alt-title/cast hit, which outranks a
/// genre hit, which outranks a loose overview hit.
fn weights(f: &Fields) -> [(Field, f32); 6] {
    [
        (f.title, 6.0),
        (f.alt_title, 4.0),
        (f.show_title, 4.0),
        (f.cast, 3.0),
        (f.genres, 2.0),
        (f.overview, 1.0),
    ]
}

/// Edit-distance budget for a token. Short tokens get none (a single edit is a
/// different word); longer tokens tolerate more voice/typing noise grows with
/// length. tantivy caps fuzzy distance at 2.
fn distance(token: &str) -> u8 {
    match token.chars().count() {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    }
}

/// Build a query from already-normalized tokens (lowercased + diacritic-folded
/// by the index analyzer). Returns `None` when there are no tokens, so the caller
/// returns an empty result set rather than matching everything.
pub(super) fn build(fields: &Fields, tokens: &[String]) -> Option<Box<dyn Query>> {
    if tokens.is_empty() {
        return None;
    }
    let weights = weights(fields);
    let mut per_token: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let dist = distance(token);
        let mut variants: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(weights.len() * 2);
        for (field, boost) in weights {
            let term = Term::from_field_text(field, token);
            // Fuzzy match (transposition_cost_one = treat swaps as one edit).
            let fuzzy: Box<dyn Query> = Box::new(FuzzyTermQuery::new(term.clone(), dist, true));
            variants.push((Occur::Should, Box::new(BoostQuery::new(fuzzy, boost))));
            // Prefix match for partial words ("brea" → "Breaking"), at half weight.
            let prefix: Box<dyn Query> = Box::new(FuzzyTermQuery::new_prefix(term, 0, true));
            variants.push((Occur::Should, Box::new(BoostQuery::new(prefix, boost * 0.5))));
        }
        per_token.push((Occur::Must, Box::new(BooleanQuery::new(variants))));
    }
    Some(Box::new(BooleanQuery::new(per_token)))
}
