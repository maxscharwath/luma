//! Stub for builds without the `rqbit` feature: same public surface, but
//! starting the engine reports "not compiled" (the whisper-local pattern).
//! Transmission / qBittorrent connectors remain fully functional.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Result};

use crate::DownloadClient;

#[derive(Debug, Clone, Default)]
pub struct RqbitConfig {
    pub session_dir: PathBuf,
    pub download_dir: PathBuf,
    pub socks_proxy_url: Option<String>,
    pub listen_port: Option<u16>,
    pub download_bps: Option<u32>,
    pub upload_bps: Option<u32>,
}

pub struct RqbitEngine {
    _private: (),
}

impl RqbitEngine {
    pub async fn start(_cfg: &RqbitConfig) -> Result<Arc<RqbitEngine>> {
        bail!("embedded engine not compiled (torrent-rqbit feature off)")
    }

    pub fn stop(&self) {}

    pub fn client(self: &Arc<Self>) -> Box<dyn DownloadClient> {
        unreachable!("stub RqbitEngine cannot be constructed")
    }
}
