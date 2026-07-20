//! The search half of TMDB enrichment: gather *candidates*, then let
//! [`kroma_domain::matching`] pick one, instead of trusting TMDB's first result.
//!
//! Two things went wrong with taking `results[0]`:
//!
//! 1. TMDB orders by its own popularity heuristic, so a generic title ("It",
//!    "Frozen") routinely resolved to the wrong film and nothing downstream ever
//!    re-questioned the stored poster.
//! 2. TMDB's year filter is *exact*, so a filename carrying the production year
//!    instead of the release year returned zero results and the title was
//!    recorded as a permanent miss.
//!
//! So: search with the year, and if nothing credible comes back, widen to an
//! unfiltered search and score the union. Scoring is pure and lives in the domain
//! crate; this module is only the HTTP half.

use serde::Deserialize;

use kroma_domain::matching::{self, Candidate, Query};

use super::client::{curl_json, Target, API};

/// Resolve `title`/`year` to the best TMDB id, or `Ok(None)` when nothing is a
/// credible match (a miss the caller may cache). `Err(())` is a transport
/// failure, which must never be cached.
pub(super) fn best_id(
    api_key: &str,
    language: &str,
    target: Target,
    title: &str,
    year: Option<u32>,
) -> Result<Option<u64>, ()> {
    let found = candidates(api_key, language, target, title, year)?;
    let query = Query { title, year };
    Ok(matching::pick_best(&query, &found).map(|(c, _)| c.tmdb_id))
}

/// Every candidate TMDB offers for `title`/`year`, deduped by id: the
/// year-filtered results first, widened with an unfiltered search when the
/// filtered set holds nothing credible.
fn candidates(
    api_key: &str,
    language: &str,
    target: Target,
    title: &str,
    year: Option<u32>,
) -> Result<Vec<Candidate>, ()> {
    let mut found = search_page(api_key, language, target, title, year)?;
    let query = Query { title, year };
    if year.is_some() && matching::pick_best(&query, &found).is_none() {
        for extra in search_page(api_key, language, target, title, None)? {
            if !found.iter().any(|c| c.tmdb_id == extra.tmdb_id) {
                found.push(extra);
            }
        }
    }
    Ok(found)
}

/// One TMDB search request. `year` adds the target's exact-year filter.
fn search_page(
    api_key: &str,
    language: &str,
    target: Target,
    title: &str,
    year: Option<u32>,
) -> Result<Vec<Candidate>, ()> {
    let mut params = vec![
        ("language", language.to_string()),
        ("query", title.to_string()),
        ("include_adult", "false".to_string()),
    ];
    if let Some(y) = year {
        params.push((target.year_param(), y.to_string()));
    }
    let resp: SearchResp =
        curl_json(&format!("{API}/{}", target.search_path()), api_key, &params)?;
    Ok(resp.results.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
struct SearchResp {
    #[serde(default)]
    results: Vec<SearchHit>,
}

/// A TMDB search result. Movies and shows use different field names for the same
/// three things, hence the pairs.
#[derive(Debug, Deserialize)]
struct SearchHit {
    id: u64,
    #[serde(default)]
    title: Option<String>, // movies
    #[serde(default)]
    name: Option<String>, // shows
    #[serde(default)]
    original_title: Option<String>, // movies
    #[serde(default)]
    original_name: Option<String>, // shows
    #[serde(default)]
    release_date: Option<String>, // movies
    #[serde(default)]
    first_air_date: Option<String>, // shows
    #[serde(default)]
    vote_count: u32,
}

impl From<SearchHit> for Candidate {
    fn from(h: SearchHit) -> Candidate {
        Candidate {
            tmdb_id: h.id,
            title: h.title.or(h.name).unwrap_or_default(),
            original_title: h.original_title.or(h.original_name).unwrap_or_default(),
            year: year_of(h.release_date.as_deref().or(h.first_air_date.as_deref())),
            votes: h.vote_count,
        }
    }
}

/// The year out of a TMDB `YYYY-MM-DD` date.
pub(super) fn year_of(date: Option<&str>) -> Option<u32> {
    date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(json: &str) -> Candidate {
        serde_json::from_str::<SearchHit>(json).expect("valid hit").into()
    }

    #[test]
    fn maps_a_movie_hit() {
        let c = hit(
            r#"{"id":603,"title":"La Matrice","original_title":"The Matrix",
                "release_date":"1999-03-30","vote_count":24000}"#,
        );
        assert_eq!(c.tmdb_id, 603);
        assert_eq!(c.title, "La Matrice");
        assert_eq!(c.original_title, "The Matrix");
        assert_eq!(c.year, Some(1999));
        assert_eq!(c.votes, 24000);
    }

    #[test]
    fn maps_a_show_hit_from_its_name_fields() {
        let c = hit(r#"{"id":1396,"name":"Breaking Bad","first_air_date":"2008-01-20"}"#);
        assert_eq!(c.title, "Breaking Bad");
        assert_eq!(c.year, Some(2008));
    }

    #[test]
    fn tolerates_a_hit_with_nothing_but_an_id() {
        let c = hit(r#"{"id":7}"#);
        assert_eq!(c.tmdb_id, 7);
        assert!(c.title.is_empty());
        assert_eq!(c.year, None);
    }

    #[test]
    fn empty_search_results_deserialize() {
        let s: SearchResp = serde_json::from_str(r#"{"results": []}"#).unwrap();
        assert!(s.results.is_empty());
    }

    #[test]
    fn year_of_handles_missing_and_malformed_dates() {
        assert_eq!(year_of(Some("2014-11-05")), Some(2014));
        assert_eq!(year_of(Some("")), None);
        assert_eq!(year_of(None), None);
    }
}
