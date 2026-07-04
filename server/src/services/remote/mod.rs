//! Managed Cloudflare Tunnel connector.
//!
//! Optional and off by default. When the admin enables it and stores a tunnel
//! token, the server supervises a `cloudflared tunnel run --token <TOKEN>` child
//! so a box with no existing tunnel gets a public HTTPS endpoint (e.g.
//! `https://luma.example.com`) with no port-forwarding. Installs that already run
//! their own `cloudflared` leave this off and just set the public URL.
//!
//! Control model: a single **reconcile** loop continuously makes the running
//! connector match the persisted `remoteAccess` flag: it launches the child when
//! enabled (with a token) and kills it when disabled. Handlers only flip the
//! setting; they never block. The `cloudflared` binary is provided by the server
//! and downloaded on demand (see [`provision`]) inside the background launch, so
//! enabling never stalls the request on a multi-MB download.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::services::settings;
use crate::state::SharedState;

mod provision;

/// How many recent connector log lines we keep for the admin panel.
const LOG_CAP: usize = 200;
/// Reconcile cadence: how often the supervisor makes reality match the setting.
const RECONCILE_SECS: u64 = 3;

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    logs: VecDeque<String>,
    running: bool,
    /// A launch (possibly downloading the binary) is in flight.
    starting: bool,
    since: Option<String>,
    last_error: Option<String>,
}

/// Supervised `cloudflared` connector, held once in [`crate::state::AppState`].
pub struct RemoteAccess {
    inner: Mutex<Inner>,
    /// Server data dir, used to locate/cache a server-provided `cloudflared`.
    data_dir: PathBuf,
}

/// Snapshot for the admin panel. Never carries the token.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub running: bool,
    /// A launch is in progress (spawning, or downloading the binary).
    pub connecting: bool,
    pub since: Option<String>,
    pub last_error: Option<String>,
    /// Whether the server's `cloudflared` binary is present + runnable.
    pub binary_found: bool,
    pub binary_version: Option<String>,
    pub logs: Vec<String>,
}

impl RemoteAccess {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { inner: Mutex::new(Inner::default()), data_dir })
    }

    /// The `cloudflared` binary the server provides. Resolution order: packaged
    /// next to the server executable, a copy cached under `<data_dir>/bin`, then
    /// `cloudflared` from `PATH` (dev fallback). Never an admin-configured path.
    fn resolve_binary(&self) -> String {
        let name = provision::bin_name();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(p) = exe.parent().map(|d| d.join(name)) {
                if p.is_file() {
                    return p.to_string_lossy().into_owned();
                }
            }
        }
        let cached = provision::cached_path(&self.data_dir);
        if cached.is_file() {
            return cached.to_string_lossy().into_owned();
        }
        name.to_string()
    }

    /// Resolve a runnable `cloudflared`, downloading + caching it under the data
    /// dir if the server doesn't already provide one. Called from the background
    /// launch task, so a multi-MB download never blocks a request.
    async fn ensure_binary(&self) -> Result<String, String> {
        let bin = self.resolve_binary();
        if std::path::Path::new(&bin).is_absolute() {
            return Ok(bin);
        }
        if binary_version(&bin).await.is_some() {
            return Ok(bin);
        }
        self.push_log("cloudflared not found, downloading…".to_string()).await;
        info!("remote access: downloading cloudflared");
        let path = provision::download(&self.data_dir).await?;
        self.push_log(format!("cloudflared installed at {}", path.display())).await;
        info!("remote access: cloudflared installed at {}", path.display());
        Ok(path.to_string_lossy().into_owned())
    }

    /// Append a line to the bounded log ring.
    async fn push_log(&self, line: String) {
        let mut g = self.inner.lock().await;
        if g.logs.len() >= LOG_CAP {
            g.logs.pop_front();
        }
        g.logs.push_back(line);
    }

    /// Is the child still alive? Reaps the exit status if it died.
    async fn alive(&self) -> bool {
        let mut g = self.inner.lock().await;
        match g.child.as_mut() {
            None => false,
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    g.running = false;
                    g.child = None;
                    g.last_error = Some(format!("cloudflared exited ({status})"));
                    false
                }
                Err(_) => true,
            },
        }
    }

    /// Kill the running child, if any.
    async fn kill_child(&self) {
        let mut g = self.inner.lock().await;
        if let Some(child) = g.child.as_mut() {
            let _ = child.start_kill();
        }
        g.child = None;
        g.running = false;
    }

    /// Spawn the background launch: download (if needed) then run `cloudflared`.
    /// Non-blocking; guarded by the `starting` flag so only one runs at a time.
    fn launch(self: Arc<Self>, token: String) {
        tokio::spawn(async move {
            {
                let mut g = self.inner.lock().await;
                if g.starting {
                    return;
                }
                g.starting = true;
                g.last_error = None;
            }
            let result = self.spawn_child(token).await;
            let mut g = self.inner.lock().await;
            g.starting = false;
            match result {
                Ok(child) => {
                    g.child = Some(child);
                    g.running = true;
                    g.since = Some(crate::services::scan::now_iso8601());
                    info!("remote access: cloudflared connector started");
                }
                Err(e) => {
                    g.running = false;
                    g.last_error = Some(e.clone());
                    warn!("remote access: {e}");
                }
            }
        });
    }

    /// Resolve + spawn the `cloudflared` child, wiring its stdout/stderr into the
    /// log ring. Does not touch shared start/running state (the caller does).
    async fn spawn_child(self: &Arc<Self>, token: String) -> Result<Child, String> {
        let bin = self.ensure_binary().await?;
        let mut cmd = Command::new(&bin);
        cmd.args(["tunnel", "--no-autoupdate", "run", "--token", token.trim()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("failed to launch cloudflared ({bin}): {e}"))?;
        // Drain stdout + stderr into the log ring so the panel shows connection
        // progress ("Registered tunnel connection …") and any error.
        if let Some(out) = child.stdout.take() {
            let me = self.clone();
            tokio::spawn(async move { drain(me, out).await });
        }
        if let Some(err) = child.stderr.take() {
            let me = self.clone();
            tokio::spawn(async move { drain(me, err).await });
        }
        Ok(child)
    }

    /// Current status snapshot for the admin panel (binary probe included).
    pub async fn status(&self) -> RemoteStatus {
        let (running, connecting) = {
            let mut g = self.inner.lock().await;
            if let Some(child) = g.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    g.running = false;
                    g.child = None;
                    g.last_error = Some(format!("cloudflared exited ({status})"));
                }
            }
            (g.running, g.starting)
        };
        let version = binary_version(&self.resolve_binary()).await;
        let g = self.inner.lock().await;
        RemoteStatus {
            running,
            connecting,
            since: g.since.clone(),
            last_error: g.last_error.clone(),
            binary_found: version.is_some(),
            binary_version: version,
            logs: g.logs.iter().cloned().collect(),
        }
    }

    /// One reconcile step: make the running connector match the persisted setting.
    /// Enabled + token → launch (if down and not already starting); otherwise →
    /// kill. Called both by the loop and immediately after a settings change so
    /// the panel reflects the action without waiting a full tick.
    pub async fn reconcile(self: &Arc<Self>, state: &SharedState) {
        let enabled = settings::remote_access_enabled(&state.settings);
        let token = settings::remote_access_token(&state.settings);
        let desired = enabled && !token.trim().is_empty();
        let alive = self.alive().await;
        let starting = self.inner.lock().await.starting;
        if desired {
            if !alive && !starting {
                self.clone().launch(token);
            }
        } else if alive {
            self.kill_child().await;
        }
    }

    /// Boot hook: run the reconcile loop forever. Brings the tunnel up at boot if
    /// the admin left it enabled with a token, keeps it alive if the child dies,
    /// and takes it down promptly once disabled. No-op while disabled.
    pub fn spawn_boot(self: Arc<Self>, state: SharedState) {
        tokio::spawn(async move {
            loop {
                self.reconcile(&state).await;
                tokio::time::sleep(Duration::from_secs(RECONCILE_SECS)).await;
            }
        });
    }
}

/// Drain a child stream line-by-line into the connector's log ring.
async fn drain<R: tokio::io::AsyncRead + Unpin>(me: Arc<RemoteAccess>, stream: R) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(l)) = lines.next_line().await {
        me.push_log(l).await;
    }
}

/// Probe `bin --version`; returns the trimmed first line on success, else `None`
/// (binary missing / not runnable). Used for the panel's "found" indicator.
async fn binary_version(bin: &str) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}
