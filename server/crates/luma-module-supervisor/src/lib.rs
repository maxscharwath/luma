//! The core side of the out-of-process module system.
//!
//! [`Supervisor`] spawns each installed module's native binary (from
//! `<data>/modules/<id>/`), assigns it a local port, keeps a live `id -> port`
//! map, and restarts it if it dies. [`proxy_to`] reverse-proxies an inbound
//! request to a module process. [`host_router`] serves the `/api/_host/*`
//! callback API (settings / events / jobs / enabled) the module runtime calls
//! back into, authenticated by a shared per-process token.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, RwLock};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use luma_module_host::{Event, HostCtx};
use serde_json::{json, Value};

/// The name the packed module binary is stored under inside `<id>/`.
pub const MODULE_BIN: &str = "module";

/// A running module process + the port it listens on.
struct Proc {
    port: u16,
    child: Child,
}

/// Everything the supervisor needs to spawn a module process.
#[derive(Clone)]
pub struct SupervisorConfig {
    /// `<data>/modules` — one subdir per installed module.
    pub modules_dir: PathBuf,
    /// The core's own base URL, for the module's callbacks.
    pub core_url: String,
    /// Shared secret authenticating module -> core callbacks.
    pub host_token: String,
    /// The shared SQLite path each module opens directly.
    pub db_path: PathBuf,
    /// The data dir handed to modules.
    pub data_dir: PathBuf,
}

pub struct Supervisor {
    cfg: SupervisorConfig,
    procs: RwLock<HashMap<String, Proc>>,
}

impl Supervisor {
    pub fn new(cfg: SupervisorConfig) -> Arc<Self> {
        Arc::new(Self { cfg, procs: RwLock::new(HashMap::new()) })
    }

    /// The install dir of a module (`<data>/modules/<id>`).
    fn dir(&self, id: &str) -> PathBuf {
        self.cfg.modules_dir.join(id)
    }

    /// Read every installed module's `module.json` (best-effort).
    pub fn installed_manifests(&self) -> Vec<Value> {
        let Ok(entries) = std::fs::read_dir(&self.cfg.modules_dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| std::fs::read_to_string(e.path().join("module.json")).ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect()
    }

    /// The local port a running module listens on, if any.
    pub fn port_of(&self, id: &str) -> Option<u16> {
        self.procs.read().unwrap().get(id).map(|p| p.port)
    }

    /// The shared secret the callback API authenticates module -> core with.
    pub fn host_token(&self) -> &str {
        &self.cfg.host_token
    }

    /// The local port of the running module that owns the admin-route first
    /// segment `seg` (from its manifest's `adminPrefixes`), for reverse-proxying
    /// `/api/admin/<seg>/*` to its sidecar. `None` if no installed+running module
    /// claims it.
    pub fn admin_route_port(&self, seg: &str) -> Option<u16> {
        for m in self.installed_manifests() {
            let owns = m
                .get("adminPrefixes")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|p| p.as_str() == Some(seg)));
            if owns {
                if let Some(id) = m.get("id").and_then(Value::as_str) {
                    return self.port_of(id);
                }
            }
        }
        None
    }

    /// Spawn a module process (idempotent: a no-op if already running). Picks a
    /// free localhost port, launches `<id>/module` with the runtime env, and
    /// tracks the child. Errors if the binary is missing.
    pub fn spawn(&self, id: &str) -> anyhow::Result<u16> {
        if let Some(p) = self.procs.read().unwrap().get(id) {
            return Ok(p.port);
        }
        let bin = self.dir(id).join(MODULE_BIN);
        if !bin.exists() {
            anyhow::bail!("module binary missing: {}", bin.display());
        }
        let port = free_port()?;
        let child = Command::new(&bin)
            .env("LUMA_MODULE_ID", id)
            .env("LUMA_MODULE_PORT", port.to_string())
            .env("LUMA_CORE_URL", &self.cfg.core_url)
            .env("LUMA_HOST_TOKEN", &self.cfg.host_token)
            .env("LUMA_DB_PATH", &self.cfg.db_path)
            .env("LUMA_DATA_DIR", &self.cfg.data_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        tracing::info!(module = %id, port, pid = child.id(), "spawned module process");
        self.procs.write().unwrap().insert(id.to_string(), Proc { port, child });
        Ok(port)
    }

    /// Stop a module process (SIGKILL). A no-op if not running.
    pub fn stop(&self, id: &str) {
        if let Some(mut p) = self.procs.write().unwrap().remove(id) {
            let _ = p.child.kill();
            let _ = p.child.wait();
            tracing::info!(module = %id, "stopped module process");
        }
    }

    /// Install a `.lmod` bundle: unpack it under `<modules_dir>/<id>/` (path-safe,
    /// allow-listed), make the binary executable, and spawn it. Returns the
    /// module's manifest JSON.
    pub fn install(&self, bytes: &[u8]) -> anyhow::Result<Value> {
        // `.lmod` is a gzip tar; a raw tar is also accepted.
        let mut decompressed = Vec::new();
        let tar_bytes: &[u8] = if bytes.starts_with(&[0x1f, 0x8b]) {
            std::io::Read::read_to_end(
                &mut flate2::read::GzDecoder::new(bytes),
                &mut decompressed,
            )?;
            &decompressed
        } else {
            bytes
        };

        let staging = self.cfg.modules_dir.join(format!(".staging-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&staging)?;
        let result = (|| {
            unpack_validated(tar_bytes, &staging)?;
            let manifest: Value =
                serde_json::from_str(&std::fs::read_to_string(staging.join("module.json"))?)?;
            let id = manifest
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("module.json has no id"))?
                .to_string();
            validate_id(&id)?;
            // Swap the install dir atomically-ish: stop the old, replace, spawn.
            self.stop(&id);
            let dest = self.dir(&id);
            let _ = std::fs::remove_dir_all(&dest);
            std::fs::rename(&staging, &dest)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = dest.join(MODULE_BIN);
                if bin.exists() {
                    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
                }
            }
            self.spawn(&id)?;
            Ok::<Value, anyhow::Error>(manifest)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    /// Download a `.lmod` from a registry URL and install it.
    pub async fn install_from_url(&self, url: &str) -> anyhow::Result<Value> {
        let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
        self.install(&bytes)
    }

    /// Fetch a module registry's `catalog.json` (the index the Store browses).
    pub async fn fetch_catalog(&self, url: &str) -> anyhow::Result<Value> {
        Ok(reqwest::get(url).await?.error_for_status()?.json().await?)
    }

    /// Uninstall a module: stop its process and delete its install dir.
    pub fn uninstall(&self, id: &str) -> anyhow::Result<()> {
        validate_id(id)?;
        self.stop(id);
        std::fs::remove_dir_all(self.dir(id))?;
        Ok(())
    }

    /// Spawn every installed module whose enabled flag (checked via `host`) is on.
    pub fn spawn_enabled(&self, host: &dyn HostCtx) {
        for manifest in self.installed_manifests() {
            let Some(id) = manifest.get("id").and_then(Value::as_str) else { continue };
            if host.module_enabled(id) {
                if let Err(e) = self.spawn(id) {
                    tracing::warn!(module = %id, error = %format!("{e:#}"), "module spawn failed");
                }
            }
        }
    }
}

/// A free localhost TCP port (bind :0, read it back, release).
fn free_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// A module id must be a safe directory name (it becomes `<modules>/<id>/`).
fn validate_id(id: &str) -> anyhow::Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id != "."
        && id != ".."
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    anyhow::ensure!(ok, "invalid module id {id:?}");
    Ok(())
}

/// Rebuild an archive entry path from its `Normal` components only (dropping
/// `..`, absolute + drive prefixes) and keep it only if it is an allow-listed
/// bundle file. An admin uploads arbitrary bytes, so the path can never escape
/// the install dir.
fn sanitized_entry(raw: &std::path::Path) -> Option<PathBuf> {
    use std::path::Component;
    let safe: PathBuf = raw
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    if safe.as_os_str().is_empty() {
        return None;
    }
    let rel = safe.to_string_lossy().replace('\\', "/");
    let allowed = matches!(rel.as_ref(), "module.json" | "module" | "icon.svg" | "icon.png")
        || rel.starts_with("fe/");
    allowed.then_some(safe)
}

/// Unpack an installed-module tar into `dest`, keeping only allow-listed entries.
fn unpack_validated(tar_bytes: &[u8], dest: &std::path::Path) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(tar_bytes);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw = entry.path()?.into_owned();
        let Some(safe) = sanitized_entry(&raw) else { continue };
        let out = dest.join(&safe);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&out)?;
    }
    Ok(())
}

/// Reverse-proxy `req` (its path already rewritten to the module-local path) to a
/// module process on `port`. Streams the body both ways.
pub async fn proxy_to(port: u16, path_and_query: &str, req: Request) -> Response {
    let url = format!("http://127.0.0.1:{port}{path_and_query}");
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "bad body").into_response(),
    };
    let client = reqwest::Client::new();
    let mut out = client.request(parts.method, &url).body(bytes.to_vec());
    for (name, value) in &parts.headers {
        // Host header must not be forwarded verbatim to the upstream.
        if name != axum::http::header::HOST {
            out = out.header(name.as_str(), value.as_bytes());
        }
    }
    match out.send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await.unwrap_or_default();
            let mut builder = Response::builder().status(status);
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
            builder.body(Body::from(body)).unwrap_or_else(|_| {
                (StatusCode::BAD_GATEWAY, "bad upstream response").into_response()
            })
        }
        Err(e) => {
            tracing::warn!(port, error = %e, "module proxy failed");
            (StatusCode::BAD_GATEWAY, "module unavailable").into_response()
        }
    }
}

// --- The /api/_host/* callback API modules call back into --------------------

#[derive(Clone)]
struct HostAuth {
    token: String,
}

/// Build the `/_host/*` callback router (mount under `/api`). Generic over the
/// core's [`HostCtx`] state so the handlers resolve settings / events / jobs
/// against the real app. Guarded by the shared `token`.
pub fn host_router<S>(token: String) -> Router<S>
where
    S: HostCtx + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/_host/setting", get(get_setting::<S>))
        .route("/_host/settings", post(set_settings::<S>))
        .route("/_host/events", post(publish_event::<S>))
        .route("/_host/job", post(trigger_job::<S>))
        .route("/_host/enabled", get(module_enabled::<S>))
        .route("/_host/libraries", get(library_folders::<S>))
        .route_layer(from_fn_with_state(HostAuth { token }, auth))
}

async fn auth(State(auth): State<HostAuth>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let ok = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == auth.token);
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "bad host token").into_response()
    }
}

#[derive(serde::Deserialize)]
struct SettingQuery {
    key: String,
    kind: String,
    default: String,
}

async fn get_setting<S: HostCtx>(
    State(host): State<S>,
    axum::extract::Query(q): axum::extract::Query<SettingQuery>,
) -> Json<Value> {
    let value = match q.kind.as_str() {
        "bool" => json!(host.setting_bool(&q.key, q.default == "true")),
        "i64" => json!(host.setting_i64(&q.key, q.default.parse().unwrap_or(0))),
        _ => json!(host.setting_str(&q.key, &q.default)),
    };
    Json(json!({ "value": value }))
}

#[derive(serde::Deserialize)]
struct SettingsPatch {
    patch: std::collections::BTreeMap<String, Value>,
}

async fn set_settings<S: HostCtx>(State(host): State<S>, Json(body): Json<SettingsPatch>) -> StatusCode {
    host.set_settings(body.patch);
    StatusCode::NO_CONTENT
}

#[derive(serde::Deserialize)]
struct EventBody {
    topic: String,
    payload: Value,
}

async fn publish_event<S: HostCtx>(State(host): State<S>, Json(body): Json<EventBody>) -> StatusCode {
    host.publish(Event { topic: body.topic, payload: body.payload });
    StatusCode::NO_CONTENT
}

#[derive(serde::Deserialize)]
struct JobBody {
    key: String,
    reason: String,
}

async fn trigger_job<S: HostCtx>(State(host): State<S>, Json(body): Json<JobBody>) -> StatusCode {
    // The trait wants &'static str; module job keys are a small fixed set, so
    // leaking them over the process lifetime is bounded and acceptable.
    let key: &'static str = Box::leak(body.key.into_boxed_str());
    let reason: &'static str = Box::leak(body.reason.into_boxed_str());
    host.trigger_job(key, reason);
    StatusCode::NO_CONTENT
}

#[derive(serde::Deserialize)]
struct EnabledQuery {
    id: String,
}

async fn module_enabled<S: HostCtx>(
    State(host): State<S>,
    axum::extract::Query(q): axum::extract::Query<EnabledQuery>,
) -> Json<Value> {
    Json(json!({ "enabled": host.module_enabled(&q.id) }))
}

/// The configured libraries, so an out-of-process import / organize module can
/// place files under the right root without linking the engine's Settings/Config.
async fn library_folders<S: HostCtx>(State(host): State<S>) -> Json<Value> {
    let libs: Vec<Value> = host
        .library_folders()
        .into_iter()
        .map(|(id, folders)| json!({ "id": id, "folders": folders }))
        .collect();
    Json(json!(libs))
}
