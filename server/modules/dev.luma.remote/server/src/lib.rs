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

use axum::extract::{Extension, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use luma_module_sdk::domain::Permission;
use luma_module_sdk::host::{async_trait, service, AuthUser, HostCtx, ServerModule};

mod provision;

/// This module's registry entry (manifest + packaged icon, embedded at compile
/// time from the shared module folder).
use luma_module_sdk::EmbeddedModule;
pub const MODULE: EmbeddedModule = luma_module_sdk::embedded_module!();

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

    /// Kill the connector: the tracked child AND any cloudflared orphaned by a
    /// SIGKILL of a previous sidecar generation (the supervisor SIGKILLs sidecars
    /// on update, which leaves their cloudflared running but no longer held by
    /// this process's in-memory handle - so killing only the tracked child left
    /// the tunnel up after "disable"). Reaps the tracked child to avoid a zombie.
    async fn kill_child(&self) {
        let child = {
            let mut g = self.inner.lock().await;
            g.running = false;
            g.child.take()
        };
        let had = child.is_some();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        #[cfg(unix)]
        self.kill_all_cloudflared();
        if had {
            info!("remote access: cloudflared connector stopped");
        }
    }

    /// SIGKILL every process running OUR `cloudflared` binary as a tunnel - the
    /// tracked child plus any orphan from a prior generation. Matching the
    /// resolved binary path is safe: nothing else on the box runs it. Best-effort.
    #[cfg(unix)]
    fn kill_all_cloudflared(&self) {
        let bin = self.resolve_binary();
        // The exact invocation is `<bin> tunnel ...`; match that adjacent
        // substring so a shell line that merely mentions the path isn't caught.
        let needle = format!("{bin} tunnel");
        let Ok(out) = std::process::Command::new("ps").arg("aux").output() else { return };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if line.contains(&needle) {
                // `ps aux`: USER PID ... - the pid is the second whitespace field.
                if let Some(pid) = line.split_whitespace().nth(1) {
                    let _ = std::process::Command::new("kill").args(["-9", pid]).status();
                }
            }
        }
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
            // Clear any orphan from a prior sidecar generation before spawning a
            // fresh tracked child, so exactly one connector ever runs (else an
            // untracked orphan keeps serving the tunnel after a later "disable").
            #[cfg(unix)]
            self.kill_all_cloudflared();
            let result = self.spawn_child(token).await;
            let mut g = self.inner.lock().await;
            g.starting = false;
            match result {
                Ok(child) => {
                    g.child = Some(child);
                    g.running = true;
                    g.since = Some(luma_module_sdk::primitives::now_iso8601());
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
    pub async fn reconcile(self: &Arc<Self>, host: &dyn HostCtx) {
        let enabled = host.setting_bool("remoteAccess", false);
        let token = host.setting_str("remoteAccessToken", "");
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
    pub fn spawn_boot(self: Arc<Self>, host: Arc<dyn HostCtx>) {
        tokio::spawn(async move {
            loop {
                self.reconcile(&*host).await;
                tokio::time::sleep(Duration::from_secs(RECONCILE_SECS)).await;
            }
        });
    }
}

// ----- routes -----------------------------------------------------------------

/// The Remote-access admin API, generic over any [`HostCtx`] state. Mounted by
/// the binary's `RemoteModule` with the live [`RemoteAccess`] injected as an
/// `Extension`. Paths are relative to the `/api/admin` nest.
pub fn routes<S>(remote: Arc<RemoteAccess>) -> Router<S>
where
    S: HostCtx + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/remote", get(get_remote::<S>).put(save_remote::<S>))
        .layer(Extension(remote))
}

/// Config (token masked) + live connector status.
async fn status_value(host: &dyn HostCtx, remote: &RemoteAccess) -> serde_json::Value {
    let st = remote.status().await;
    json!({
        "enabled": host.setting_bool("remoteAccess", false),
        "url": host.setting_str("remoteUrl", "").trim().trim_end_matches('/'),
        "hasToken": !host.setting_str("remoteAccessToken", "").trim().is_empty(),
        "status": serde_json::to_value(&st).unwrap_or_default(),
    })
}

/// `GET /api/admin/remote` -> current config (token masked) + live status.
async fn get_remote<S: HostCtx>(
    State(state): State<S>,
    Extension(remote): Extension<Arc<RemoteAccess>>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    state.require_any_admin(&user)?;
    Ok(Json(status_value(&state, &remote).await).into_response())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RemoteSaveBody {
    enabled: bool,
    url: String,
    /// Blank/omitted -> keep the stored token.
    token: Option<String>,
}

/// `PUT /api/admin/remote` -> persist config, then kick one reconcile so the
/// connector starts/stops immediately (non-blocking). Returns the fresh status.
async fn save_remote<S: HostCtx>(
    State(state): State<S>,
    Extension(remote): Extension<Arc<RemoteAccess>>,
    AuthUser(user): AuthUser,
    Json(body): Json<RemoteSaveBody>,
) -> Result<Response, Response> {
    state.require(&user, Permission::SettingsManage)?;
    // Only overwrite the secret when a non-blank value was actually typed.
    let token = body.token.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let mut patch = std::collections::BTreeMap::new();
    patch.insert("remoteAccess".to_string(), json!(body.enabled));
    patch.insert("remoteUrl".to_string(), json!(body.url.trim()));
    if let Some(tok) = token {
        patch.insert("remoteAccessToken".to_string(), json!(tok));
    }
    state.set_settings(patch);
    remote.reconcile(&state).await;
    Ok(Json(status_value(&state, &remote).await).into_response())
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

/// This module's id (matches its `module.json`).
pub const MODULE_ID: &str = "dev.luma.remote";

/// The Remote-access sub-module: serves the connector's admin routes and, on
/// enable, brings the managed tunnel up (if configured) and supervises it. It
/// resolves its own [`RemoteAccess`] through the host's service registry.
pub struct RemoteModule;

#[async_trait]
impl<S: HostCtx + Clone + Send + Sync + 'static> ServerModule<S> for RemoteModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn admin_routes(&self, host: &S) -> Option<Router<S>> {
        let remote = service::<RemoteAccess>(host)?;
        Some(routes::<S>(remote))
    }

    async fn on_enable(&self, host: Arc<dyn HostCtx>) {
        // Bring the tunnel up from the stored config (a no-op when off) and keep
        // it alive via the watchdog.
        if let Some(remote) = service::<RemoteAccess>(host.as_ref()) {
            remote.spawn_boot(host);
        }
    }
}

/// This module's backend behavior, for the host's generic module roster.
pub fn server_module<S: HostCtx + Clone + Send + Sync + 'static>() -> Box<dyn ServerModule<S>> {
    Box::new(RemoteModule)
}
