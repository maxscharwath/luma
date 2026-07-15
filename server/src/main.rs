//! LUMA a self-hosted, direct-play media streaming server.
//!
//! Scans a media library (Plex-style movie/show detection), persists it in
//! SQLite, exposes metadata over a JSON REST API, and range-streams the original
//! files to clients. It never transcodes: clients decode HEVC/H.265/AV1
//! themselves. `ffprobe` is used only to read metadata.

// The axum `Response` is intentionally the Err type of request guards so handlers
// short-circuit with `?`; boxing every guard for `result_large_err` would churn
// dozens of signatures for no real gain on these error paths.
#![allow(clippy::result_large_err)]

// The HTTP router + handlers. Everything below the router (infra adapters,
// services, app state, the i18n extractor and the wire-model barrel) lives in
// the luma-engine crate, aliased here so `crate::{infra,services,state,i18n,model}`
// call sites in api/ keep resolving. Lower layers (config/db/domain) are their
// own crates, likewise aliased.
mod api;
use luma_config as config;
use luma_db as db;
use luma_engine::{i18n, infra, model, services, state};

use anyhow::Context;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;
use crate::state::AppState;

/// Composition-root adapter: talks to the Vector module's `.lmod` sidecar
/// (dev.luma.vector) over the port bridge, wrapping it as the engine's
/// [`luma_engine::ports::Embedder`] port so the core never names the concrete
/// embedder crate. The heavy MiniLM/candle model runs out of the core; the
/// `embed`/`embed_batch` calls reuse the shared bridge helper, and `embed_batch`
/// keeps the catalog-wide reembed to one round-trip per chunk. When the sidecar
/// is absent every call degrades to empty vectors (like `NoopEmbedder`), so
/// recommendations quietly no-op rather than break.
struct EmbedderClient {
    resolve: luma_port_bridge::Resolver,
    /// Memoized `/_port/embedder/meta` (dim + relevance_floor), constant for the
    /// sidecar's life; `dim()` is hit per-item in the pipeline embed stage.
    meta: std::sync::RwLock<Option<serde_json::Value>>,
}
impl EmbedderClient {
    fn new(resolve: luma_port_bridge::Resolver) -> Self {
        Self { resolve, meta: std::sync::RwLock::new(None) }
    }
    fn meta(&self) -> serde_json::Value {
        if let Some(v) = self.meta.read().unwrap().clone() {
            return v;
        }
        let Some((base, token)) = (self.resolve)() else {
            return serde_json::Value::Null;
        };
        let v = luma_http::Fetch::new()
            .header("authorization", format!("Bearer {token}"))
            .get_json::<serde_json::Value>(&format!("{base}/_port/embedder/meta"))
            .unwrap_or(serde_json::Value::Null);
        if !v.is_null() {
            *self.meta.write().unwrap() = Some(v.clone());
        }
        v
    }
}
impl luma_engine::ports::Embedder for EmbedderClient {
    fn dim(&self) -> usize {
        self.meta().get("dim").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize
    }
    fn embed(&self, text: &str) -> Vec<f32> {
        luma_port_bridge::call_raw(&self.resolve, "embedder/embed", &serde_json::json!({ "text": text }))
            .unwrap_or_default()
    }
    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        luma_port_bridge::call_raw(&self.resolve, "embedder/embed_batch", &serde_json::json!({ "texts": texts }))
            .unwrap_or_default()
    }
    fn relevance_floor(&self) -> f32 {
        self.meta().get("relevance_floor").and_then(serde_json::Value::as_f64).map(|f| f as f32).unwrap_or(1.0)
    }
}

// On the Linux/musl single binary, musl's malloc is a global-lock design that
// collapses under our thread mix (tokio workers + rayon walks + candle tensors);
// mimalloc removes that contention. macOS dev keeps the system allocator.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Seed the uptime clock (now owned by luma-engine).
    luma_engine::process_started();
    let config = Config::from_env();
    // Keep the appender guard alive for the whole process so buffered log lines
    // are flushed to disk.
    let _log_guard = init_tracing(&config.logs_dir());

    info!(
        host = %config.host,
        port = config.port,
        media_dirs = config.media_dirs.len(),
        db = %config.db_path().display(),
        "starting LUMA server"
    );

    let ffprobe_available = infra::probe::ffprobe_available();
    if ffprobe_available {
        info!("ffprobe detected: full metadata extraction enabled");
    } else {
        warn!("ffprobe not found: metadata will be inferred from file extensions");
    }

    if config.tmdb_api_key.is_some() {
        if infra::metadata::curl_available() {
            info!(language = %config.tmdb_language, "TMDB enrichment enabled");
        } else {
            warn!("LUMA_TMDB_API_KEY is set but `curl` was not found; TMDB enrichment disabled");
        }
    }

    let db = db::init(&config.db_path()).context("failed to initialise database")?;

    // Let each module create the tables it owns, once, right after the core
    // schema (the acquisition module tables live in the module crates now). Runs
    // before any module reads/writes them (settings load, `apply_enabled_states`).
    {
        let conn = db.get().context("failed to get a db connection for module schema")?;
        for migration in luma_module_kernel::module_migrations() {
            db::apply_migrations(&conn, migration).context("failed to apply module schema")?;
        }
    }

    // Persisted settings (incl. the editable library definitions, seeded from
    // LUMA_MEDIA_DIRS on first run).
    let settings = services::settings::Settings::load(&db);
    let library_defs = services::settings::library_defs(&settings, &config);
    let has_folders = library_defs.iter().any(|d| !d.folders.is_empty());

    // Phase 1 (fast): walk + stat only, no ffprobe. The library becomes
    // browsable in seconds; codecs/duration/HDR fill in during phase 2 below.
    let mut data = services::scan::scan_all(&library_defs);

    // An empty scan is ambiguous. With *no* media dirs configured it's a fresh
    // install → seed demo content. But if dirs are configured and still produced
    // nothing, it's almost certainly a transient mount outage (NAS down). Syncing
    // an empty scan would make `sync_all` treat every real library as "vanished"
    // and cascade-delete it along with all the expensive probed metadata, so in
    // that case we keep the existing index instead of overwriting it the
    // watcher re-syncs automatically once the mount returns.
    let mount_outage = data.items.is_empty() && has_folders;
    if data.items.is_empty() && !has_folders {
        info!("no media dirs configured; seeding built-in demo content");
        data = services::demo::demo_data();
    }

    if mount_outage {
        warn!(
            media_dirs = config.media_dirs.len(),
            "configured media dirs produced no items; keeping the existing index (mount offline?) and skipping sync"
        );
    } else {
        db::sync_all(&db, &data.libraries, &data.shows, &data.items, &data.mtimes)
            .context("failed to persist library")?;
        info!(
            libraries = data.libraries.len(),
            shows = data.shows.len(),
            items = data.items.len(),
            "library index ready (phase 1)"
        );
    }

    let addr = config.socket_addr();

    // The out-of-process module supervisor (spawns/proxies installed .lmod
    // modules). Built here (before the module services) so ported modules that
    // moved out of the base build can be resolved as client proxies pointing at
    // their sidecar process. A fresh random token authenticates host callbacks.
    let host_token: String = {
        use rand::Rng;
        rand::rng()
            .sample_iter(rand::distr::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect()
    };
    let supervisor =
        luma_module_supervisor::Supervisor::new(luma_module_supervisor::SupervisorConfig {
            modules_dir: config.data_dir.join("modules"),
            core_url: format!("http://127.0.0.1:{}", config.port),
            host_token: host_token.clone(),
            db_path: config.db_path(),
            data_dir: config.data_dir.clone(),
            // A module with an in-core backend (a compiled ServerModule) can't be
            // shadowed by an installed `.lmod` of the same id (two live backends),
            // so the store rejects those. Manifest-only modules whose backend IS a
            // sidecar (whisper / vector) are NOT reserved -- their `.lmod` must be
            // installable.
            reserved_ids: luma_module_kernel::backend_ids(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        });

    // Build the module services + peer ports the composition root owns, so the
    // core (luma-engine) names no module: the Remote connector, the VPN bridge, and
    // the VpnProxy / TorrentFetch ports. AppState builds the download manager (its
    // one direct module field) and merges these in.
    let mut module_services: std::collections::HashMap<
        std::any::TypeId,
        std::sync::Arc<dyn std::any::Any + Send + Sync>,
    > = std::collections::HashMap::new();
    // The supervisor is a service too, so the module registry (luma-module-kernel)
    // can list runtime-installed `.lmod` modules + resolve their icons without the
    // kernel holding a router Extension.
    module_services.insert(
        std::any::TypeId::of::<luma_module_supervisor::Supervisor>(),
        supervisor.clone() as std::sync::Arc<dyn std::any::Any + Send + Sync>,
    );
    let remote = luma_remote::RemoteAccess::new(config.data_dir.clone());
    module_services.insert(std::any::TypeId::of::<luma_remote::RemoteAccess>(), remote);

    // Every out-of-process module the core CONSUMES is reached by a client proxy
    // that resolves the sidecar's live localhost port from the supervisor. One
    // resolver builder for all of them: id -> port -> (url, token).
    let local_resolver = |id: &'static str| -> luma_port_bridge::Resolver {
        let (sup, tok) = (supervisor.clone(), host_token.clone());
        std::sync::Arc::new(move || {
            sup.port_of(id).map(|p| (format!("http://127.0.0.1:{p}"), tok.clone()))
        })
    };

    // VPN (dev.luma.vpn): VpnProxyPort, consumed by indexer + torrents.
    let vpn_proxy: std::sync::Arc<dyn luma_module_sdk::ports::VpnProxyPort> =
        std::sync::Arc::new(luma_port_bridge::VpnProxyClient::new(local_resolver("dev.luma.vpn")));
    let (tid, val) = luma_module_host::port_service(vpn_proxy);
    module_services.insert(tid, val);
    // Indexers (dev.luma.indexer): torrent-fetch / data / native-search ports,
    // consumed by the torrents queue + acquisition.
    let torrent_fetch: std::sync::Arc<dyn luma_module_sdk::ports::TorrentFetchPort> =
        std::sync::Arc::new(luma_port_bridge::TorrentFetchClient::new(local_resolver("dev.luma.indexer")));
    let (tid, val) = luma_module_host::port_service(torrent_fetch);
    module_services.insert(tid, val);
    // Torznab (dev.luma.torznab): the external-aggregator search engine.
    let torznab: std::sync::Arc<dyn luma_module_sdk::ports::TorznabPort> =
        std::sync::Arc::new(luma_port_bridge::TorznabClient::new(local_resolver("dev.luma.torznab")));
    let (tid, val) = luma_module_host::port_service(torznab);
    module_services.insert(tid, val);
    let idx_db: std::sync::Arc<dyn luma_module_sdk::ports::IndexerDbPort> =
        std::sync::Arc::new(luma_port_bridge::IndexerDbClient::new(local_resolver("dev.luma.indexer")));
    let (tid, val) = luma_module_host::port_service(idx_db);
    module_services.insert(tid, val);
    let idx_search: std::sync::Arc<dyn luma_module_sdk::ports::IndexerSearchPort> =
        std::sync::Arc::new(luma_port_bridge::IndexerSearchClient::new(local_resolver("dev.luma.indexer")));
    let (tid, val) = luma_module_host::port_service(idx_search);
    module_services.insert(tid, val);
    // Acquisition (dev.luma.acquisition): its interactive-search + grab surface,
    // consumed by the core's /api/requests/:id/search + /grab endpoints.
    let acq_search: std::sync::Arc<dyn luma_module_sdk::ports::AcquisitionSearchPort> =
        std::sync::Arc::new(luma_port_bridge::AcquisitionSearchClient::new(local_resolver("dev.luma.acquisition")));
    let (tid, val) = luma_module_host::port_service(acq_search);
    module_services.insert(tid, val);
    // The download engine (dev.luma.torrents) is a sidecar: it PROVIDES the
    // DownloadClientHost / DownloadVpn / DownloadGrab / DownloadDb ports from its
    // own process (its bin serves them over the bridge), and the sidecars that
    // consume them (acquisition, vpn) resolve them sibling-to-sibling through the
    // core proxy. So the core neither constructs the manager nor registers those
    // ports here.
    // Whisper transcription runs out-of-process (the dev.luma.whisper .lmod);
    // register the client proxy so the subtitles endpoint resolves it by type. It
    // carries the DB pool (the progress/cancel side-channel) + a resolver to the
    // sidecar's port.
    let whisper_client = std::sync::Arc::new(api::online_subs::WhisperClient::new(
        local_resolver("dev.luma.whisper"),
        db.clone(),
    ));
    module_services
        .insert(std::any::TypeId::of::<api::online_subs::WhisperClient>(), whisper_client);
    // The embedder runs out-of-process (the dev.luma.vector .lmod); resolve it as a
    // client proxy to its sidecar. Absent sidecar => empty vectors (recommendations
    // quietly no-op), same as the former NoopEmbedder fallback.
    let embedder: std::sync::Arc<dyn luma_engine::ports::Embedder> =
        std::sync::Arc::new(EmbedderClient::new(local_resolver("dev.luma.vector")));
    // Acquisition's search / import / match jobs run in ITS sidecar now (resident
    // timers via `start_cron`), so the core registers no module jobs.
    let state = AppState::new(
        config,
        ffprobe_available,
        db,
        settings,
        embedder,
        module_services,
        &[],
    );
    services::activity::scan_completed(
        &state.activity,
        data.libraries.len(),
        data.shows.len(),
        data.items.len(),
        services::scan::now_iso8601(),
    );

    // Phase 2 (background): ffprobe every unprobed file, emitting live events as
    // codecs land. Spawned before serving so it overlaps request handling.
    infra::probe::spawn_probe_pass(
        state.db.clone(),
        state.ffprobe_available,
        state.events.clone(),
        state.activity.clone(),
    );

    // Build the keyword search index from the freshly-synced catalogue (titles
    // are searchable immediately; enrichment triggers a second rebuild once
    // cast/overview/genres land). Off-thread so it never delays serving.
    services::search::spawn_reindex(state.clone());

    // Resolve TMDB art for the freshly-scanned catalog in the background.
    services::enrich::maybe_spawn(&state, &data.items, &data.shows);

    // Watch the library for changes (periodic re-scan + filesystem events) so new
    // files appear without a manual rescan. Baseline = the startup scan we just
    // applied, so it stays quiet until something actually changes.
    infra::watch::spawn(state.clone(), infra::watch::signature(&data.items, &data.mtimes));

    // Reap idle HLS remux sessions (ffmpeg children + temp dirs).
    state.hls.spawn_reaper();

    // Live playback sessions: reap stale heartbeats → append to play history.
    state
        .playback
        .spawn_reaper(state.db.clone(), state.events.clone());

    // Sample CPU / RAM (and bandwidth from the playback registry) for the
    // admin dashboard charts.
    state.metrics.spawn_sampler(state.playback.clone());

    // Start the background-job cron scheduler (cache cleanup, recommendations
    // refresh, …). Manual + scheduled runs are tracked in the admin "Tâches" UI.
    state.jobs.clone().spawn_scheduler(state.clone());

    // Bring every ENABLED module's live services up in dependency order (the VPN
    // bridge before the engine that tunnels through it; the download engines +
    // their monitor + client-row seed; the remote tunnel), and leave disabled ones
    // down. Each module seeds/starts/monitors its OWN resources in on_enable, so
    // this shell names no module and touches no module-specific data (onion
    // boundary); a module's enabled state is also durable across a restart.
    luma_module_kernel::apply_enabled_states(&state).await;

    // mDNS advertising is a runtime-toggleable setting (Réseau → Découverte locale).

    // The supervisor was built earlier (before the module services) so ported
    // modules resolve as client proxies to their sidecars.
    let app = api::router(state.clone(), supervisor.clone());

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    info!("LUMA listening on http://{addr}  (API under /api)");

    // Bring up every installed out-of-process module whose enabled flag is on.
    // (mDNS advertising moved into the `dev.luma.mdns` module; install its .lmod
    // to advertise the server over `_luma._tcp` / `luma.local`.)
    supervisor.spawn_enabled(&*state);

    // `into_make_service_with_connect_info` so handlers can read the client's
    // socket address (LAN/WAN classification for playback sessions).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    // Drain before exiting: ask running jobs to cancel (each records itself
    // `cancelled` instead of showing up as "interrupted by server restart" at
    // the next boot) and give them a bounded window to observe the flag, then
    // stop the module sidecars (child processes survive their parent, so
    // skipping this orphans them).
    info!("shutting down: cancelling running jobs + stopping module processes");
    state.jobs.cancel_all();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while state.jobs.running_count() > 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    supervisor.stop_all();

    Ok(())
}

/// Resolves on SIGINT (Ctrl-C) or, on unix, SIGTERM (docker stop / systemd /
/// DSM package stop), so axum stops accepting connections and `main` can drain
/// jobs + sidecars instead of the process dying mid-write.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    info!("shutdown signal received");
}

/// Initialise tracing. Honours `RUST_LOG`, defaulting to info-level for our
/// crate. Logs to stdout **and** a daily-rolling file under `<data>/logs/`
/// (best-effort). Returns the appender guard, which must be held for the process
/// lifetime so buffered lines flush.
fn init_tracing(log_dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    // `librqbit=info` surfaces the embedded engine's tracker announces + peer
    // connection errors (why a torrent finds no peers). Bump to
    // `RUST_LOG=librqbit=debug` for the full swarm chatter.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("luma_server=info,tower_http=info,axum=info,librqbit=info")
    });

    // Best-effort rolling file layer (no ANSI colour codes on disk).
    let (file_layer, guard) = match std::fs::create_dir_all(log_dir) {
        Ok(()) => {
            let appender = tracing_appender::rolling::daily(log_dir, "luma.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            (Some(layer), Some(guard))
        }
        Err(e) => {
            // Tracing isn't initialised yet, so report to stderr directly.
            eprintln!(
                "warning: could not create log dir {} ({e}); file logging disabled",
                log_dir.display()
            );
            (None, None)
        }
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(file_layer)
        .init();

    guard
}
