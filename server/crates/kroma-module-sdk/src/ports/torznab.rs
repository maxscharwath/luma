//! The Torznab search protocol contract: the endpoint config, query/result/caps
//! types, and the `TorznabPort` a consumer resolves to run searches. Lives here
//! so the indexer / acquisition modules use the types + port instead of naming
//! the torznab crate. The torznab crate implements the port over HTTP.

use serde::{Deserialize, Serialize};

/// Torznab category: movies. Sub-categories (2040 HD, 2045 UHD...) are the
/// indexer's business; the coarse bucket is what we ask for by default.
pub const CAT_MOVIES: u32 = 2000;
/// Torznab category: TV.
pub const CAT_TV: u32 = 5000;

/// A configured Torznab endpoint (crate-owned config type; the server maps its
/// DB row into this).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndexerEndpoint {
    /// Base URL up to and including the torznab api path, e.g.
    /// `http://nas:9117/api/v2.0/indexers/xyz/results/torznab`.
    pub url: String,
    pub api_key: String,
    pub categories: Vec<u32>,
}

/// One search request. Build via the constructors so the query strategy
/// (id-based first, free-text fallback) stays in one place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Caps {
    pub search_tmdb: bool,
    pub search_imdb: bool,
    pub tv_search_tmdb: bool,
    pub server_title: Option<String>,
}

/// Runs Torznab searches for a configured endpoint. Implemented by the torznab
/// crate (over HTTP/XML) and resolved via `kroma_module_host::resolve_port`.
pub trait TorznabPort: Send + Sync {
    /// Fetch `t=caps` (also the admin test-connection call).
    fn caps(&self, endpoint: &IndexerEndpoint) -> anyhow::Result<Caps>;
    /// Run one query against one indexer and normalize the results.
    fn search(
        &self,
        endpoint: &IndexerEndpoint,
        query: &Query,
        caps: &Caps,
    ) -> anyhow::Result<Vec<Release>>;
}
