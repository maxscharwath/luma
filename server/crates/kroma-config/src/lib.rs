//! Runtime configuration, sourced entirely from environment variables with
//! sensible defaults so the server runs out-of-the-box.

use std::env;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

/// Built-in TMDB API key, so metadata works with zero configuration the way
/// Overseerr/Jellyseerr/Seerr ship one shared application key. Per-install
/// `KROMA_TMDB_API_KEY` overrides it. Empty here = feature stays off until a key
/// is provided. Register a free key at <https://www.themoviedb.org/settings/api>
/// and paste it below (it's a public app key, fine to commit).
const BUILTIN_TMDB_API_KEY: &str = "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJiYjI2M2YzMGNlNGY5MjJjYzkxODAwMTc4NzIyYmQ2ZiIsIm5iZiI6MTU1NTQyMzg5MS4yNDg5OTk4LCJzdWIiOiI1Y2I1ZTI5MzBlMGEyNjZiOWJlZDJjNTEiLCJzY29wZXMiOlsiYXBpX3JlYWQiXSwidmVyc2lvbiI6MX0.n7C78ISAFNtk1To3rCSqwdGcM2c72jPslotoU3UCtxc";

/// Resolved server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// Library roots to scan. May be empty (demo seed kicks in then).
    pub media_dirs: Vec<PathBuf>,
    /// Movie-only library roots (`KROMA_MOVIES_DIRS`). Seed a typed "Films"
    /// library on first run. May be empty.
    pub movies_dirs: Vec<PathBuf>,
    /// TV/show-only library roots (`KROMA_SERIES_DIRS`). Seed a typed "Séries"
    /// library on first run. May be empty.
    pub series_dirs: Vec<PathBuf>,
    /// Where `library.json` is cached.
    pub data_dir: PathBuf,
    /// TMDB API key for metadata enrichment. `None` disables the feature.
    pub tmdb_api_key: Option<String>,
    /// TMDB language tag for titles/overviews, e.g. `en-US`, `fr-FR`.
    pub tmdb_language: String,
    /// Enrich the catalog with TMDB art during scans (background). Default on
    /// when a key is present; set `KROMA_TMDB_ENRICH=0` to disable.
    pub tmdb_enrich: bool,
    /// Public base URL of the web app (`KROMA_WEB_URL`), used to build the Quick
    /// Connect QR target (`<web>/connect?code=…`). `None` → the device shows the
    /// numeric code only (no QR).
    pub web_url: Option<String>,
    /// Directory of the built web SPA (`KROMA_WEB_DIR`) to serve on the same origin
    /// as the API (the single-binary deploy, e.g. the Synology package). `None` →
    /// API only (dev, where the web runs on its own Vite server).
    pub web_dir: Option<PathBuf>,
}

impl Config {
    /// Build configuration from the process environment.
    pub fn from_env() -> Self {
        let host = env::var("KROMA_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = env::var("KROMA_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(4040);

        let media_dirs = env::var("KROMA_MEDIA_DIRS")
            .ok()
            .map(|raw| parse_dir_list(&raw))
            .unwrap_or_default();

        let movies_dirs = env::var("KROMA_MOVIES_DIRS")
            .ok()
            .map(|raw| parse_dir_list(&raw))
            .unwrap_or_default();

        let series_dirs = env::var("KROMA_SERIES_DIRS")
            .ok()
            .map(|raw| parse_dir_list(&raw))
            .unwrap_or_default();

        let data_dir = env::var("KROMA_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        // Explicit env override wins; otherwise fall back to the built-in key so
        // metadata works out of the box. Empty in both → feature off.
        let tmdb_api_key = env::var("KROMA_TMDB_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let builtin = BUILTIN_TMDB_API_KEY.trim();
                (!builtin.is_empty()).then(|| builtin.to_string())
            });

        let tmdb_language =
            env::var("KROMA_TMDB_LANGUAGE").unwrap_or_else(|_| "en-US".to_string());

        let tmdb_enrich = env::var("KROMA_TMDB_ENRICH")
            .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off"))
            .unwrap_or(true);

        let web_url = env::var("KROMA_WEB_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        let web_dir = env::var("KROMA_WEB_DIR")
            .ok()
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| !p.as_os_str().is_empty() && p.join("_shell.html").is_file());

        Config {
            host,
            port,
            media_dirs,
            movies_dirs,
            series_dirs,
            data_dir,
            tmdb_api_key,
            tmdb_language,
            tmdb_enrich,
            web_url,
            web_dir,
        }
    }

    /// The socket address to bind. Falls back to `0.0.0.0` if the host string
    /// does not parse as an IP.
    pub fn socket_addr(&self) -> SocketAddr {
        let ip: IpAddr = self.host.parse().unwrap_or_else(|_| {
            tracing::warn!(host = %self.host, "KROMA_HOST is not a valid IP; binding 0.0.0.0");
            IpAddr::from([0, 0, 0, 0])
        });
        SocketAddr::new(ip, self.port)
    }

    /// Path to the SQLite database file.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("kroma.db")
    }

    /// Directory for rolling log files.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

/// Split a directory list on `:`, `;`, or `,`. On Unix we accept `;` in addition
/// to the native `:` so a user who types semicolons (natural, and what the NAS
/// install wizard's examples invite) still gets the directories split correctly
/// rather than one bogus combined path. Empty / whitespace entries are dropped.
fn parse_dir_list(raw: &str) -> Vec<PathBuf> {
    let seps: &[char] = if cfg!(windows) { &[';', ','] } else { &[':', ';', ','] };
    raw.split(seps)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}
