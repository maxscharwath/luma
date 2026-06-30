//! TMDB HTTP client: search for the best match, fetch its details + external
//! IDs / credits / images via `curl`, and map the JSON into a [`Metadata`].

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use tracing::{debug, warn};

use crate::domain::metadata::{CastMember, CrewMember, Metadata};

use super::cache::Cache;

const API: &str = "https://api.themoviedb.org/3";
const IMG: &str = "https://image.tmdb.org/t/p";

/// Whether to resolve against TMDB's movie or TV namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Movie,
    Tv,
}

impl Target {
    fn search_path(self) -> &'static str {
        match self {
            Target::Movie => "search/movie",
            Target::Tv => "search/tv",
        }
    }
    fn detail_path(self) -> &'static str {
        match self {
            Target::Movie => "movie",
            Target::Tv => "tv",
        }
    }
    /// TMDB uses a different year query param for movies vs. shows.
    /// `primary_release_year` is the precise movie filter Seerr/Overseerr use.
    fn year_param(self) -> &'static str {
        match self {
            Target::Movie => "primary_release_year",
            Target::Tv => "first_air_date_year",
        }
    }
    fn web_kind(self) -> &'static str {
        self.detail_path()
    }
}

/// How many cast members to keep (top-billed, by TMDB `order`).
const MAX_CAST: usize = 12;

/// How many key crew to keep (directors first, then writers/creators).
const MAX_CREW: usize = 8;
/// TMDB crew jobs we surface the authorship roles, ranked. Anything else
/// (gaffer, editor, …) is dropped.
const KEY_CREW_JOBS: &[&str] = &["Director", "Creator", "Writer", "Screenplay", "Story"];

/// How many TMDB keyword tags to keep (TMDB returns them unordered; the cap just
/// bounds how much thematic text feeds the embedding doc).
const MAX_KEYWORDS: usize = 20;

/// Detect whether `curl` is callable. Done once at startup for a log line.
pub fn curl_available() -> bool {
    Command::new("curl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve metadata for `title`/`year`, caching the result (hit or miss).
pub fn lookup(
    cache: &Cache,
    api_key: &str,
    language: &str,
    target: Target,
    title: &str,
    year: Option<u32>,
) -> Option<Metadata> {
    let key = format!(
        "{}|{}|{}|{}",
        target.detail_path(),
        language,
        year.unwrap_or(0),
        title.to_lowercase()
    );
    if let Some(cached) = cache.get(&key) {
        return cached;
    }
    match fetch(api_key, language, target, title, year) {
        // Genuine "looked up, no match" cache it so we don't retry every request.
        Ok(Some(meta)) => {
            cache.put(key, Some(meta.clone()));
            Some(meta)
        }
        Ok(None) => {
            cache.put(key, None);
            None
        }
        // A request failed (bad key, rate-limit, timeout, network). Do NOT cache:
        // caching `None` here would poison the title permanently on a transient
        // blip. Just return None for this call and let a later one retry.
        Err(()) => None,
    }
}

/// Search TMDB for the best match, then fetch its details + external IDs.
///
/// `Ok(Some)` = resolved, `Ok(None)` = searched fine but no match (cacheable),
/// `Err(())` = a request failed (transient caller must not cache it). Keeping
/// the no-match/failure split out of `Option` is what stops [`lookup`] from
/// poisoning a title on a transient blip.
fn fetch(
    api_key: &str,
    language: &str,
    target: Target,
    title: &str,
    year: Option<u32>,
) -> Result<Option<Metadata>, ()> {
    let mut search_params = vec![
        ("language", language.to_string()),
        ("query", title.to_string()),
        ("include_adult", "false".to_string()),
    ];
    if let Some(y) = year {
        search_params.push((target.year_param(), y.to_string()));
    }
    let search: SearchResp =
        curl_json(&format!("{API}/{}", target.search_path()), api_key, &search_params)?;
    let Some(hit) = search.results.first() else {
        return Ok(None);
    };
    let id = hit.id;

    // Base language code (e.g. "fr" from "fr-FR") for picking a localized logo.
    let lang2 = language.split('-').next().unwrap_or("en");
    let detail_params = vec![
        ("language", language.to_string()),
        ("append_to_response", "external_ids,credits,images,keywords".to_string()),
        // Logos: the configured language, English, and language-neutral.
        ("include_image_language", format!("{lang2},en,null")),
    ];
    let d: Details =
        curl_json(&format!("{API}/{}/{id}", target.detail_path()), api_key, &detail_params)?;

    let ext = d.external_ids;
    let imdb_id = d
        .imdb_id
        .or_else(|| ext.as_ref().and_then(|e| e.imdb_id.clone()))
        .filter(|s| !s.is_empty());
    // TVDB series id (TV only) keys the theme-song lookup during enrichment.
    let tvdb_id = ext.as_ref().and_then(|e| e.tvdb_id).filter(|&id| id > 0);

    // Cast + crew share the appended `credits` block. Keep the top-billed faces
    // (TMDB orders `cast` by `order` ascending; sort defensively) trimmed to a
    // rail size, and the key crew (directors first) for collections / the detail
    // "Réalisation" line. TV creators come from the top-level `created_by`.
    let (raw_cast, raw_crew) = d.credits.map(|c| (c.cast, c.crew)).unwrap_or_default();
    let mut cast_members = raw_cast;
    cast_members.sort_by_key(|m| m.order.unwrap_or(u32::MAX));
    let cast: Vec<CastMember> = cast_members
        .into_iter()
        .take(MAX_CAST)
        .map(|m| CastMember {
            name: m.name,
            character: m.character.filter(|s| !s.is_empty()),
            profile_url: m.profile_path.map(|p| format!("{IMG}/w185{p}")),
        })
        .collect();
    let crew = map_crew(raw_crew, d.created_by);

    Ok(Some(Metadata {
        provider: "tmdb",
        tmdb_id: d.id,
        imdb_id,
        title: d.title.or(d.name),
        tagline: d.tagline.filter(|s| !s.is_empty()),
        overview: d.overview.filter(|s| !s.is_empty()),
        release_date: d.release_date.or(d.first_air_date).filter(|s| !s.is_empty()),
        genres: d.genres.into_iter().map(|g| g.name).collect(),
        keywords: d.keywords.map(collect_keywords).unwrap_or_default(),
        rating: d.vote_average.filter(|v| *v > 0.0),
        poster_url: d.poster_path.map(|p| format!("{IMG}/w500{p}")),
        backdrop_url: d.backdrop_path.map(|p| format!("{IMG}/w1280{p}")),
        logo_url: d
            .images
            .as_ref()
            .and_then(|i| pick_logo(&i.logos, lang2))
            .map(|p| format!("{IMG}/w500{p}")),
        cast,
        crew,
        // Theme song is resolved later (a disk download) by `infra::theme`; the
        // pure lookup just carries the TVDB id it needs.
        theme_url: None,
        tvdb_id,
        tmdb_url: format!("https://www.themoviedb.org/{}/{id}", target.web_kind()),
    }))
}

/// Per-episode artwork + text resolved from a TMDB season fetch. `still_url` is
/// an absolute TMDB URL (localized to WebP by the enrichment pass, mirroring
/// poster/backdrop).
#[derive(Debug, Clone)]
pub struct EpisodeArt {
    pub episode: u32,
    pub still_url: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub rating: Option<f32>,
}

/// One season's episode stills + its season-level cast, from a single TMDB call.
#[derive(Debug, Clone, Default)]
pub struct SeasonData {
    pub episodes: Vec<EpisodeArt>,
    pub cast: Vec<CastMember>,
}

/// Fetch one season's episodes (with their stills) and cast for a resolved TV show
/// in a single TMDB call. Returns empty data on any failure season enrichment is
/// best-effort and must never break show enrichment.
pub fn season_episodes(api_key: &str, language: &str, tv_id: u64, season: u32) -> SeasonData {
    let params = vec![
        ("language", language.to_string()),
        ("append_to_response", "credits".to_string()),
    ];
    let resp: SeasonResp =
        match curl_json(&format!("{API}/tv/{tv_id}/season/{season}"), api_key, &params) {
            Ok(r) => r,
            Err(()) => return SeasonData::default(),
        };
    let episodes = resp
        .episodes
        .into_iter()
        .map(|e| EpisodeArt {
            episode: e.episode_number,
            still_url: e.still_path.map(|p| format!("{IMG}/w300{p}")),
            name: e.name.filter(|s| !s.is_empty()),
            overview: e.overview.filter(|s| !s.is_empty()),
            air_date: e.air_date.filter(|s| !s.is_empty()),
            rating: e.vote_average.filter(|v| *v > 0.0),
        })
        .collect();
    let mut cast_members = resp.credits.map(|c| c.cast).unwrap_or_default();
    cast_members.sort_by_key(|m| m.order.unwrap_or(u32::MAX));
    let cast = cast_members
        .into_iter()
        .take(MAX_CAST)
        .map(|m| CastMember {
            name: m.name,
            character: m.character.filter(|s| !s.is_empty()),
            profile_url: m.profile_path.map(|p| format!("{IMG}/w185{p}")),
        })
        .collect();
    SeasonData { episodes, cast }
}

#[derive(Debug, Deserialize)]
struct SeasonResp {
    #[serde(default)]
    episodes: Vec<RawEpisode>,
    #[serde(default)]
    credits: Option<Credits>,
}

#[derive(Debug, Deserialize)]
struct RawEpisode {
    #[serde(default)]
    episode_number: u32,
    #[serde(default)]
    still_path: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    air_date: Option<String>,
    #[serde(default)]
    vote_average: Option<f32>,
}

/// Emit one WARN the first time a TMDB request fails so a wrong
/// `LUMA_TMDB_API_KEY` or a dead network is visible at the default log level
/// instead of silently yielding `resolved=0`. Subsequent failures drop to DEBUG
/// to avoid spamming a bulk enrichment pass.
static FAILURE_WARNED: AtomicBool = AtomicBool::new(false);

fn note_curl_failure(reason: &str, detail: &str) {
    let detail = detail.trim();
    if FAILURE_WARNED.swap(true, Ordering::Relaxed) {
        debug!(reason, detail, "TMDB request failed");
    } else {
        warn!(
            reason,
            detail,
            "TMDB enrichment request failed check LUMA_TMDB_API_KEY and network connectivity; \
             further failures are logged at debug level"
        );
    }
}

/// GET `url` with URL-encoded query params via `curl`, parsed as JSON `T`.
///
/// Returns `Err(())` on any transport / HTTP-status / parse failure (logged via
/// [`note_curl_failure`]) and `Ok(T)` on success. The error is intentionally
/// distinct from an empty-but-valid response so the caller never caches a
/// transient failure as a permanent miss. `-S` keeps curl's error message on
/// stderr even under `-s`; curl exit 22 = HTTP ≥ 400 (e.g. 401 bad key), 28 =
/// timeout, 6/7 = DNS/connect.
fn curl_json<T: for<'de> Deserialize<'de>>(
    url: &str,
    api_key: &str,
    params: &[(&str, String)],
) -> Result<T, ()> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-S", "-f", "-G", "--max-time", "10"]);
    // TMDB accepts a v3 key as the `api_key` query param, or a v4 read token
    // (a JWT: header.payload.signature) as a Bearer header. Pick by shape.
    if is_bearer_token(api_key) {
        cmd.arg("-H").arg(format!("Authorization: Bearer {api_key}"));
    } else {
        cmd.arg("--data-urlencode").arg(format!("api_key={api_key}"));
    }
    cmd.arg(url);
    for (k, v) in params {
        cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    let out = match cmd.output() {
        Ok(out) => out,
        Err(e) => {
            note_curl_failure("spawn", &e.to_string());
            return Err(());
        }
    };
    if !out.status.success() {
        let code = out.status.code().unwrap_or(-1);
        note_curl_failure(
            &format!("curl_exit_{code}"),
            &String::from_utf8_lossy(&out.stderr),
        );
        return Err(());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| {
        note_curl_failure("parse", &e.to_string());
    })
}

/// A TMDB v4 read token is a JWT (`header.payload.signature`); v3 keys are
/// 32-char hex with no dots.
fn is_bearer_token(key: &str) -> bool {
    key.split('.').count() == 3
}

// ----- Raw TMDB JSON shapes ----------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchResp {
    #[serde(default)]
    results: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct Details {
    id: u64,
    #[serde(default)]
    title: Option<String>, // movies
    #[serde(default)]
    name: Option<String>, // shows
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    tagline: Option<String>,
    #[serde(default)]
    release_date: Option<String>, // movies
    #[serde(default)]
    first_air_date: Option<String>, // shows
    #[serde(default)]
    vote_average: Option<f32>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    genres: Vec<Genre>,
    #[serde(default)]
    imdb_id: Option<String>, // present on movie details
    #[serde(default)]
    external_ids: Option<ExternalIds>, // appended (carries imdb_id for shows)
    #[serde(default)]
    credits: Option<Credits>, // appended (cast + crew)
    #[serde(default)]
    created_by: Vec<CreatedBy>, // TV series creators (top-level on show details)
    #[serde(default)]
    images: Option<Images>, // appended (logos)
    #[serde(default)]
    keywords: Option<Keywords>, // appended (thematic tags)
}

/// Appended `keywords` block. Movies nest the list under `keywords`, TV under
/// `results` only one is ever populated, so flattening both is safe.
#[derive(Debug, Deserialize)]
struct Keywords {
    #[serde(default)]
    keywords: Vec<KeywordEntry>,
    #[serde(default)]
    results: Vec<KeywordEntry>,
}

#[derive(Debug, Deserialize)]
struct KeywordEntry {
    #[serde(default)]
    name: String,
}

/// Flatten a TMDB keywords block into a capped list of non-empty tag names.
fn collect_keywords(k: Keywords) -> Vec<String> {
    k.keywords
        .into_iter()
        .chain(k.results)
        .map(|e| e.name)
        .filter(|n| !n.is_empty())
        .take(MAX_KEYWORDS)
        .collect()
}

#[derive(Debug, Deserialize)]
struct Genre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Images {
    #[serde(default)]
    logos: Vec<ImageEntry>,
}

#[derive(Debug, Deserialize)]
struct ImageEntry {
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default, rename = "iso_639_1")]
    lang: Option<String>,
    #[serde(default)]
    vote_average: Option<f32>,
}

/// Best title logo `file_path`: PNG only, preferring the configured language,
/// then English, then language-neutral; ties broken by TMDB vote.
fn pick_logo(logos: &[ImageEntry], lang2: &str) -> Option<String> {
    let rank = |l: &ImageEntry| -> u8 {
        match l.lang.as_deref() {
            Some(x) if x == lang2 => 0,
            Some("en") => 1,
            None | Some("") => 2,
            _ => 3,
        }
    };
    logos
        .iter()
        .filter(|l| l.file_path.as_deref().is_some_and(|p| p.ends_with(".png")))
        .min_by(|a, b| {
            rank(a).cmp(&rank(b)).then(
                b.vote_average
                    .unwrap_or(0.0)
                    .partial_cmp(&a.vote_average.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        })
        .and_then(|l| l.file_path.clone())
}

#[derive(Debug, Deserialize)]
struct ExternalIds {
    #[serde(default)]
    imdb_id: Option<String>,
    /// TheTVDB series id (present on TV external_ids; absent for movies).
    #[serde(default)]
    tvdb_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Credits {
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
struct CreatedBy {
    #[serde(default)]
    name: String,
}

/// Map TMDB crew + TV creators into our capped, deduped [`CrewMember`] list:
/// authorship roles only, directors/creators first, one entry per person (their
/// most senior role wins).
fn map_crew(crew: Vec<RawCrew>, created_by: Vec<CreatedBy>) -> Vec<CrewMember> {
    let rank = |job: &str| KEY_CREW_JOBS.iter().position(|j| *j == job).unwrap_or(usize::MAX);
    let mut candidates: Vec<(usize, CrewMember)> = crew
        .into_iter()
        .filter(|c| !c.name.is_empty() && KEY_CREW_JOBS.contains(&c.job.as_str()))
        .map(|c| (rank(&c.job), CrewMember { name: c.name, job: c.job, profile_url: None }))
        .collect();
    // TV creators (no crew "Director" on series) → treat as "Creator".
    for cb in created_by.into_iter().filter(|c| !c.name.is_empty()) {
        candidates.push((rank("Creator"), CrewMember { name: cb.name, job: "Creator".into(), profile_url: None }));
    }
    // Most senior role first; keep one row per person.
    candidates.sort_by_key(|(r, _)| *r);
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|(_, m)| seen.insert(m.name.clone()))
        .map(|(_, m)| m)
        .take(MAX_CREW)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Network is not exercised in tests; we validate the JSON→Metadata mapping
    // against representative TMDB payloads instead.
    #[test]
    fn parses_movie_details() {
        let raw = r#"{
            "id": 542178,
            "title": "The French Dispatch",
            "tagline": "Read all about it.",
            "overview": "A love letter to journalists.",
            "release_date": "2021-10-21",
            "vote_average": 7.4,
            "poster_path": "/poster.jpg",
            "backdrop_path": "/back.jpg",
            "genres": [{"id": 35, "name": "Comedy"}, {"id": 18, "name": "Drama"}],
            "imdb_id": "tt8847712",
            "external_ids": {"imdb_id": "tt8847712"}
        }"#;
        let d: Details = serde_json::from_str(raw).unwrap();
        assert_eq!(d.id, 542178);
        assert_eq!(d.title.as_deref(), Some("The French Dispatch"));
        assert_eq!(d.imdb_id.as_deref(), Some("tt8847712"));
        assert_eq!(d.genres.len(), 2);
        assert_eq!(d.vote_average, Some(7.4));
    }

    #[test]
    fn parses_tv_details_with_external_ids() {
        let raw = r#"{
            "id": 1399,
            "name": "Game of Thrones",
            "overview": "Seven noble families fight.",
            "first_air_date": "2011-04-17",
            "vote_average": 8.4,
            "poster_path": "/got.jpg",
            "genres": [{"id": 10765, "name": "Sci-Fi & Fantasy"}],
            "external_ids": {"imdb_id": "tt0944947"}
        }"#;
        let d: Details = serde_json::from_str(raw).unwrap();
        assert_eq!(d.name.as_deref(), Some("Game of Thrones"));
        assert!(d.title.is_none());
        assert_eq!(
            d.external_ids.and_then(|e| e.imdb_id).as_deref(),
            Some("tt0944947")
        );
    }

    #[test]
    fn parses_appended_credits() {
        let raw = r#"{
            "id": 1,
            "title": "X",
            "credits": {
                "cast": [
                    {"name": "Bravo", "character": "B", "order": 1},
                    {"name": "Alpha", "character": "A", "order": 0},
                    {"name": "NoChar", "character": "", "order": 2}
                ]
            }
        }"#;
        let d: Details = serde_json::from_str(raw).unwrap();
        let mut cast = d.credits.unwrap().cast;
        cast.sort_by_key(|m| m.order.unwrap_or(u32::MAX));
        assert_eq!(cast[0].name, "Alpha");
        assert_eq!(cast[0].character.as_deref(), Some("A"));
        // Empty character strings are dropped during the Metadata mapping.
        assert_eq!(cast[2].character.as_deref(), Some(""));
    }

    #[test]
    fn empty_search_results_deserialize() {
        let s: SearchResp = serde_json::from_str(r#"{"results": []}"#).unwrap();
        assert!(s.results.is_empty());
    }

    #[test]
    fn parses_season_episode_stills() {
        let raw = r#"{
            "episodes": [
                {"episode_number": 1, "still_path": "/s1.jpg", "name": "Pilot", "overview": "It begins.", "air_date": "2022-02-18", "vote_average": 8.1},
                {"episode_number": 2, "name": "Half Loop", "overview": ""}
            ]
        }"#;
        let s: SeasonResp = serde_json::from_str(raw).unwrap();
        assert_eq!(s.episodes.len(), 2);
        assert_eq!(s.episodes[0].episode_number, 1);
        assert_eq!(s.episodes[0].still_path.as_deref(), Some("/s1.jpg"));
        assert!(s.episodes[1].still_path.is_none());
    }

    #[test]
    fn collects_movie_and_tv_keywords() {
        // Movies nest under `keywords`.
        let movie: Keywords =
            serde_json::from_str(r#"{"keywords":[{"id":1,"name":"road movie"},{"id":2,"name":"summer"}]}"#)
                .unwrap();
        assert_eq!(collect_keywords(movie), vec!["road movie", "summer"]);
        // Shows nest under `results`.
        let tv: Keywords =
            serde_json::from_str(r#"{"results":[{"id":3,"name":"dystopia"}]}"#).unwrap();
        assert_eq!(collect_keywords(tv), vec!["dystopia"]);
    }
}
