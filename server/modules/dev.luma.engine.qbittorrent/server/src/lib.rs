//! qBittorrent WebUI connector (`/api/v2`, cookie-authenticated form posts
//! over curl). The SID cookie lives in a per-endpoint jar file; a 403 answer
//! re-logs-in once and replays. qBittorrent's add endpoint returns no hash, so
//! the ref comes from the magnet URI, else from diffing the LUMA category
//! before/after the add.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use luma_module_sdk::ports::{magnet_info_hash, AddTorrentReq, ClientDef, DownloadClient, TorrentState, TorrentStatus};

pub struct QBittorrent {
    base: String,
    username: String,
    password: String,
    jar: PathBuf,
}

impl QBittorrent {
    pub fn new(def: &ClientDef, jar: PathBuf) -> Self {
        Self {
            base: def.url.trim_end_matches('/').to_string(),
            username: def.username.clone(),
            password: def.password.clone(),
            jar,
        }
    }

    fn fetch(&self) -> luma_module_sdk::http::Fetch {
        luma_module_sdk::http::Fetch::new().max_time(60).cookie_jar(&self.jar)
    }

    fn login(&self) -> Result<()> {
        let resp = self.fetch().post_form(
            &format!("{}/api/v2/auth/login", self.base),
            &[("username", &self.username), ("password", &self.password)],
        )?;
        let resp = resp.ensure_ok()?;
        if !resp.text().contains("Ok") {
            bail!("authentication failed (check username/password)");
        }
        Ok(())
    }

    /// GET returning the body, re-logging-in once on 403 (expired SID).
    fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<luma_module_sdk::http::Response> {
        let url = format!("{}{path}", self.base);
        let build = || {
            let mut f = self.fetch();
            for (k, v) in params {
                f = f.query(k, v.to_string());
            }
            f
        };
        let resp = build().get(&url)?;
        if resp.status == 403 {
            self.login()?;
            return build().get(&url)?.ensure_ok();
        }
        resp.ensure_ok()
    }

    fn post(&self, path: &str, fields: &[(&str, &str)]) -> Result<luma_module_sdk::http::Response> {
        let url = format!("{}{path}", self.base);
        let resp = self.fetch().post_form(&url, fields)?;
        if resp.status == 403 {
            self.login()?;
            return self.fetch().post_form(&url, fields)?.ensure_ok();
        }
        resp.ensure_ok()
    }

    fn torrents_info(&self, params: &[(&str, &str)]) -> Result<Vec<Value>> {
        let resp = self.get("/api/v2/torrents/info", params)?;
        resp.json::<Vec<Value>>()
    }
}

fn state_of(qbit_state: &str, progress: f64) -> TorrentState {
    match qbit_state {
        "error" | "missingFiles" => TorrentState::Error,
        "pausedDL" | "stoppedDL" => TorrentState::Paused,
        "pausedUP" | "stoppedUP" => TorrentState::Completed,
        "uploading" | "stalledUP" | "queuedUP" | "forcedUP" => TorrentState::Seeding,
        "checkingDL" | "checkingUP" | "checkingResumeData" | "metaDL" | "queuedDL" | "allocating" => {
            TorrentState::Queued
        }
        _ if progress >= 1.0 => TorrentState::Seeding,
        _ => TorrentState::Downloading,
    }
}

impl DownloadClient for QBittorrent {
    fn kind(&self) -> &'static str {
        "qbittorrent"
    }

    fn test(&self) -> Result<String> {
        self.login()?;
        let version = self.get("/api/v2/app/version", &[])?.text();
        Ok(format!("qBittorrent {}", version.trim()))
    }

    fn add(&self, req: &AddTorrentReq) -> Result<String> {
        // Known hash up-front for magnets; otherwise diff the category.
        let known = magnet_info_hash(req.magnet_or_url);
        let before: Vec<String> = if known.is_none() {
            self.torrents_info(&[("category", req.label)])?
                .iter()
                .filter_map(|t| t.get("hash").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };

        let mut fields: Vec<(&str, &str)> =
            vec![("urls", req.magnet_or_url), ("category", req.label)];
        if let Some(dir) = req.download_dir {
            fields.push(("savepath", dir));
        }
        let resp = self.post("/api/v2/torrents/add", &fields)?;
        if resp.text().contains("Fails") {
            bail!("qBittorrent rejected the torrent");
        }
        if let Some(hash) = known {
            return Ok(hash);
        }
        // .torrent-URL adds return no hash: poll the category for the new one.
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let now = self.torrents_info(&[("category", req.label)])?;
            if let Some(hash) = now
                .iter()
                .filter_map(|t| t.get("hash").and_then(Value::as_str))
                .find(|h| !before.iter().any(|b| b == h))
            {
                return Ok(hash.to_string());
            }
        }
        Err(anyhow!("added, but could not identify the new torrent's hash"))
    }

    fn status(&self, client_ref: &str) -> Result<Option<TorrentStatus>> {
        let torrents = self.torrents_info(&[("hashes", client_ref)])?;
        let Some(t) = torrents.first() else {
            return Ok(None);
        };
        let progress = t.get("progress").and_then(Value::as_f64).unwrap_or(0.0);
        let qstate = t.get("state").and_then(Value::as_str).unwrap_or("");
        let files: Vec<String> = self
            .get("/api/v2/torrents/files", &[("hash", client_ref)])
            .and_then(|r| r.json::<Vec<Value>>())
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
            info_hash: Some(client_ref.to_string()),
            progress,
            state: state_of(qstate, progress),
            down_bps: t.get("dlspeed").and_then(Value::as_u64).unwrap_or(0),
            up_bps: t.get("upspeed").and_then(Value::as_u64).unwrap_or(0),
            // Connected leechers + seeds currently in swarm.
            peers: (t.get("num_leechs").and_then(Value::as_u64).unwrap_or(0)
                + t.get("num_seeds").and_then(Value::as_u64).unwrap_or(0)) as u32,
            // Total swarm size the tracker reported (incl. not-connected).
            peers_seen: (t.get("num_incomplete").and_then(Value::as_u64).unwrap_or(0)
                + t.get("num_complete").and_then(Value::as_u64).unwrap_or(0)) as u32,
            size_bytes: t.get("size").and_then(Value::as_u64).unwrap_or(0),
            save_path: t.get("save_path").and_then(Value::as_str).map(str::to_string),
            files,
            error: matches!(qstate, "error" | "missingFiles").then(|| format!("state: {qstate}")),
        }))
    }

    fn pause(&self, client_ref: &str) -> Result<()> {
        self.post("/api/v2/torrents/pause", &[("hashes", client_ref)]).map(|_| ())
    }

    fn resume(&self, client_ref: &str) -> Result<()> {
        self.post("/api/v2/torrents/resume", &[("hashes", client_ref)]).map(|_| ())
    }

    fn reannounce(&self, client_ref: &str) -> Result<()> {
        self.post("/api/v2/torrents/reannounce", &[("hashes", client_ref)]).map(|_| ())
    }

    fn remove(&self, client_ref: &str, delete_data: bool) -> Result<()> {
        self.post(
            "/api/v2/torrents/delete",
            &[("hashes", client_ref), ("deleteFiles", if delete_data { "true" } else { "false" })],
        )
        .map(|_| ())
    }
}

/// One cookie jar per endpoint+user so two qBittorrent configs never share a SID.
fn cookie_jar_path(state_dir: &std::path::Path, def: &ClientDef) -> PathBuf {
    let mut tag: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{}|{}", def.url, def.username).bytes() {
        tag ^= u64::from(b);
        tag = tag.wrapping_mul(0x1000_0000_01b3);
    }
    state_dir.join(format!("qbit-{tag:016x}.cookies"))
}

/// The download-client registry kind this engine provides.
pub const KIND: &str = "qbittorrent";

/// Register the qBittorrent factory into a download-client registry (called by
/// the engine module's ServerModule on enable).
pub fn register(reg: &mut luma_module_sdk::ports::DownloadClientRegistry) {
    reg.register(KIND, |def, ctx| {
        Ok(Box::new(QBittorrent::new(def, cookie_jar_path(ctx.state_dir, def))) as Box<dyn DownloadClient>)
    });
}

/// This module's id (matches its `module.json`).
pub const MODULE_ID: &str = "dev.luma.engine.qbittorrent";

/// This module's registry entry (manifest + packaged icon embedded at compile time).
use luma_module_sdk::EmbeddedModule;
pub const MODULE: EmbeddedModule = luma_module_sdk::embedded_module!();

/// The qBittorrent engine sub-module: a lifecycle-only [`ServerModule`] that
/// registers / unregisters its download-client kind on the Downloads module's
/// shared registry as it is enabled / disabled. It reaches the `DownloadManager`
/// through the host's service registry, so the binary wires nothing.
pub struct QbittorrentModule;

#[luma_module_sdk::host::async_trait]
impl<S: luma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>
    luma_module_sdk::host::ServerModule<S> for QbittorrentModule
{
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    async fn on_enable(&self, host: std::sync::Arc<dyn luma_module_sdk::host::HostCtx>) {
        if let Some(dm) = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::DownloadClientHost>(host.as_ref()) {
            dm.register_engine(register);
        }
    }

    async fn on_disable(&self, host: std::sync::Arc<dyn luma_module_sdk::host::HostCtx>) {
        if let Some(dm) = luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::DownloadClientHost>(host.as_ref()) {
            dm.unregister_engine(KIND);
        }
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module<S: luma_module_sdk::host::HostCtx + Clone + Send + Sync + 'static>(
) -> Box<dyn luma_module_sdk::host::ServerModule<S>> {
    Box::new(QbittorrentModule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_jars_are_stable_and_distinct() {
        let a = ClientDef {
            kind: "qbittorrent".into(),
            url: "http://a:8080".into(),
            username: "u".into(),
            password: String::new(),
        };
        let b = ClientDef { url: "http://b:8080".into(), ..a.clone() };
        let dir = std::path::Path::new("/tmp");
        assert_eq!(cookie_jar_path(dir, &a), cookie_jar_path(dir, &a));
        assert_ne!(cookie_jar_path(dir, &a), cookie_jar_path(dir, &b));
    }
}
