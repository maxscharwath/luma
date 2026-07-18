//! Native Cardigann indexer engine.
//!
//! KROMA's acquisition stack normally talks Torznab to an external Jackett /
//! Prowlarr instance (`kroma-torznab`). This crate is the alternative: it runs
//! the same community-maintained Cardigann YAML *definitions* those aggregators
//! use, directly - parsing a tracker's HTML/JSON, driving its login, and
//! resolving its download links - so an admin can search real trackers without
//! standing up a second service.
//!
//! The definitions themselves are GPL and are **not** vendored into this
//! MIT-licensed repo; the [`store`] module fetches them at runtime on the end
//! user's machine (see the crate-level design notes in the acquisition docs).
//!
//! Public surface mirrors [`kroma_torznab`] on purpose ([`Query`], [`Release`],
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
pub const MODULE_ID: &str = "tv.kroma.indexer";

/// The Indexers sub-module: exposes the native-engine admin routes over the
/// HostCtx seam. Lifecycle-free (disabling it just gates its routes off).
pub struct IndexersModule;

#[kroma_module_sdk::host::async_trait]
impl<S: kroma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>
    kroma_module_sdk::host::ServerModule<S> for IndexersModule
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
pub fn server_module<S: kroma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>(
) -> Box<dyn kroma_module_sdk::host::ServerModule<S>> {
    Box::new(IndexersModule)
}

/// The [`TorrentFetchPort`](kroma_module_sdk::ports::TorrentFetchPort) impl: fetch a
/// `.torrent` through a built-in Cardigann indexer's authenticated session. The
/// composition root registers it so the downloads module can grab private-tracker
/// files without depending on this crate.
pub struct IndexerTorrentFetch;

impl kroma_module_sdk::ports::TorrentFetchPort for IndexerTorrentFetch {
    fn fetch_torrent(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
        indexer_id: &str,
        url: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let conn = match host.db().get() {
            Ok(conn) => conn,
            Err(e) => return Some(Err(e)),
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

/// One search request. Mirrors [`kroma_torznab::Query`] so the acquisition layer
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

/// A normalized release, field-compatible with [`kroma_torznab::Release`] plus
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
/// [`kroma_torznab::Caps`] so capability-aware query building is shared.
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

/// The IndexerDbPort implementation (stateless): reads/updates the `indexers`
/// table through the host DB pool, so the downloads queue view + acquisition
/// resolve it instead of depending on this crate.
pub struct IndexerDb;

impl kroma_module_sdk::ports::IndexerDbPort for IndexerDb {
    fn list_indexers(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
    ) -> anyhow::Result<Vec<kroma_module_sdk::ports::IndexerRow>> {
        let conn = host.db().get()?;
        Ok(db::list_indexers(&conn)?)
    }

    fn enabled_indexers(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
    ) -> anyhow::Result<Vec<kroma_module_sdk::ports::IndexerRow>> {
        let conn = host.db().get()?;
        Ok(db::enabled_indexers(&conn)?)
    }

    fn get_indexer(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
        id: &str,
    ) -> anyhow::Result<Option<kroma_module_sdk::ports::IndexerRow>> {
        let conn = host.db().get()?;
        Ok(db::get_indexer(&conn, id)?)
    }

    fn note_indexer_result(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
        id: &str,
        ok: bool,
        error: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        db::note_indexer_result(host.db(), id, ok, error, now_ms)
    }
}

/// The IndexerSearchPort implementation: runs native (Cardigann) searches and
/// resolves grab targets, hiding the stateful `Session` + the indexer's richer
/// native types behind the SDK contract shapes. The query/release converters
/// (formerly in acquisition) live here now.
pub struct IndexerSearch;

impl kroma_module_sdk::ports::IndexerSearchPort for IndexerSearch {
    fn search(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
        row: &kroma_module_sdk::ports::IndexerRow,
        query: &kroma_module_sdk::ports::Query,
        categories: &[u32],
    ) -> anyhow::Result<kroma_module_sdk::ports::SearchOutcome> {
        if row.kind == admin::KIND_BUILTIN {
            let session = admin::builtin_session(host, row)?;
            let outcome = session.search(&to_native_query(query), categories);
            Ok(kroma_module_sdk::ports::SearchOutcome {
                releases: outcome.releases.into_iter().map(release_to_port).collect(),
                errors: outcome.errors,
            })
        } else {
            // External Torznab endpoint: build it from the row + cached caps and
            // resolve the Torznab engine port.
            let caps = admin::indexer_caps(host, row)?;
            let endpoint = admin::endpoint_of(row);
            let tz = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::TorznabPort>(host)
                .ok_or_else(|| anyhow::anyhow!("torznab search engine unavailable"))?;
            let releases = tz.search(&endpoint, query, &caps)?;
            Ok(kroma_module_sdk::ports::SearchOutcome { releases, errors: Vec::new() })
        }
    }

    fn resolve_download(
        &self,
        host: &dyn kroma_module_sdk::host::HostCtx,
        row: &kroma_module_sdk::ports::IndexerRow,
        title: &str,
        details_url: Option<&str>,
        magnet_or_url: &str,
    ) -> anyhow::Result<kroma_module_sdk::ports::DownloadTarget> {
        if magnet_or_url.starts_with("magnet:") {
            return Ok(kroma_module_sdk::ports::DownloadTarget::Magnet(magnet_or_url.to_string()));
        }
        let session = admin::builtin_session(host, row)?;
        let release = Release {
            title: title.to_string(),
            magnet: magnet_or_url.starts_with("magnet:").then(|| magnet_or_url.to_string()),
            link: magnet_or_url.starts_with("http").then(|| magnet_or_url.to_string()),
            details_url: details_url.map(str::to_string),
            ..Default::default()
        };
        Ok(match session.resolve_download(&release)? {
            DownloadTarget::Magnet(m) => kroma_module_sdk::ports::DownloadTarget::Magnet(m),
            DownloadTarget::TorrentUrl(u) => kroma_module_sdk::ports::DownloadTarget::TorrentUrl(u),
        })
    }
}

/// Map an SDK query shape onto the indexer's native query.
fn to_native_query(q: &kroma_module_sdk::ports::Query) -> Query {
    match q {
        kroma_module_sdk::ports::Query::Movie { tmdb_id, imdb_id, title, year } => Query::Movie {
            tmdb_id: *tmdb_id,
            imdb_id: imdb_id.clone(),
            title: title.clone(),
            year: *year,
        },
        kroma_module_sdk::ports::Query::Episode { tmdb_id, title, season, episode } => {
            Query::Episode { tmdb_id: *tmdb_id, title: title.clone(), season: *season, episode: *episode }
        }
        kroma_module_sdk::ports::Query::Season { tmdb_id, title, season } => {
            Query::Season { tmdb_id: *tmdb_id, title: title.clone(), season: *season }
        }
    }
}

/// Normalize a native release into the SDK release shape the scoring pipeline uses.
fn release_to_port(r: Release) -> kroma_module_sdk::ports::Release {
    kroma_module_sdk::ports::Release {
        title: r.title,
        guid: r.guid,
        link: r.link,
        magnet: r.magnet,
        info_hash: r.info_hash,
        size_bytes: r.size_bytes,
        seeders: r.seeders,
        leechers: r.leechers,
        tmdb_id: r.tmdb_id,
        imdb_id: r.imdb_id,
        published_at: r.published_at,
        details_url: r.details_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Query::keywords --------------------------------------------------------

    #[test]
    fn keywords_render_per_query_kind() {
        assert_eq!(
            Query::Movie { tmdb_id: None, imdb_id: None, title: "Dune".into(), year: Some(2021) }
                .keywords(),
            "Dune 2021"
        );
        assert_eq!(
            Query::Movie { tmdb_id: None, imdb_id: None, title: "Heat".into(), year: None }
                .keywords(),
            "Heat"
        );
        assert_eq!(
            Query::Episode { tmdb_id: None, title: "Breaking Bad".into(), season: 1, episode: 2 }
                .keywords(),
            "Breaking Bad S01E02"
        );
        assert_eq!(
            Query::Season { tmdb_id: None, title: "Breaking Bad".into(), season: 3 }.keywords(),
            "Breaking Bad S03"
        );
        assert_eq!(Query::Text { query: "free text".into() }.keywords(), "free text");
    }

    // ----- Caps::from_definition --------------------------------------------------

    fn def_with_modes(modes_yaml: &str) -> Definition {
        let yaml = format!(
            r#"
id: t
name: The Tracker
caps:
  modes:
{modes_yaml}
search:
  rows:
    selector: "tr"
"#
        );
        crate::definition::parse(yaml.as_bytes()).unwrap()
    }

    #[test]
    fn caps_from_definition_reads_modes() {
        let def = def_with_modes(
            "    movie-search: [q, imdbid, tmdbid]\n    tv-search: [q, season, tmdbid]",
        );
        let caps = Caps::from_definition(&def);
        assert!(caps.search_imdb);
        assert!(caps.search_tmdb);
        assert!(caps.tv_search_tmdb);
        assert!(caps.tv_search_season);
        assert_eq!(caps.server_title.as_deref(), Some("The Tracker"));
    }

    #[test]
    fn caps_from_definition_search_mode_fallback() {
        // The generic `search` mode also grants imdb/tmdb id search.
        let def = def_with_modes("    search: [q, imdbid, tmdbid]");
        let caps = Caps::from_definition(&def);
        assert!(caps.search_imdb && caps.search_tmdb);
        // No tv-search mode -> tv flags stay off.
        assert!(!caps.tv_search_tmdb && !caps.tv_search_season);
    }

    #[test]
    fn caps_from_definition_no_modes_all_false() {
        let def = def_with_modes("    search: [q]");
        let caps = Caps::from_definition(&def);
        assert!(!caps.search_imdb && !caps.search_tmdb);
        assert!(!caps.tv_search_tmdb && !caps.tv_search_season);
        assert_eq!(caps.server_title.as_deref(), Some("The Tracker"));
    }

    // ----- query / release converters ---------------------------------------------

    #[test]
    fn to_native_query_maps_all_shapes() {
        use kroma_module_sdk::ports::Query as PQ;
        assert_eq!(
            to_native_query(&PQ::Movie {
                tmdb_id: Some(603),
                imdb_id: Some("tt0133093".into()),
                title: "The Matrix".into(),
                year: Some(1999),
            }),
            Query::Movie {
                tmdb_id: Some(603),
                imdb_id: Some("tt0133093".into()),
                title: "The Matrix".into(),
                year: Some(1999),
            }
        );
        assert_eq!(
            to_native_query(&PQ::Episode {
                tmdb_id: Some(1),
                title: "S".into(),
                season: 2,
                episode: 5,
            }),
            Query::Episode { tmdb_id: Some(1), title: "S".into(), season: 2, episode: 5 }
        );
        assert_eq!(
            to_native_query(&PQ::Season { tmdb_id: None, title: "S".into(), season: 4 }),
            Query::Season { tmdb_id: None, title: "S".into(), season: 4 }
        );
    }

    #[test]
    fn release_to_port_keeps_shared_fields() {
        let r = Release {
            title: "T".into(),
            guid: "g".into(),
            link: Some("l".into()),
            magnet: Some("m".into()),
            info_hash: Some("h".into()),
            size_bytes: Some(5),
            seeders: Some(3),
            leechers: Some(1),
            grabs: Some(9),
            tmdb_id: Some(2),
            imdb_id: Some("tt1".into()),
            published_at: Some("d".into()),
            details_url: Some("u".into()),
            categories: vec![2040],
            download_volume_factor: Some(0.5),
            upload_volume_factor: Some(1.0),
        };
        let p = release_to_port(r);
        assert_eq!(p.title, "T");
        assert_eq!(p.guid, "g");
        assert_eq!(p.link.as_deref(), Some("l"));
        assert_eq!(p.magnet.as_deref(), Some("m"));
        assert_eq!(p.info_hash.as_deref(), Some("h"));
        assert_eq!(p.size_bytes, Some(5));
        assert_eq!(p.seeders, Some(3));
        assert_eq!(p.leechers, Some(1));
        assert_eq!(p.tmdb_id, Some(2));
        assert_eq!(p.imdb_id.as_deref(), Some("tt1"));
        assert_eq!(p.published_at.as_deref(), Some("d"));
        assert_eq!(p.details_url.as_deref(), Some("u"));
    }
}
