//! Native Cardigann indexer engine.
//!
//! LUMA's acquisition stack normally talks Torznab to an external Jackett /
//! Prowlarr instance (`luma-torznab`). This crate is the alternative: it runs
//! the same community-maintained Cardigann YAML *definitions* those aggregators
//! use, directly - parsing a tracker's HTML/JSON, driving its login, and
//! resolving its download links - so an admin can search real trackers without
//! standing up a second service.
//!
//! The definitions themselves are GPL and are **not** vendored into this
//! MIT-licensed repo; the [`store`] module fetches them at runtime on the end
//! user's machine (see the crate-level design notes in the acquisition docs).
//!
//! Public surface mirrors [`luma_torznab`] on purpose ([`Query`], [`Release`],
//! [`Caps`]) so the acquisition service can dispatch to either engine behind one
//! interface.
//!
//! ## Layout
//! - [`definition`] - the Cardigann YAML schema.
//! - `template` - the Go-template subset definitions use (`{{ .Keywords }}`…).
//! - `filters` - the field/keyword filter pipeline (`re_replace`, `dateparse`…).
//! - `selector` - CSS (and optional XPath) element selection + field extraction.
//! - `engine` - request building, row iteration, field extraction into releases.
//! - `session` - per-indexer cookie jar + login flows.
//! - `store` - runtime fetch/cache of the definition set.

use serde::{Deserialize, Serialize};

pub mod category;
pub mod context;
pub mod db;
pub mod definition;
pub mod dtos;
pub mod engine;
pub mod filters;
pub mod module;
pub mod selector;
pub mod session;
pub mod store;
pub mod admin;
pub mod routes;
pub mod template;
pub mod xmltree;
#[cfg(feature = "xpath")]
pub mod xpath;

pub use dtos::*;

pub use session::{DownloadTarget, SearchOutcome, Session};

pub use definition::Definition;
pub use module::MODULE;

/// This module's id (matches its `module.json`).
pub const MODULE_ID: &str = "dev.luma.indexer";

/// The Indexers sub-module: exposes the native-engine admin routes over the
/// HostCtx seam. Lifecycle-free (disabling it just gates its routes off).
pub struct IndexersModule;

#[luma_module_host::async_trait]
impl<S: luma_module_host::HostCtx + Clone + Send + Sync + 'static>
    luma_module_host::ServerModule<S> for IndexersModule
{
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn migrations(&self) -> &'static str {
        db::MIGRATIONS
    }

    fn admin_routes(&self, _host: &S) -> Option<axum::Router<S>> {
        Some(routes::routes::<S>())
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module<S: luma_module_host::HostCtx + Clone + Send + Sync + 'static>(
) -> Box<dyn luma_module_host::ServerModule<S>> {
    Box::new(IndexersModule)
}

/// The [`TorrentFetchPort`](luma_contracts::TorrentFetchPort) impl: fetch a
/// `.torrent` through a built-in Cardigann indexer's authenticated session. The
/// composition root registers it so the downloads module can grab private-tracker
/// files without depending on this crate.
pub struct IndexerTorrentFetch;

impl luma_contracts::TorrentFetchPort for IndexerTorrentFetch {
    fn fetch_torrent(
        &self,
        host: &dyn luma_module_host::HostCtx,
        indexer_id: &str,
        url: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let conn = match host.db().get() {
            Ok(conn) => conn,
            Err(e) => return Some(Err(e.into())),
        };
        let row = match crate::db::get_indexer(&conn, indexer_id) {
            Ok(Some(row)) => row,
            Ok(None) => return None,
            Err(e) => return Some(Err(e.into())),
        };
        drop(conn);
        // Only built-in (native Cardigann) indexers cookie-gate downloads; a
        // Torznab / manual grab is handled by the caller's plain fetch.
        if row.kind != admin::KIND_BUILTIN {
            return None;
        }
        Some((|| {
            let session = admin::builtin_session(host, &row)?;
            session.fetch_torrent(url)
        })())
    }
}

/// A configured built-in indexer: the chosen base link plus the admin-entered
/// settings (`.Config.<name>` resolves against this, falling back to the
/// definition's setting defaults).
#[derive(Debug, Clone, Default)]
pub struct IndexerConfig {
    /// Base site URL, with trailing slash (e.g. `https://1337x.to/`). Chosen
    /// from the definition's `links` (or an admin override).
    pub base_url: String,
    /// Setting name -> configured value (username, password, toggles, selects).
    pub settings: std::collections::HashMap<String, String>,
}

/// One search request. Mirrors [`luma_torznab::Query`] so the acquisition layer
/// builds one query shape for both engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Movie { tmdb_id: Option<u64>, imdb_id: Option<String>, title: String, year: Option<u32> },
    Episode { tmdb_id: Option<u64>, title: String, season: u32, episode: u32 },
    Season { tmdb_id: Option<u64>, title: String, season: u32 },
    /// Free-text (manual admin search).
    Text { query: String },
}

impl Query {
    /// The free-text keywords a definition's `{{ .Keywords }}` expands to.
    pub fn keywords(&self) -> String {
        match self {
            Query::Movie { title, year, .. } => match year {
                Some(y) => format!("{title} {y}"),
                None => title.clone(),
            },
            Query::Episode { title, season, episode, .. } => {
                format!("{title} S{season:02}E{episode:02}")
            }
            Query::Season { title, season, .. } => format!("{title} S{season:02}"),
            Query::Text { query } => query.clone(),
        }
    }
}

/// A normalized release, field-compatible with [`luma_torznab::Release`] plus
/// the richer attributes Cardigann exposes (categories, freeleech factors).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub title: String,
    pub guid: String,
    /// `.torrent` download URL, when present (may need the session cookie to
    /// fetch).
    pub link: Option<String>,
    pub magnet: Option<String>,
    pub info_hash: Option<String>,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub grabs: Option<u32>,
    pub tmdb_id: Option<u64>,
    pub imdb_id: Option<String>,
    pub published_at: Option<String>,
    pub details_url: Option<String>,
    /// Mapped Newznab category ids.
    pub categories: Vec<u32>,
    /// Freeleech / bonus multipliers (1.0 = normal). Feed the decision engine.
    pub download_volume_factor: Option<f64>,
    pub upload_volume_factor: Option<f64>,
}

/// What a definition advertises it can do, derived from `caps.modes`. Mirrors
/// [`luma_torznab::Caps`] so capability-aware query building is shared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caps {
    pub search_tmdb: bool,
    pub search_imdb: bool,
    pub tv_search_tmdb: bool,
    pub tv_search_season: bool,
    pub server_title: Option<String>,
}

impl Caps {
    /// Read capabilities out of a definition's `caps.modes`.
    pub fn from_definition(def: &Definition) -> Self {
        let has = |mode: &str, param: &str| {
            def.caps.modes.get(mode).is_some_and(|params| params.iter().any(|p| p == param))
        };
        Caps {
            search_imdb: has("movie-search", "imdbid") || has("search", "imdbid"),
            search_tmdb: has("movie-search", "tmdbid") || has("search", "tmdbid"),
            tv_search_tmdb: has("tv-search", "tmdbid"),
            tv_search_season: has("tv-search", "season"),
            server_title: Some(def.name.clone()),
        }
    }
}
