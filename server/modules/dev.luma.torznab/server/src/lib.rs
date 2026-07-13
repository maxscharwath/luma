//! Torznab client: the Newznab-derived HTTP/XML API spoken by Jackett and
//! Prowlarr, which is how LUMA searches torrent indexers without knowing any
//! tracker's private dialect. One [`IndexerEndpoint`] per configured indexer;
//! [`search`] normalizes the RSS answer into [`Release`]s for the decision
//! engine (`luma-scene`), and [`caps`] doubles as the test-connection call.
//!
//! The public surface is stable from day one; the transport + XML parsing land
//! with the indexer milestone.


// The Torznab types now live in the SDK ports module (luma_module_sdk::ports) (so indexer / acquisition use
// them without depending on this crate); re-exported here for this crate's fns.
pub use luma_module_sdk::ports::{
    Caps, IndexerEndpoint, Query, Release, CAT_MOVIES, CAT_TV,
};

mod xml;

/// Network budget per Torznab call: trackers behind Jackett can be slow.
const MAX_TIME_SECS: u32 = 40;

fn fetch_xml(endpoint: &IndexerEndpoint, params: &[(&str, String)]) -> anyhow::Result<Vec<u8>> {
    let mut req = luma_module_sdk::http::Fetch::new().max_time(MAX_TIME_SECS);
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

pub mod module;
pub use module::MODULE;

/// The Torznab port implementation: a stateless engine the composition root
/// registers so consumer modules resolve `luma_module_sdk::ports::TorznabPort`.
pub struct TorznabEngine;

impl luma_module_sdk::ports::TorznabPort for TorznabEngine {
    fn caps(&self, endpoint: &IndexerEndpoint) -> anyhow::Result<Caps> {
        caps(endpoint)
    }

    fn search(
        &self,
        endpoint: &IndexerEndpoint,
        query: &Query,
        caps: &Caps,
    ) -> anyhow::Result<Vec<Release>> {
        search(endpoint, query, caps)
    }
}
