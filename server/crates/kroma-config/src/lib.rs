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
///
/// `from_env` is the only real constructor; the derived `Default` exists so tests
/// can build a stub with `..Default::default()` and override just the fields they
/// exercise, instead of hand-listing every field (which broke on each new field).
/// Its defaults (`host: ""`, `port: 0`, no key) are deliberately non-functional.
#[derive(Debug, Clone, Default)]
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
    /// Connect QR target (`<web>/connect?code=…`). `None` → the admin "Remote
    /// access" public URL setting is used instead, and with neither the device
    /// falls back to its own server origin (which serves the SPA in production).
    pub web_url: Option<String>,
    /// Directory of the built web SPA (`KROMA_WEB_DIR`) to serve on the same origin
    /// as the API (the single-binary deploy, e.g. the Synology package). `None` →
    /// API only (dev, where the web runs on its own Vite server).
    pub web_dir: Option<PathBuf>,
    /// Force-enable the HTTPS listener regardless of the stored setting
    /// (`KROMA_HTTPS=1`). `None` = defer to the `httpsEnabled` setting; `Some`
    /// pins it either way (env wins over the admin toggle).
    pub https_override: Option<bool>,
    /// HTTPS port override (`KROMA_HTTPS_PORT`). `None` = use the `httpsPort`
    /// setting (default 4443). The plain-HTTP `port` keeps serving too.
    pub https_port_override: Option<u16>,
    /// Extra certificate SANs (`KROMA_TLS_SANS`, comma/space separated): a static
    /// LAN IP or custom hostname to add to the auto-generated self-signed cert,
    /// on top of the auto-detected localhost / hostname / primary LAN IP.
    pub tls_extra_sans: Vec<String>,
    /// Force the HTTP listener to redirect everything to HTTPS
    /// (`KROMA_HTTPS_REDIRECT=1`). `None` = defer to the `httpsRedirect` setting.
    /// Only takes effect when HTTPS is actually running; the cert-download route
    /// stays reachable over plain HTTP so a device can bootstrap trust first.
    pub https_redirect_override: Option<bool>,
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

        let https_override = env::var("KROMA_HTTPS")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off" | ""));

        let https_port_override = env::var("KROMA_HTTPS_PORT")
            .ok()
            .and_then(|p| p.trim().parse::<u16>().ok());

        let tls_extra_sans = env::var("KROMA_TLS_SANS")
            .ok()
            .map(|raw| {
                raw.split([',', ' ', ';'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let https_redirect_override = env::var("KROMA_HTTPS_REDIRECT")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off" | ""));

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
            https_override,
            https_port_override,
            tls_extra_sans,
            https_redirect_override,
        }
    }

    /// Directory holding the auto-generated TLS certificate + key.
    pub fn tls_dir(&self) -> PathBuf {
        self.data_dir.join("tls")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ----- pure helpers ----------------------------------------------------------

    #[test]
    fn parse_dir_list_splits_and_trims() {
        assert_eq!(
            parse_dir_list("/a:/b:/c"),
            vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
        );
        // Semicolons and commas are also separators (NAS wizard invites them).
        assert_eq!(
            parse_dir_list("/a ; /b , /c"),
            vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn parse_dir_list_drops_empty_and_whitespace_entries() {
        assert!(parse_dir_list("").is_empty());
        assert!(parse_dir_list("   ").is_empty());
        assert!(parse_dir_list(":::").is_empty());
        assert_eq!(parse_dir_list(":/only:").into_iter().count(), 1);
        assert_eq!(parse_dir_list(":/only:"), vec![PathBuf::from("/only")]);
    }

    fn cfg_with(host: &str, port: u16, data_dir: &str) -> Config {
        Config {
            host: host.into(),
            port,
            data_dir: PathBuf::from(data_dir),
            tmdb_language: "en-US".into(),
            tmdb_enrich: true,
            ..Default::default()
        }
    }

    #[test]
    fn socket_addr_parses_ipv4_ipv6_and_falls_back() {
        assert_eq!(
            cfg_with("127.0.0.1", 8080, "./data").socket_addr(),
            "127.0.0.1:8080".parse().unwrap()
        );
        assert_eq!(cfg_with("::1", 4040, "./data").socket_addr(), "[::1]:4040".parse().unwrap());
        // A non-IP host binds 0.0.0.0 on the configured port instead of panicking.
        assert_eq!(
            cfg_with("not-an-ip", 1234, "./data").socket_addr(),
            "0.0.0.0:1234".parse().unwrap()
        );
    }

    #[test]
    fn db_and_logs_paths_hang_off_the_data_dir() {
        let c = cfg_with("0.0.0.0", 4040, "/var/lib/kroma");
        assert_eq!(c.db_path(), PathBuf::from("/var/lib/kroma/kroma.db"));
        assert_eq!(c.logs_dir(), PathBuf::from("/var/lib/kroma/logs"));
    }

    // ----- from_env (serialized: mutates process env) ----------------------------

    const KEYS: &[&str] = &[
        "KROMA_HOST",
        "KROMA_PORT",
        "KROMA_MEDIA_DIRS",
        "KROMA_MOVIES_DIRS",
        "KROMA_SERIES_DIRS",
        "KROMA_DATA_DIR",
        "KROMA_TMDB_API_KEY",
        "KROMA_TMDB_LANGUAGE",
        "KROMA_TMDB_ENRICH",
        "KROMA_WEB_URL",
        "KROMA_WEB_DIR",
    ];

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Take the env lock, recovering from a prior panicked holder so one failing
    /// assertion doesn't cascade into "poisoned" failures for the rest.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clear_env() {
        for k in KEYS {
            env::remove_var(k);
        }
    }

    #[test]
    fn from_env_uses_defaults_when_unset() {
        let _g = env_guard();
        clear_env();

        let c = Config::from_env();
        assert_eq!(c.host, "0.0.0.0");
        assert_eq!(c.port, 4040);
        assert!(c.media_dirs.is_empty());
        assert!(c.movies_dirs.is_empty());
        assert!(c.series_dirs.is_empty());
        assert_eq!(c.data_dir, PathBuf::from("./data"));
        // No explicit key: falls back to the built-in TMDB key so metadata works.
        assert_eq!(c.tmdb_api_key.as_deref(), Some(BUILTIN_TMDB_API_KEY));
        assert_eq!(c.tmdb_language, "en-US");
        assert!(c.tmdb_enrich);
        assert!(c.web_url.is_none());
        assert!(c.web_dir.is_none());

        clear_env();
    }

    #[test]
    fn from_env_reads_and_normalizes_every_var() {
        let _g = env_guard();
        clear_env();

        // A web dir counts only when it holds `_shell.html`.
        let web = std::env::temp_dir().join(format!("kroma-webdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&web);
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("_shell.html"), b"<html></html>").unwrap();

        env::set_var("KROMA_HOST", "127.0.0.1");
        env::set_var("KROMA_PORT", "9999");
        env::set_var("KROMA_MEDIA_DIRS", "/a:/b");
        env::set_var("KROMA_MOVIES_DIRS", "/movies");
        env::set_var("KROMA_SERIES_DIRS", "/tv;/tv2");
        env::set_var("KROMA_DATA_DIR", "/data/root");
        env::set_var("KROMA_TMDB_API_KEY", "  mykey  ");
        env::set_var("KROMA_TMDB_LANGUAGE", "fr-FR");
        env::set_var("KROMA_TMDB_ENRICH", "0");
        env::set_var("KROMA_WEB_URL", "https://kroma.example/");
        env::set_var("KROMA_WEB_DIR", web.to_str().unwrap());

        let c = Config::from_env();
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 9999);
        assert_eq!(c.media_dirs, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(c.movies_dirs, vec![PathBuf::from("/movies")]);
        assert_eq!(c.series_dirs, vec![PathBuf::from("/tv"), PathBuf::from("/tv2")]);
        assert_eq!(c.data_dir, PathBuf::from("/data/root"));
        // Explicit key wins over the built-in and is trimmed.
        assert_eq!(c.tmdb_api_key.as_deref(), Some("mykey"));
        assert_eq!(c.tmdb_language, "fr-FR");
        assert!(!c.tmdb_enrich);
        // Trailing slash trimmed.
        assert_eq!(c.web_url.as_deref(), Some("https://kroma.example"));
        assert_eq!(c.web_dir, Some(web.clone()));

        clear_env();
        let _ = std::fs::remove_dir_all(&web);
    }

    #[test]
    fn from_env_edge_cases_for_port_key_enrich_and_webdir() {
        let _g = env_guard();
        clear_env();

        // A non-numeric port falls back to the default.
        env::set_var("KROMA_PORT", "not-a-port");
        // An explicit-but-empty key falls back to the built-in.
        env::set_var("KROMA_TMDB_API_KEY", "   ");
        // Any other enrich spelling than the off-words stays enabled.
        env::set_var("KROMA_TMDB_ENRICH", "yes-please");
        // A web dir without `_shell.html` is rejected.
        let bad = std::env::temp_dir().join(format!("kroma-webdir-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bad);
        std::fs::create_dir_all(&bad).unwrap();
        env::set_var("KROMA_WEB_DIR", bad.to_str().unwrap());

        let c = Config::from_env();
        assert_eq!(c.port, 4040);
        assert_eq!(c.tmdb_api_key.as_deref(), Some(BUILTIN_TMDB_API_KEY));
        assert!(c.tmdb_enrich);
        assert!(c.web_dir.is_none());

        // Each of the recognized off-words disables enrichment.
        for off in ["0", "false", "no", "off"] {
            env::set_var("KROMA_TMDB_ENRICH", off);
            assert!(!Config::from_env().tmdb_enrich, "{off} should disable enrich");
        }
        // A blank web URL collapses to None.
        env::set_var("KROMA_WEB_URL", "");
        assert!(Config::from_env().web_url.is_none());

        clear_env();
        let _ = std::fs::remove_dir_all(&bad);
    }
}
