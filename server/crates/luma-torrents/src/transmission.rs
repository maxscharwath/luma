//! Transmission RPC connector (`/transmission/rpc`, JSON over curl). The
//! protocol's CSRF handshake: any request may answer 409 with a fresh
//! `X-Transmission-Session-Id`, which we cache and replay once.

use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::{AddTorrentReq, ClientDef, DownloadClient, TorrentState, TorrentStatus};

const SESSION_HEADER: &str = "X-Transmission-Session-Id";
const STATUS_FIELDS: &[&str] = &[
    "hashString",
    "name",
    "percentDone",
    "status",
    "rateDownload",
    "rateUpload",
    "peersConnected",
    "totalSize",
    "downloadDir",
    "files",
    "errorString",
];

pub struct Transmission {
    url: String,
    username: String,
    password: String,
    session_id: Mutex<String>,
}

impl Transmission {
    pub fn new(def: &ClientDef) -> Self {
        let base = def.url.trim_end_matches('/');
        let url = if base.ends_with("/transmission/rpc") {
            base.to_string()
        } else {
            format!("{base}/transmission/rpc")
        };
        Self {
            url,
            username: def.username.clone(),
            password: def.password.clone(),
            session_id: Mutex::new(String::new()),
        }
    }

    fn fetch(&self) -> luma_fetch::Fetch {
        let mut f = luma_fetch::Fetch::new().max_time(60);
        let sid = self.session_id.lock().unwrap().clone();
        if !sid.is_empty() {
            f = f.header(SESSION_HEADER, sid);
        }
        if !self.username.is_empty() {
            let credentials = base64(format!("{}:{}", self.username, self.password).as_bytes());
            f = f.header("authorization", format!("Basic {credentials}"));
        }
        f
    }

    /// One RPC call, replaying once on the 409 session-id handshake.
    fn rpc(&self, method: &str, arguments: Value) -> Result<Value> {
        let body = json!({ "method": method, "arguments": arguments });
        let mut resp = self.fetch().post_json(&self.url, &body)?;
        if resp.status == 409 {
            let sid = resp
                .header(SESSION_HEADER)
                .ok_or_else(|| anyhow!("409 without a {SESSION_HEADER} header"))?
                .to_string();
            *self.session_id.lock().unwrap() = sid;
            resp = self.fetch().post_json(&self.url, &body)?;
        }
        if resp.status == 401 {
            bail!("authentication failed (check username/password)");
        }
        let v: Value = resp.ensure_ok()?.json()?;
        match v.get("result").and_then(Value::as_str) {
            Some("success") => Ok(v.get("arguments").cloned().unwrap_or(Value::Null)),
            Some(other) => bail!("transmission error: {other}"),
            None => bail!("malformed transmission response"),
        }
    }
}

impl DownloadClient for Transmission {
    fn kind(&self) -> &'static str {
        "transmission"
    }

    fn test(&self) -> Result<String> {
        let args = self.rpc("session-get", json!({}))?;
        let version = args.get("version").and_then(Value::as_str).unwrap_or("?");
        Ok(format!("Transmission {version}"))
    }

    fn add(&self, req: &AddTorrentReq) -> Result<String> {
        let mut arguments = json!({ "filename": req.magnet_or_url });
        if let Some(dir) = req.download_dir {
            arguments["download-dir"] = json!(dir);
        }
        if !req.label.is_empty() {
            arguments["labels"] = json!([req.label]);
        }
        let args = self.rpc("torrent-add", arguments);
        // Transmission < 4 rejects unknown fields like `labels`: retry bare.
        let args = match args {
            Ok(a) => a,
            Err(_) => {
                let mut bare = json!({ "filename": req.magnet_or_url });
                if let Some(dir) = req.download_dir {
                    bare["download-dir"] = json!(dir);
                }
                self.rpc("torrent-add", bare)?
            }
        };
        let added = args.get("torrent-added").or_else(|| args.get("torrent-duplicate"));
        added
            .and_then(|t| t.get("hashString"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("torrent-add returned no hash"))
    }

    fn status(&self, client_ref: &str) -> Result<Option<TorrentStatus>> {
        let args = self.rpc(
            "torrent-get",
            json!({ "ids": [client_ref], "fields": STATUS_FIELDS }),
        )?;
        let Some(t) = args.get("torrents").and_then(Value::as_array).and_then(|a| a.first()) else {
            return Ok(None);
        };
        let progress = t.get("percentDone").and_then(Value::as_f64).unwrap_or(0.0);
        // https://github.com/transmission/transmission docs: 0 stopped, 1-2
        // verify, 3-4 download (queued/active), 5-6 seed (queued/active).
        let code = t.get("status").and_then(Value::as_i64).unwrap_or(0);
        let error = t
            .get("errorString")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let state = if error.is_some() {
            TorrentState::Error
        } else {
            match code {
                0 if progress >= 1.0 => TorrentState::Completed,
                0 => TorrentState::Paused,
                1 | 2 => TorrentState::Queued,
                3 | 4 => TorrentState::Downloading,
                _ => TorrentState::Seeding,
            }
        };
        let files = t
            .get("files")
            .and_then(Value::as_array)
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| f.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(TorrentStatus {
            client_ref: client_ref.to_string(),
            name: t.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            info_hash: t.get("hashString").and_then(Value::as_str).map(str::to_string),
            progress,
            state,
            down_bps: t.get("rateDownload").and_then(Value::as_u64).unwrap_or(0),
            up_bps: t.get("rateUpload").and_then(Value::as_u64).unwrap_or(0),
            peers: t.get("peersConnected").and_then(Value::as_u64).unwrap_or(0) as u32,
            // Transmission doesn't split discovered vs connected here.
            peers_seen: t.get("peersConnected").and_then(Value::as_u64).unwrap_or(0) as u32,
            size_bytes: t.get("totalSize").and_then(Value::as_u64).unwrap_or(0),
            save_path: t.get("downloadDir").and_then(Value::as_str).map(str::to_string),
            files,
            error,
        }))
    }

    fn pause(&self, client_ref: &str) -> Result<()> {
        self.rpc("torrent-stop", json!({ "ids": [client_ref] })).map(|_| ())
    }

    fn resume(&self, client_ref: &str) -> Result<()> {
        self.rpc("torrent-start", json!({ "ids": [client_ref] })).map(|_| ())
    }

    fn reannounce(&self, client_ref: &str) -> Result<()> {
        self.rpc("torrent-reannounce", json!({ "ids": [client_ref] })).map(|_| ())
    }

    fn remove(&self, client_ref: &str, delete_data: bool) -> Result<()> {
        self.rpc(
            "torrent-remove",
            json!({ "ids": [client_ref], "delete-local-data": delete_data }),
        )
        .map(|_| ())
    }
}

/// Dependency-free base64 (standard alphabet, padded) for HTTP Basic auth.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = [
            ALPHABET[(n >> 18) as usize & 63],
            ALPHABET[(n >> 12) as usize & 63],
            ALPHABET[(n >> 6) as usize & 63],
            ALPHABET[n as usize & 63],
        ];
        // n input bytes yield n+1 real chars; the rest is padding.
        let keep = chunk.len() + 1;
        for (i, c) in chars.iter().enumerate() {
            out.push(if i < keep { *c as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn url_normalization_appends_rpc_path() {
        let def = |url: &str| ClientDef {
            kind: "transmission".into(),
            url: url.into(),
            username: String::new(),
            password: String::new(),
        };
        assert_eq!(Transmission::new(&def("http://nas:9091")).url, "http://nas:9091/transmission/rpc");
        assert_eq!(
            Transmission::new(&def("http://nas:9091/transmission/rpc")).url,
            "http://nas:9091/transmission/rpc"
        );
    }
}
