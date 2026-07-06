//! Torznab client: the Newznab-derived HTTP/XML API spoken by Jackett and
//! Prowlarr, which is how LUMA searches torrent indexers without knowing any
//! tracker's private dialect. One [`IndexerEndpoint`] per configured indexer;
//! [`search`] normalizes the RSS answer into [`Release`]s for the decision
//! engine (`luma-release`), and [`caps`] doubles as the test-connection call.
//!
//! The public surface is stable from day one; the transport + XML parsing land
//! with the indexer milestone.

use serde::{Deserialize, Serialize};

/// Torznab category: movies. Sub-categories (2040 HD, 2045 UHD...) are the
/// indexer's business; the coarse bucket is what we ask for by default.
pub const CAT_MOVIES: u32 = 2000;
/// Torznab category: TV.
pub const CAT_TV: u32 = 5000;

/// A configured Torznab endpoint (crate-owned config type; the server maps its
/// DB row into this).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexerEndpoint {
    /// Base URL up to and including the torznab api path, e.g.
    /// `http://nas:9117/api/v2.0/indexers/xyz/results/torznab`.
    pub url: String,
    pub api_key: String,
    pub categories: Vec<u32>,
}

/// One search request. Build via the constructors so the query strategy
/// (id-based first, free-text fallback) stays in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Movie { tmdb_id: Option<u64>, imdb_id: Option<String>, title: String, year: Option<u32> },
    Episode { tmdb_id: Option<u64>, title: String, season: u32, episode: u32 },
    Season { tmdb_id: Option<u64>, title: String, season: u32 },
}

/// A normalized Torznab result item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub title: String,
    pub guid: String,
    /// `.torrent` download URL (Jackett proxies these), when present.
    pub link: Option<String>,
    /// `torznab:attr magneturl`, when present.
    pub magnet: Option<String>,
    pub info_hash: Option<String>,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub tmdb_id: Option<u64>,
    pub imdb_id: Option<String>,
    /// RFC 2822 `pubDate`, unparsed (age display only).
    pub published_at: Option<String>,
    /// The tracker's human-viewable torrent page (RSS `<comments>`, else the
    /// details `<guid>` when it is an http URL). Sonarr/Radarr's "info" link.
    pub details_url: Option<String>,
}

/// What an indexer advertises via `t=caps`: which query parameters its
/// backing tracker actually understands (not all support `tmdbid`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caps {
    pub search_tmdb: bool,
    pub search_imdb: bool,
    pub tv_search_tmdb: bool,
    pub server_title: Option<String>,
}

mod xml;

/// Network budget per Torznab call: trackers behind Jackett can be slow.
const MAX_TIME_SECS: u32 = 40;

fn fetch_xml(endpoint: &IndexerEndpoint, params: &[(&str, String)]) -> anyhow::Result<Vec<u8>> {
    let mut req = luma_fetch::Fetch::new().max_time(MAX_TIME_SECS);
    if !endpoint.api_key.is_empty() {
        req = req.query("apikey", endpoint.api_key.clone());
    }
    for (k, v) in params {
        req = req.query(k, v.clone());
    }
    Ok(req.get(&endpoint.url)?.ensure_ok()?.body)
}

/// Fetch `t=caps` (also the admin test-connection call).
pub fn caps(endpoint: &IndexerEndpoint) -> anyhow::Result<Caps> {
    let body = fetch_xml(endpoint, &[("t", "caps".to_string())])?;
    xml::parse_caps(&body)
}

/// Run one query against one indexer and normalize the results.
///
/// Attempts the strongest parameter set the indexer supports first (tmdb id,
/// then imdb id, then free text) and falls back on an empty answer: not every
/// tracker behind Jackett resolves external ids, and an id miss must not hide
/// releases a text query would find.
pub fn search(endpoint: &IndexerEndpoint, query: &Query, caps: &Caps) -> anyhow::Result<Vec<Release>> {
    let cats = endpoint
        .categories
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut last_err: Option<anyhow::Error> = None;
    for mut params in attempts(query, caps) {
        if !cats.is_empty() {
            params.push(("cat", cats.clone()));
        }
        match fetch_xml(endpoint, &params).and_then(|body| xml::parse_items(&body)) {
            Ok(items) if !items.is_empty() => return Ok(items),
            Ok(_) => {}
            Err(e) => last_err = Some(e),
        }
    }
    match last_err {
        // Every attempt errored: surface it (auth/network problems must not
        // read as "no releases found").
        Some(e) => Err(e),
        None => Ok(Vec::new()),
    }
}

/// The ordered parameter sets to try for a query, strongest first.
fn attempts(query: &Query, caps: &Caps) -> Vec<Vec<(&'static str, String)>> {
    let mut out: Vec<Vec<(&'static str, String)>> = Vec::new();
    match query {
        Query::Movie { tmdb_id, imdb_id, title, year } => {
            if caps.search_tmdb {
                if let Some(id) = tmdb_id {
                    out.push(vec![("t", "movie".into()), ("tmdbid", id.to_string())]);
                }
            }
            if caps.search_imdb {
                if let Some(imdb) = imdb_id {
                    // Torznab wants the bare number, without the tt prefix.
                    let bare = imdb.trim_start_matches("tt").to_string();
                    out.push(vec![("t", "movie".into()), ("imdbid", bare)]);
                }
            }
            let q = match year {
                Some(y) => format!("{title} {y}"),
                None => title.clone(),
            };
            out.push(vec![("t", "search".into()), ("q", q)]);
        }
        Query::Episode { tmdb_id, title, season, episode } => {
            if caps.tv_search_tmdb {
                if let Some(id) = tmdb_id {
                    out.push(vec![
                        ("t", "tvsearch".into()),
                        ("tmdbid", id.to_string()),
                        ("season", season.to_string()),
                        ("ep", episode.to_string()),
                    ]);
                }
            }
            out.push(vec![
                ("t", "search".into()),
                ("q", format!("{title} S{season:02}E{episode:02}")),
            ]);
        }
        Query::Season { tmdb_id, title, season } => {
            if caps.tv_search_tmdb {
                if let Some(id) = tmdb_id {
                    out.push(vec![
                        ("t", "tvsearch".into()),
                        ("tmdbid", id.to_string()),
                        ("season", season.to_string()),
                    ]);
                }
            }
            out.push(vec![("t", "search".into()), ("q", format!("{title} S{season:02}"))]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_order_ids_first_then_text_fallback() {
        let caps = Caps { search_tmdb: true, search_imdb: true, ..Caps::default() };
        let q = Query::Movie {
            tmdb_id: Some(603),
            imdb_id: Some("tt0133093".into()),
            title: "The Matrix".into(),
            year: Some(1999),
        };
        let a = attempts(&q, &caps);
        assert_eq!(a.len(), 3);
        assert!(a[0].contains(&("tmdbid", "603".to_string())));
        assert!(a[1].contains(&("imdbid", "0133093".to_string())), "tt prefix stripped");
        assert!(a[2].contains(&("q", "The Matrix 1999".to_string())));

        // Without id caps only the text attempt remains.
        let a = attempts(&q, &Caps::default());
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn episode_and_season_attempts() {
        let caps = Caps { tv_search_tmdb: true, ..Caps::default() };
        let q = Query::Episode { tmdb_id: Some(1396), title: "Breaking Bad".into(), season: 1, episode: 2 };
        let a = attempts(&q, &caps);
        assert_eq!(a.len(), 2);
        assert!(a[0].contains(&("ep", "2".to_string())));
        assert!(a[1].contains(&("q", "Breaking Bad S01E02".to_string())));

        let q = Query::Season { tmdb_id: None, title: "Breaking Bad".into(), season: 3 };
        let a = attempts(&q, &caps);
        assert_eq!(a.len(), 1);
        assert!(a[0].contains(&("q", "Breaking Bad S03".to_string())));
    }
}
