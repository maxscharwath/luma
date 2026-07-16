//! HTTP transport over the system `curl` binary the same no-HTTP-crate approach
//! as the server's TMDB and LLM adapters, packaged as a crate so the
//! acquisition stack (Torznab indexers, Transmission/qBittorrent RPC, VPN
//! checks) shares one transport instead of three private copies.
//!
//! Scope: small request/response exchanges with visible status + headers,
//! JSON/form/bytes bodies, SOCKS5 proxying and cookie jars. Deliberately NOT
//! streaming: every payload here (XML feeds, RPC replies, .torrent files) fits
//! in memory. Response headers are captured via `-D <tmpfile>` because some
//! protocols carry state there (Transmission's `X-Transmission-Session-Id`
//! rides a 409 response), which is also why requests never pass `-f`: callers
//! read [`Response::status`] instead of losing the body on HTTP errors.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

/// A prepared request: builder-style options, then one of the executors
/// ([`Fetch::get`], [`Fetch::post_json`], [`Fetch::post_form`]).
#[derive(Debug, Clone)]
pub struct Fetch {
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    socks5: Option<String>,
    cookie_jar: Option<PathBuf>,
    max_time_secs: u32,
}

impl Default for Fetch {
    fn default() -> Self {
        Self { headers: Vec::new(), query: Vec::new(), socks5: None, cookie_jar: None, max_time_secs: 30 }
    }
}

impl Fetch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    /// URL-encoded query parameter (GET only; sent via `-G --data-urlencode`).
    pub fn query(mut self, name: &str, value: impl Into<String>) -> Self {
        self.query.push((name.to_string(), value.into()));
        self
    }

    /// Route this request through a SOCKS5 proxy (`host:port` or a full
    /// `socks5://user:pass@host:port` URL). Uses `--socks5-hostname` so DNS
    /// resolves on the proxy side too (no local DNS leak).
    pub fn socks5(mut self, proxy: impl Into<String>) -> Self {
        let p = proxy.into();
        if !p.trim().is_empty() {
            self.socks5 = Some(p);
        }
        self
    }

    /// Read + write cookies at `jar` across calls (qBittorrent's SID auth).
    pub fn cookie_jar(mut self, jar: impl Into<PathBuf>) -> Self {
        self.cookie_jar = Some(jar.into());
        self
    }

    /// Network budget for the whole transfer (default 30s).
    pub fn max_time(mut self, secs: u32) -> Self {
        self.max_time_secs = secs;
        self
    }

    pub fn get(&self, url: &str) -> Result<Response> {
        let mut cmd = self.base_cmd();
        if !self.query.is_empty() {
            cmd.arg("-G");
            for (k, v) in &self.query {
                cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
            }
        }
        cmd.arg(url);
        run(cmd)
    }

    pub fn post_json(&self, url: &str, body: &serde_json::Value) -> Result<Response> {
        let mut cmd = self.base_cmd();
        cmd.arg("-H").arg("content-type: application/json");
        cmd.arg("--data-binary").arg(serde_json::to_string(body)?);
        cmd.arg(url);
        run(cmd)
    }

    /// `application/x-www-form-urlencoded` POST (qBittorrent login/actions).
    pub fn post_form(&self, url: &str, fields: &[(&str, &str)]) -> Result<Response> {
        let mut cmd = self.base_cmd();
        for (k, v) in fields {
            cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
        }
        cmd.arg(url);
        run(cmd)
    }

    /// GET expecting a 2xx JSON body; the common happy path in one call.
    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.get(url)?.ensure_ok()?.json()
    }

    fn base_cmd(&self) -> Command {
        let mut cmd = Command::new("curl");
        // -L: indexer download links commonly redirect. No -f: we surface the
        // status ourselves so error bodies (and 409 handshakes) stay readable.
        cmd.args(["-s", "-S", "-L", "--max-time", &self.max_time_secs.to_string()]);
        if let Some(proxy) = &self.socks5 {
            // Force IPv4. Our only SOCKS proxy is the WireGuard-to-SOCKS bridge,
            // which is IPv4-only (wireproxy can't carry IPv6 traffic). With
            // remote DNS (`--socks5-hostname`) a dual-stack tracker hostname
            // otherwise resolves, part of the time, to an AAAA the tunnel can't
            // route, and the request fails "SOCKS host unreachable". `-4` pins
            // resolution to A records so the announce reliably rides IPv4.
            cmd.arg("-4").arg("--socks5-hostname").arg(proxy);
        }
        if let Some(jar) = &self.cookie_jar {
            cmd.arg("-c").arg(jar).arg("-b").arg(jar);
        }
        for (k, v) in &self.headers {
            cmd.arg("-H").arg(format!("{k}: {v}"));
        }
        cmd
    }
}

/// One HTTP exchange: final status + final header block + body bytes.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// First header with this name, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body)
            .with_context(|| format!("parse JSON response: {}", snippet(&self.body)))
    }

    /// Error out (with a body snippet) unless the status is 2xx.
    pub fn ensure_ok(self) -> Result<Self> {
        if !(200..300).contains(&self.status) {
            bail!("HTTP {}: {}", self.status, snippet(&self.body));
        }
        Ok(self)
    }
}

fn snippet(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    let mut s: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        s.push_str("...");
    }
    s
}

/// Unique-enough temp path for the `-D` header dump (pid + counter; the file
/// lives milliseconds and is best-effort removed).
fn header_dump_path() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("luma-http-hdr-{}-{n}", std::process::id()))
}

fn run(mut cmd: Command) -> Result<Response> {
    let hdr_path = header_dump_path();
    cmd.arg("-D").arg(&hdr_path);
    let out = cmd.output().context("spawn curl")?;
    let raw_headers = std::fs::read_to_string(&hdr_path).unwrap_or_default();
    let _ = std::fs::remove_file(&hdr_path);
    if !out.status.success() {
        bail!(
            "curl exit {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let (status, headers) = parse_last_block(&raw_headers)?;
    Ok(Response { status, headers, body: out.stdout })
}

/// Parse the LAST header block of a `-D` dump (with `-L`, curl appends one
/// block per hop; the final one describes the response whose body we hold).
fn parse_last_block(raw: &str) -> Result<(u16, Vec<(String, String)>)> {
    let mut status = None;
    let mut headers = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("HTTP/") {
            // New block: "HTTP/1.1 200 OK" or "HTTP/2 302". Reset accumulation.
            status = rest.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok());
            headers.clear();
        } else if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let status = status.ok_or_else(|| anyhow::anyhow!("no HTTP status line in curl header dump"))?;
    Ok((status, headers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_block() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Thing: a\r\n\r\n";
        let (status, headers) = parse_last_block(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("Content-Type".to_string(), "application/json".to_string()));
    }

    #[test]
    fn keeps_only_the_final_redirect_block() {
        let raw = concat!(
            "HTTP/1.1 302 Found\r\nLocation: https://elsewhere\r\n\r\n",
            "HTTP/2 200\r\ncontent-type: text/xml\r\n\r\n",
        );
        let (status, headers) = parse_last_block(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers, vec![("content-type".to_string(), "text/xml".to_string())]);
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let resp = Response {
            status: 409,
            headers: vec![("X-Transmission-Session-Id".to_string(), "abc123".to_string())],
            body: Vec::new(),
        };
        assert_eq!(resp.header("x-transmission-session-id"), Some("abc123"));
        assert_eq!(resp.header("missing"), None);
    }

    #[test]
    fn ensure_ok_rejects_non_2xx_with_snippet() {
        let resp = Response { status: 500, headers: Vec::new(), body: b"boom".to_vec() };
        let err = resp.ensure_ok().unwrap_err().to_string();
        assert!(err.contains("500"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }

    #[test]
    fn empty_socks5_is_ignored() {
        let f = Fetch::new().socks5("  ");
        assert!(f.socks5.is_none());
    }

    #[test]
    fn socks5_forces_ipv4() {
        // The SOCKS bridge is IPv4-only; a proxied request must pass `-4` so a
        // dual-stack hostname never resolves to an unroutable AAAA.
        let args: Vec<String> = Fetch::new()
            .socks5("socks5://127.0.0.1:25345")
            .base_cmd()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-4".to_string()), "{args:?}");
        assert!(args.contains(&"--socks5-hostname".to_string()), "{args:?}");
        // Without a proxy, no forced family.
        let plain: Vec<String> = Fetch::new()
            .base_cmd()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(!plain.contains(&"-4".to_string()), "{plain:?}");
    }
}
