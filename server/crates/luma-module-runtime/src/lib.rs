//! The out-of-process module runtime.
//!
//! Each module ships as its own native binary. Its `main()` is essentially one
//! call to [`serve`]: the runtime reads the environment the core supervisor set
//! (module id, the local port to bind, the core's URL + a shared secret, and the
//! shared SQLite path), opens the shared database directly (WAL = multi-process),
//! builds a [`RemoteHost`] that implements the same [`HostCtx`] contract the
//! module code is written against, mounts the module's `admin_routes`, and serves
//! them on the local port. The core reverse-proxies the module's HTTP and fans
//! its events; the module opens the DB itself, so `db()`, auth, and session
//! lookups are direct with no IPC.
//!
//! The only things that cross to the core are the genuinely in-process host
//! services: settings resolution (so built-in defaults stay correct), event
//! publish, and job triggers. Those go over a tiny authenticated callback API
//! (`/api/_host/*`).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use axum::http::StatusCode;
use axum::response::Response;
use luma_db::Pool;
use luma_domain::{Permission, User};
use luma_module_host::{json_error, Event, HostCtx, ServerModule};

/// The environment the core supervisor hands each module process.
struct Env {
    module_id: String,
    port: u16,
    core_url: String,
    host_token: String,
    db_path: PathBuf,
    data_dir: PathBuf,
}

impl Env {
    fn from_process() -> anyhow::Result<Self> {
        let get = |k: &str| std::env::var(k).map_err(|_| anyhow::anyhow!("{k} not set"));
        Ok(Self {
            module_id: get("LUMA_MODULE_ID")?,
            port: get("LUMA_MODULE_PORT")?.parse()?,
            core_url: get("LUMA_CORE_URL")?,
            host_token: get("LUMA_HOST_TOKEN")?,
            db_path: PathBuf::from(get("LUMA_DB_PATH")?),
            data_dir: PathBuf::from(get("LUMA_DATA_DIR")?),
        })
    }
}

/// The out-of-process [`HostCtx`]: the module's own view of the app. `db()` is a
/// direct pool on the shared SQLite; settings / events / jobs go to the core over
/// the callback API; module-owned services (built here, not injected by the core)
/// live in a local registry.
#[derive(Clone)]
pub struct RemoteHost {
    inner: Arc<Inner>,
}

struct Inner {
    module_id: String,
    data_dir: PathBuf,
    db: Pool,
    core_url: String,
    host_token: String,
    services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl RemoteHost {
    fn new(env: &Env) -> anyhow::Result<Self> {
        // Open the shared DB the core owns. `init` is idempotent (CREATE TABLE IF
        // NOT EXISTS); the core has already migrated by the time we spawn.
        let db = luma_db::init(&env.db_path)?;
        Ok(Self {
            inner: Arc::new(Inner {
                module_id: env.module_id.clone(),
                data_dir: env.data_dir.clone(),
                db,
                core_url: env.core_url.clone(),
                host_token: env.host_token.clone(),
                services: RwLock::new(HashMap::new()),
            }),
        })
    }

    /// This module's id (as the core supervisor assigned it).
    pub fn module_id(&self) -> &str {
        &self.inner.module_id
    }

    /// Register a module-owned concrete service (e.g. the module's engine /
    /// bridge) so its own code can resolve it by type through `service::<T>(host)`.
    /// Keyed exactly like the in-process registry (concrete `TypeId::of::<T>()`,
    /// single `Arc`). This is the wiring that used to live in the core binary's
    /// `main.rs`; it now belongs to the module process.
    pub fn register_service<T: Any + Send + Sync>(&self, service: Arc<T>) {
        self.inner
            .services
            .write()
            .unwrap()
            .insert(TypeId::of::<T>(), service as Arc<dyn Any + Send + Sync>);
    }

    /// Register a cross-module PORT provider (a `dyn Trait` object), keyed like
    /// [`luma_module_host::port_service`] so consumers resolve it via
    /// `resolve_port::<dyn Trait>(host)`. Used when a module both provides a port
    /// and serves it in-process to its own code.
    pub fn register_port<P: ?Sized + Any + Send + Sync>(&self, port: Arc<P>) {
        let (tid, val) = luma_module_host::port_service(port);
        self.inner.services.write().unwrap().insert(tid, val);
    }

    /// An authenticated curl client to the core callback API.
    fn callback(&self) -> luma_http::Fetch {
        luma_http::Fetch::new().header("authorization", format!("Bearer {}", self.inner.host_token))
    }

    fn host_url(&self, path: &str) -> String {
        format!("{}/api/_host/{path}", self.inner.core_url.trim_end_matches('/'))
    }
}

impl HostCtx for RemoteHost {
    fn db(&self) -> &Pool {
        &self.inner.db
    }

    fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }

    fn require(&self, user: &User, perm: Permission) -> Result<(), Response> {
        if user.can(perm) {
            Ok(())
        } else {
            Err(json_error(StatusCode::FORBIDDEN, "forbidden"))
        }
    }

    fn require_any_admin(&self, user: &User) -> Result<(), Response> {
        if user.is_any_admin() {
            Ok(())
        } else {
            Err(json_error(StatusCode::FORBIDDEN, "forbidden"))
        }
    }

    fn lerr(&self, _user: &User, status: StatusCode, key: &str) -> Response {
        // Out-of-process modules don't carry the core's i18n catalogs; return the
        // key. The frontend already localizes known error keys.
        json_error(status, key)
    }

    fn setting_str(&self, key: &str, default: &str) -> String {
        self.callback()
            .query("key", key)
            .query("kind", "str")
            .query("default", default)
            .get_json::<serde_json::Value>(&self.host_url("setting"))
            .ok()
            .and_then(|v| v.get("value").and_then(|x| x.as_str().map(str::to_string)))
            .unwrap_or_else(|| default.to_string())
    }

    fn setting_bool(&self, key: &str, default: bool) -> bool {
        self.callback()
            .query("key", key)
            .query("kind", "bool")
            .query("default", default.to_string())
            .get_json::<serde_json::Value>(&self.host_url("setting"))
            .ok()
            .and_then(|v| v.get("value").and_then(serde_json::Value::as_bool))
            .unwrap_or(default)
    }

    fn setting_i64(&self, key: &str, default: i64) -> i64 {
        self.callback()
            .query("key", key)
            .query("kind", "i64")
            .query("default", default.to_string())
            .get_json::<serde_json::Value>(&self.host_url("setting"))
            .ok()
            .and_then(|v| v.get("value").and_then(serde_json::Value::as_i64))
            .unwrap_or(default)
    }

    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>) {
        let _ = self
            .callback()
            .post_json(&self.host_url("settings"), &serde_json::json!({ "patch": patch }));
    }

    fn publish(&self, event: Event) {
        let _ = self.callback().post_json(
            &self.host_url("events"),
            &serde_json::json!({ "topic": event.topic, "payload": event.payload }),
        );
    }

    fn trigger_job(&self, key: &'static str, reason: &'static str) {
        let _ = self
            .callback()
            .post_json(&self.host_url("job"), &serde_json::json!({ "key": key, "reason": reason }));
    }

    fn module_enabled(&self, id: &str) -> bool {
        self.callback()
            .query("id", id)
            .get_json::<serde_json::Value>(&self.host_url("enabled"))
            .ok()
            .and_then(|v| v.get("enabled").and_then(serde_json::Value::as_bool))
            // A module process only runs while enabled, so default to true.
            .unwrap_or(true)
    }

    fn get_service(&self, type_id: TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        self.inner.services.read().unwrap().get(&type_id).cloned()
    }
}

/// Run a module process serving one `ServerModule`. Convenience over [`serve`].
pub async fn serve_one(
    setup: impl FnOnce(&RemoteHost),
    module: Box<dyn ServerModule<RemoteHost>>,
) -> anyhow::Result<()> {
    serve(setup, vec![module], axum::Router::new()).await
}

/// Run a module process. `setup` builds the process's own services + port
/// providers into the host (the wiring the core binary used to do); each module's
/// `admin_routes` + any `extra` routes (e.g. cross-module port endpoints) are
/// served on the assigned local port, and every module's `on_enable` runs. A
/// process may host several modules (an in-process cluster) or none (a
/// port-provider-only process); `extra` carries their `/_port/*` routes.
pub async fn serve(
    setup: impl FnOnce(&RemoteHost),
    modules: Vec<Box<dyn ServerModule<RemoteHost>>>,
    extra: axum::Router<RemoteHost>,
) -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").try_init().ok();
    let env = Env::from_process()?;
    let host = RemoteHost::new(&env)?;
    tracing::info!(module = %env.module_id, port = env.port, "module process starting");

    // Apply each module's schema (idempotent), then let the process wire services.
    for module in &modules {
        let migrations = module.migrations();
        if !migrations.is_empty() {
            let conn = host.db().get()?;
            luma_db::apply_migrations(&conn, migrations)?;
        }
    }
    setup(&host);

    // Bring every module's live services up (the process only spawns while enabled).
    for module in &modules {
        module.on_enable(Arc::new(host.clone()) as Arc<dyn HostCtx>).await;
    }

    // Serve every module's routes + any extra port routes + a health probe.
    let mut app = extra.route("/_health", axum::routing::get(|| async { "ok" }));
    for module in &modules {
        if let Some(routes) = module.admin_routes(&host) {
            app = app.merge(routes);
        }
    }
    let app = app.with_state(host);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], env.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "module listening");
    axum::serve(listener, app).await?;
    Ok(())
}
