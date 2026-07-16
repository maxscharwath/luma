//! TMDB discovery: search/trending/detail for titles the library may NOT have
//! yet the request flow's window onto the outside catalog. Same curl/JSON
//! transport as the sibling enrichment [`super::client`]; returns plain adapter
//! structs the services layer flags against the local catalog + open requests
//! before they become wire types.

use serde::Deserialize;

use crate::domain::metadata::{CastMember, CrewMember};
use crate::model::RequestKind;

use super::client::{curl_json, API, IMG};

/// How many top-billed cast / similar titles to keep on a discovery detail.
const MAX_CAST: usize = 12;
const MAX_CREW: usize = 6;
const MAX_SIMILAR: usize = 12;
/// TMDB crew jobs surfaced on the discovery detail (authorship roles, ranked).
const KEY_CREW_JOBS: &[&str] = &["Director", "Creator", "Writer", "Screenplay", "Story"];

/// One search/trending hit, unflagged.
#[derive(Debug, Clone)]
pub struct DiscoverHit {
    pub kind: RequestKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    pub rating: Option<f32>,
}

/// A page of hits.
#[derive(Debug, Clone, Default)]
pub struct DiscoverPage {
    pub hits: Vec<DiscoverHit>,
    pub page: u32,
    pub total_pages: u32,
}

/// A title's detail for the request flow (movie runtime / show seasons).
#[derive(Debug, Clone)]
pub struct DiscoverRawDetail {
    pub kind: RequestKind,
    pub tmdb_id: u64,
    pub title: String,
    pub year: Option<u32>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f32>,
    pub runtime_min: Option<u32>,
    pub imdb_id: Option<String>,
    /// Regular seasons only (specials / season 0 excluded), ascending.
    pub seasons: Vec<RawSeason>,
    /// Top-billed cast (name + character + photo).
    pub cast: Vec<CastMember>,
    /// Key crew (directors / creators / writers), directors first.
    pub crew: Vec<CrewMember>,
    /// TMDB recommendations for the "Titres similaires" rail (unflagged).
    pub similar: Vec<DiscoverHit>,
    /// TMDB airing status ("Returning Series"/"Ended"/"Canceled" for shows;
    /// "Released"/"Post Production"/… for movies). Feeds the refresh skip gate.
    pub status: Option<String>,
    /// Show only: the next episode TMDB expects to air, as
    /// `(air_date YYYY-MM-DD, season, episode)`. `None` for movies / shows with
    /// nothing upcoming.
    pub next_air: Option<(String, u32, u32)>,
    /// Movie only: soonest home-availability date (digital > theatrical >
    /// physical > basic release_date), `YYYY-MM-DD`. `None` for shows.
    pub available_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RawSeason {
    pub season: u32,
    pub name: Option<String>,
    pub episode_count: u32,
    pub air_date: Option<String>,
}

/// Which namespace(s) a search targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverScope {
    Movies,
    Shows,
    All,
}

/// Search TMDB. `All` uses multi-search (drops `person` hits); the scoped
/// variants hit the movie/tv endpoints directly.
// The `()` Err is the unit error threaded up from `curl_json` via `?`; callers only
// care ok/err, so a richer error type would add nothing here.
#[allow(clippy::result_unit_err)]
pub fn search(
    api_key: &str,
    language: &str,
    scope: DiscoverScope,
    query: &str,
    page: u32,
) -> Result<DiscoverPage, ()> {
    let params = vec![
        ("language", language.to_string()),
        ("query", query.to_string()),
        ("include_adult", "false".to_string()),
        ("page", page.max(1).to_string()),
    ];
    let path = match scope {
        DiscoverScope::Movies => "search/movie",
        DiscoverScope::Shows => "search/tv",
        DiscoverScope::All => "search/multi",
    };
    let resp: PageResp = curl_json(&format!("{API}/{path}"), api_key, &params)?;
    Ok(map_page(resp, scope))
}

/// This week's trending titles. `All` (movies + shows, page 1) powers the
/// discover empty-state rails; the scoped variants back the full "trending
/// movies" / "trending shows" pages with real pagination.
// The `()` Err is the unit error threaded up from `curl_json` via `?`; callers only
// care ok/err, so a richer error type would add nothing here.
#[allow(clippy::result_unit_err)]
pub fn trending(
    api_key: &str,
    language: &str,
    scope: DiscoverScope,
    page: u32,
) -> Result<DiscoverPage, ()> {
    let path = match scope {
        DiscoverScope::Movies => "trending/movie/week",
        DiscoverScope::Shows => "trending/tv/week",
        DiscoverScope::All => "trending/all/week",
    };
    let params = vec![("language", language.to_string()), ("page", page.max(1).to_string())];
    let resp: PageResp = curl_json(&format!("{API}/{path}"), api_key, &params)?;
    Ok(map_page(resp, scope))
}

/// Fetch one title's detail (+ external ids for the wanted ledger).
// The `()` Err is the unit error threaded up from `curl_json` via `?`; callers only
// care ok/err, so a richer error type would add nothing here.
#[allow(clippy::result_unit_err)]
pub fn detail(
    api_key: &str,
    language: &str,
    kind: RequestKind,
    tmdb_id: u64,
) -> Result<Option<DiscoverRawDetail>, ()> {
    let path = match kind {
        RequestKind::Movie => "movie",
        RequestKind::Show => "tv",
    };
    // `recommendations` is TMDB's editorially-tuned "more like this" (better than
    // the raw `similar` genre overlap); `credits` carries cast + crew. Movies add
    // `release_dates` for the typed home-availability windows (digital / physical /
    // theatrical); shows carry `status` + `next_episode_to_air` natively (no append).
    let append = match kind {
        RequestKind::Movie => "external_ids,credits,recommendations,release_dates",
        RequestKind::Show => "external_ids,credits,recommendations",
    };
    let params = vec![
        ("language", language.to_string()),
        ("append_to_response", append.to_string()),
    ];
    let mut d: DetailResp = match curl_json(&format!("{API}/{path}/{tmdb_id}"), api_key, &params) {
        Ok(d) => d,
        // curl -f turns TMDB 404s into exit 22; treat any failure on the detail
        // endpoint as "not found" only when the id namespace mismatched is
        // indistinguishable, so callers surface a uniform not-found.
        Err(()) => return Err(()),
    };
    let title = match d.title.or(d.name) {
        Some(t) => t,
        None => return Ok(None),
    };
    let seasons = d
        .seasons
        .into_iter()
        .filter(|s| s.season_number.unwrap_or(0) >= 1)
        .map(|s| RawSeason {
            season: s.season_number.unwrap_or(1),
            name: s.name.filter(|n| !n.is_empty()),
            episode_count: s.episode_count.unwrap_or(0),
            air_date: s.air_date.filter(|a| !a.is_empty()),
        })
        .collect();
    let (raw_cast, raw_crew) = d.credits.map(|c| (c.cast, c.crew)).unwrap_or_default();
    let cast = map_cast(raw_cast);
    let crew = map_crew(raw_crew, d.created_by);
    // `recommendations` is a `movie` detail even when appended to a `tv` detail,
    // so the media_type is implied by the parent title's kind.
    let similar = d
        .recommendations
        .map(|r| r.results)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|h| hit_from(kind, h))
        .take(MAX_SIMILAR)
        .collect();
    // Airing signals for the request refresh pass + "coming soon" badges.
    let status = d.status.take().filter(|s| !s.is_empty());
    let next_air = d.next_episode_to_air.take().and_then(|e| {
        let date = e.air_date.filter(|s| !s.is_empty())?;
        Some((date, e.season_number.unwrap_or(0), e.episode_number.unwrap_or(0)))
    });
    let available_date = match kind {
        RequestKind::Movie => movie_available_date(d.release_dates.take(), d.release_date.as_deref()),
        RequestKind::Show => None,
    };
    Ok(Some(DiscoverRawDetail {
        kind,
        tmdb_id,
        title,
        year: year_of(d.release_date.as_deref().or(d.first_air_date.as_deref())),
        poster_url: d.poster_path.map(|p| format!("{IMG}/w500{p}")),
        backdrop_url: d.backdrop_path.map(|p| format!("{IMG}/w1280{p}")),
        overview: d.overview.filter(|s| !s.is_empty()),
        tagline: d.tagline.filter(|s| !s.is_empty()),
        genres: d.genres.into_iter().map(|g| g.name).collect(),
        rating: d.vote_average.filter(|v| *v > 0.0),
        runtime_min: d.runtime.filter(|&r| r > 0),
        imdb_id: d
            .imdb_id
            .or(d.external_ids.and_then(|e| e.imdb_id))
            .filter(|s| !s.is_empty()),
        seasons,
        cast,
        crew,
        similar,
        status,
        next_air,
        available_date,
    }))
}

/// TMDB release types we key on for a movie's home-availability date.
const REL_THEATRICAL: u32 = 3;
const REL_DIGITAL: u32 = 4;
const REL_PHYSICAL: u32 = 5;

/// A movie's "available_date": when it can plausibly be acquired. Prefers the
/// digital release, then theatrical, then physical, then the basic release_date.
/// Reads the US window (most complete for digital) and falls back to any
/// country. All dates normalized to `YYYY-MM-DD`.
fn movie_available_date(rd: Option<ReleaseDatesResp>, basic: Option<&str>) -> Option<String> {
    let results = rd.map(|r| r.results).unwrap_or_default();
    let block = results
        .iter()
        .find(|c| c.iso_3166_1.as_deref() == Some("US"))
        .or_else(|| results.first());
    let typed = block.and_then(|b| {
        typed_release(b, REL_DIGITAL)
            .or_else(|| typed_release(b, REL_THEATRICAL))
            .or_else(|| typed_release(b, REL_PHYSICAL))
    });
    typed.or_else(|| basic.and_then(ymd))
}

/// The `YYYY-MM-DD` of a country block's first release of type `want`.
fn typed_release(block: &ReleaseDatesCountry, want: u32) -> Option<String> {
    block
        .release_dates
        .iter()
        .find(|d| d.kind == want)
        .and_then(|d| d.release_date.as_deref())
        .and_then(ymd)
}

/// Truncate a TMDB date/datetime ("2024-02-27T00:00:00.000Z") to `YYYY-MM-DD`.
fn ymd(s: &str) -> Option<String> {
    s.get(..10).map(str::to_string)
}

/// Top-billed cast (TMDB orders by `order` ascending; sort defensively), capped.
fn map_cast(mut raw: Vec<RawCast>) -> Vec<CastMember> {
    raw.sort_by_key(|m| m.order.unwrap_or(u32::MAX));
    raw.into_iter()
        .filter(|m| !m.name.is_empty())
        .take(MAX_CAST)
        .map(|m| CastMember {
            name: m.name,
            character: m.character.filter(|s| !s.is_empty()),
            profile_url: m.profile_path.map(|p| format!("{IMG}/w185{p}")),
        })
        .collect()
}

/// Authorship crew (directors/creators first), one row per person, capped. TV
/// series carry their creators in the top-level `created_by` block instead.
fn map_crew(crew: Vec<RawCrew>, created_by: Vec<RawCreatedBy>) -> Vec<CrewMember> {
    let rank = |job: &str| KEY_CREW_JOBS.iter().position(|j| *j == job).unwrap_or(usize::MAX);
    let mut candidates: Vec<(usize, CrewMember)> = crew
        .into_iter()
        .filter(|c| !c.name.is_empty() && KEY_CREW_JOBS.contains(&c.job.as_str()))
        .map(|c| (rank(&c.job), CrewMember { name: c.name, job: c.job, profile_url: None }))
        .collect();
    for cb in created_by.into_iter().filter(|c| !c.name.is_empty()) {
        candidates.push((rank("Creator"), CrewMember { name: cb.name, job: "Creator".into(), profile_url: None }));
    }
    candidates.sort_by_key(|(r, _)| *r);
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|(_, m)| seen.insert(m.name.clone()))
        .map(|(_, m)| m)
        .take(MAX_CREW)
        .collect()
}

/// Map one raw TMDB row (search / trending / recommendation) into a hit;
/// `kind` is the row's resolved namespace. Skips rows with no usable title.
fn hit_from(kind: RequestKind, h: RawHit) -> Option<DiscoverHit> {
    let title = h.title.or(h.name)?;
    Some(DiscoverHit {
        kind,
        tmdb_id: h.id,
        title,
        year: year_of(h.release_date.as_deref().or(h.first_air_date.as_deref())),
        poster_url: h.poster_path.map(|p| format!("{IMG}/w342{p}")),
        backdrop_url: h.backdrop_path.map(|p| format!("{IMG}/w780{p}")),
        overview: h.overview.filter(|s| !s.is_empty()),
        rating: h.vote_average.filter(|v| *v > 0.0),
    })
}

fn map_page(resp: PageResp, scope: DiscoverScope) -> DiscoverPage {
    let hits = resp
        .results
        .into_iter()
        .filter_map(|h| {
            // Multi-search mixes movies, shows and people; scoped searches carry
            // no media_type (the endpoint implies it).
            let kind = match h.media_type.as_deref() {
                Some("movie") => RequestKind::Movie,
                Some("tv") => RequestKind::Show,
                Some(_) => return None, // person / collection
                None => match scope {
                    DiscoverScope::Movies => RequestKind::Movie,
                    DiscoverScope::Shows => RequestKind::Show,
                    DiscoverScope::All => return None,
                },
            };
            hit_from(kind, h)
        })
        .collect();
    DiscoverPage { hits, page: resp.page.max(1), total_pages: resp.total_pages.max(1) }
}

/// `"2019-07-12"` -> `2019`.
fn year_of(date: Option<&str>) -> Option<u32> {
    date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok())
}

// ----- raw TMDB JSON shapes ------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PageResp {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    total_pages: u32,
    #[serde(default)]
    results: Vec<RawHit>,
}

#[derive(Debug, Deserialize)]
struct RawHit {
    id: u64,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    vote_average: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct DetailResp {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    tagline: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    vote_average: Option<f32>,
    #[serde(default)]
    runtime: Option<u32>,
    #[serde(default)]
    genres: Vec<RawGenre>,
    #[serde(default)]
    imdb_id: Option<String>,
    #[serde(default)]
    external_ids: Option<RawExternalIds>,
    #[serde(default)]
    seasons: Vec<RawSeasonResp>,
    #[serde(default)]
    credits: Option<RawCredits>,
    #[serde(default)]
    created_by: Vec<RawCreatedBy>,
    #[serde(default)]
    recommendations: Option<PageResp>,
    // ----- airing signals (Phase 2: refresh + coming-soon) -------------------
    /// Show: "Returning Series"/"Ended"/"Canceled"/"In Production"; movie:
    /// "Released"/"Post Production"/…
    #[serde(default)]
    status: Option<String>,
    /// Show only: the next episode TMDB expects to air.
    #[serde(default)]
    next_episode_to_air: Option<RawEpisodeStub>,
    /// Show only: the most recent aired episode. Parsed for the coming-soon
    /// calendar (later phase); not read yet.
    #[serde(default)]
    #[allow(dead_code)]
    last_episode_to_air: Option<RawEpisodeStub>,
    /// Show only: total episode count, backing the calendar's completeness
    /// check (later phase); not read yet.
    #[serde(default)]
    #[allow(dead_code)]
    number_of_episodes: Option<u32>,
    /// Movie only: per-country typed release windows (append_to_response).
    #[serde(default)]
    release_dates: Option<ReleaseDatesResp>,
}

#[derive(Debug, Deserialize)]
struct RawCredits {
    #[serde(default)]
    cast: Vec<RawCast>,
    #[serde(default)]
    crew: Vec<RawCrew>,
}

#[derive(Debug, Deserialize)]
struct RawCast {
    #[serde(default)]
    name: String,
    #[serde(default)]
    character: Option<String>,
    #[serde(default)]
    profile_path: Option<String>,
    #[serde(default)]
    order: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawCrew {
    #[serde(default)]
    name: String,
    #[serde(default)]
    job: String,
}

/// TV `created_by` block (top-level on series details) the show's creators.
#[derive(Debug, Deserialize)]
struct RawCreatedBy {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawExternalIds {
    #[serde(default)]
    imdb_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSeasonResp {
    #[serde(default)]
    season_number: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    episode_count: Option<u32>,
    #[serde(default)]
    air_date: Option<String>,
}

/// TMDB `next_episode_to_air` / `last_episode_to_air`: the show's upcoming (or
/// most recent) episode's air date + numbering.
#[derive(Debug, Deserialize)]
struct RawEpisodeStub {
    #[serde(default)]
    air_date: Option<String>,
    #[serde(default)]
    season_number: Option<u32>,
    #[serde(default)]
    episode_number: Option<u32>,
}

/// TMDB movie `release_dates` append: per-country typed release windows.
#[derive(Debug, Deserialize)]
struct ReleaseDatesResp {
    #[serde(default)]
    results: Vec<ReleaseDatesCountry>,
}

#[derive(Debug, Deserialize)]
struct ReleaseDatesCountry {
    #[serde(default)]
    iso_3166_1: Option<String>,
    #[serde(default)]
    release_dates: Vec<ReleaseDateEntry>,
}

#[derive(Debug, Deserialize)]
struct ReleaseDateEntry {
    /// TMDB release type: 3 = theatrical, 4 = digital, 5 = physical (others exist).
    #[serde(rename = "type", default)]
    kind: u32,
    #[serde(default)]
    release_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A detail payload with appended `credits` + `recommendations` maps into the
    /// cast/crew/similar the discovery detail carries (network is not exercised;
    /// we validate the JSON→adapter mapping the way the sibling client does).
    #[test]
    fn maps_credits_and_recommendations() {
        let raw = r#"{
            "title": "Dune: Part Two",
            "credits": {
                "cast": [
                    {"name": "Second", "character": "B", "profile_path": "/b.jpg", "order": 1},
                    {"name": "First", "character": "A", "profile_path": "/a.jpg", "order": 0},
                    {"name": "NoChar", "character": "", "order": 2}
                ],
                "crew": [
                    {"name": "Editor Ed", "job": "Editor"},
                    {"name": "Denis", "job": "Director"},
                    {"name": "Writer Wanda", "job": "Writer"}
                ]
            },
            "recommendations": {
                "page": 1,
                "total_pages": 1,
                "results": [
                    {"id": 693134, "title": "Rec One", "poster_path": "/r.jpg", "release_date": "2024-02-27", "vote_average": 8.2},
                    {"id": 0, "title": null}
                ]
            }
        }"#;
        let d: DetailResp = serde_json::from_str(raw).unwrap();

        let (raw_cast, raw_crew) = d.credits.map(|c| (c.cast, c.crew)).unwrap_or_default();
        let cast = map_cast(raw_cast);
        // Sorted by `order`; empty characters dropped; profile URL absolutized.
        assert_eq!(cast[0].name, "First");
        assert_eq!(cast[0].character.as_deref(), Some("A"));
        assert_eq!(cast[0].profile_url.as_deref(), Some("https://image.tmdb.org/t/p/w185/a.jpg"));
        assert_eq!(cast[2].character, None);

        let crew = map_crew(raw_crew, d.created_by);
        // Non-authorship jobs (Editor) dropped; directors rank first.
        assert_eq!(crew[0].name, "Denis");
        assert!(crew.iter().all(|c| c.name != "Editor Ed"));

        let similar: Vec<_> = d
            .recommendations
            .map(|r| r.results)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|h| hit_from(RequestKind::Movie, h))
            .collect();
        // The title-less row is skipped; the good one keeps the parent's kind.
        assert_eq!(similar.len(), 1);
        assert_eq!(similar[0].tmdb_id, 693134);
        assert_eq!(similar[0].kind, RequestKind::Movie);
        assert_eq!(similar[0].year, Some(2024));
    }

    /// A movie's `available_date` prefers the US digital window, then theatrical,
    /// then the basic release_date; all normalized to `YYYY-MM-DD`.
    #[test]
    fn movie_available_date_prefers_digital_then_theatrical() {
        let raw = r#"{
            "release_dates": { "results": [
                { "iso_3166_1": "FR", "release_dates": [
                    { "type": 3, "release_date": "2024-02-28T00:00:00.000Z" }
                ]},
                { "iso_3166_1": "US", "release_dates": [
                    { "type": 3, "release_date": "2024-03-01T00:00:00.000Z" },
                    { "type": 4, "release_date": "2024-04-16T00:00:00.000Z" }
                ]}
            ]}
        }"#;
        let d: DetailResp = serde_json::from_str(raw).unwrap();
        // US digital (type 4) wins over US/FR theatrical, truncated to a date.
        assert_eq!(
            movie_available_date(d.release_dates, Some("2024-03-01")).as_deref(),
            Some("2024-04-16")
        );
        // No typed windows: fall back to the basic release_date.
        assert_eq!(movie_available_date(None, Some("2025-01-09")).as_deref(), Some("2025-01-09"));
    }

    /// A show's `status` + `next_episode_to_air` map into the airing signals.
    #[test]
    fn show_next_air_maps_from_next_episode() {
        let raw = r#"{
            "name": "Severance",
            "status": "Returning Series",
            "next_episode_to_air": { "air_date": "2026-01-17", "season_number": 2, "episode_number": 3 },
            "number_of_episodes": 19
        }"#;
        let d: DetailResp = serde_json::from_str(raw).unwrap();
        assert_eq!(d.status.as_deref(), Some("Returning Series"));
        let n = d.next_episode_to_air.unwrap();
        assert_eq!(n.air_date.as_deref(), Some("2026-01-17"));
        assert_eq!(n.season_number, Some(2));
        assert_eq!(n.episode_number, Some(3));
    }

    /// TV creators come from the top-level `created_by`, not the crew list.
    #[test]
    fn tv_creators_fold_into_crew() {
        let raw = r#"{
            "name": "Severance",
            "created_by": [{"name": "Dan Erickson"}],
            "credits": {"cast": [], "crew": []}
        }"#;
        let d: DetailResp = serde_json::from_str(raw).unwrap();
        let crew = map_crew(d.credits.map(|c| c.crew).unwrap_or_default(), d.created_by);
        assert_eq!(crew.len(), 1);
        assert_eq!(crew[0].name, "Dan Erickson");
        assert_eq!(crew[0].job, "Creator");
    }
}
