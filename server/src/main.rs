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

/// Resolve the dev.luma.torrents download manager from the host service
/// registry. It was a direct `AppState` field until the acquisition vertical
/// moved into the module crate; the download admin routes (and the grab path)
/// now look it up by type through the `HostCtx` seam, exactly like every other
/// module service. Kept as an extension method so the many `state.downloads`
/// call sites read the same after the move.
pub(crate) trait DownloadsExt {
    fn downloads(&self) -> std::sync::Arc<luma_torrent::DownloadManager>;
}

impl DownloadsExt for luma_engine::state::SharedState {
    fn downloads(&self) -> std::sync::Arc<luma_torrent::DownloadManager> {
        luma_module_host::service::<luma_torrent::DownloadManager>(&**self)
            .expect("download manager registered")
    }
}

/// Composition-root adapter: wraps the vector module's embedder into the engine's
/// [`luma_engine::ports::Embedder`] port, so `AppState` holds the capability
/// without the core naming the concrete embedder crate.
struct EmbedderPort(std::sync::Arc<dyn luma_vector::Embedder>);
impl luma_engine::ports::Embedder for EmbedderPort {
    fn dim(&self) -> usize {
        self.0.dim()
    }
    fn embed(&self, text: &str) -> Vec<f32> {
        self.0.embed(text)
    }
    fn relevance_floor(&self) -> f32 {
        self.0.relevance_floor()
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
        rand::thread_rng()
            .sample_iter(rand::distributions::Alphanumeric)
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
        });

    // Build the module services + peer ports the composition root owns, so the
    // core (luma-engine) names no module: the Remote connector, the VPN bridge, and
    // the VpnProxy / TorrentFetch ports. AppState builds the download manager (its
    // one direct module field) and merges these in.
    let mut module_services: std::collections::HashMap<
        std::any::TypeId,
        std::sync::Arc<dyn std::any::Any + Send + Sync>,
    > = std::collections::HashMap::new();
    let remote = luma_remote::RemoteAccess::new(config.data_dir.clone());
    module_services.insert(std::any::TypeId::of::<luma_remote::RemoteAccess>(), remote);
    // VPN runs out-of-process (the dev.luma.vpn .lmod); resolve VpnProxyPort as a
    // client proxy to its sidecar (indexer + torrents consume it).
    let vpn_proxy: std::sync::Arc<dyn luma_module_sdk::ports::VpnProxyPort> = {
        let sup = supervisor.clone();
        let tok = host_token.clone();
        let resolve: luma_port_bridge::Resolver = std::sync::Arc::new(move || {
            sup.port_of("dev.luma.vpn").map(|p| (format!("http://127.0.0.1:{p}"), tok.clone()))
        });
        std::sync::Arc::new(luma_port_bridge::VpnProxyClient::new(resolve))
    };
    let (tid, val) = luma_module_host::port_service(vpn_proxy);
    module_services.insert(tid, val);
    let torrent_fetch: std::sync::Arc<dyn luma_module_sdk::ports::TorrentFetchPort> =
        std::sync::Arc::new(luma_indexer::IndexerTorrentFetch);
    let (tid, val) = luma_module_host::port_service(torrent_fetch);
    module_services.insert(tid, val);
    // The Torznab search engine now runs out-of-process (the dev.luma.torznab
    // .lmod); resolve it as a client proxy that forwards over localhost to the
    // module's sidecar (discovered live via the supervisor's port map).
    let torznab: std::sync::Arc<dyn luma_module_sdk::ports::TorznabPort> = {
        let sup = supervisor.clone();
        let tok = host_token.clone();
        let resolve: luma_port_bridge::Resolver = std::sync::Arc::new(move || {
            sup.port_of("dev.luma.torznab").map(|p| (format!("http://127.0.0.1:{p}"), tok.clone()))
        });
        std::sync::Arc::new(luma_port_bridge::TorznabClient::new(resolve))
    };
    let (tid, val) = luma_module_host::port_service(torznab);
    module_services.insert(tid, val);
    // The indexer data + native-search ports, resolved by downloads / acquisition.
    let idx_db: std::sync::Arc<dyn luma_module_sdk::ports::IndexerDbPort> =
        std::sync::Arc::new(luma_indexer::IndexerDb);
    let (tid, val) = luma_module_host::port_service(idx_db);
    module_services.insert(tid, val);
    let idx_search: std::sync::Arc<dyn luma_module_sdk::ports::IndexerSearchPort> =
        std::sync::Arc::new(luma_indexer::IndexerSearch);
    let (tid, val) = luma_module_host::port_service(idx_search);
    module_services.insert(tid, val);
    // The download manager (dev.luma.torrents) is now a module service like the
    // rest: the composition root constructs it and injects it by type, so the
    // core (luma-engine) never names the torrent engine. The acquisition services
    // and the download admin routes resolve it through the HostCtx registry.
    let downloads = luma_torrent::DownloadManager::new(&config.data_dir);
    // Also expose it as the DownloadClientHost port, so the engine modules
    // (transmission / qBittorrent) register their kind without naming this crate.
    let dc_host: std::sync::Arc<dyn luma_module_sdk::ports::DownloadClientHost> = downloads.clone();
    let (tid, val) = luma_module_host::port_service(dc_host);
    module_services.insert(tid, val);
    // ...and as the DownloadVpnPort, so the VPN module reads the engine's VPN
    // status / seal check / restart without naming this crate.
    let dc_vpn: std::sync::Arc<dyn luma_module_sdk::ports::DownloadVpnPort> = downloads.clone();
    let (tid, val) = luma_module_host::port_service(dc_vpn);
    module_services.insert(tid, val);
    // ...and as the DownloadGrabPort + DownloadDbPort, so the Acquisition module
    // grabs releases + reads/updates the downloads ledger without naming this crate.
    let dc_grab: std::sync::Arc<dyn luma_module_sdk::ports::DownloadGrabPort> = downloads.clone();
    let (tid, val) = luma_module_host::port_service(dc_grab);
    module_services.insert(tid, val);
    let dc_db: std::sync::Arc<dyn luma_module_sdk::ports::DownloadDbPort> =
        std::sync::Arc::new(luma_torrent::DownloadDb);
    let (tid, val) = luma_module_host::port_service(dc_db);
    module_services.insert(tid, val);
    module_services.insert(std::any::TypeId::of::<luma_torrent::DownloadManager>(), downloads);
    // `luma_acquisition::JOBS` are the acquisition jobs (search / import / match),
    // registered alongside the core built-ins so the core roster names no module.
    let state = AppState::new(
        config,
        ffprobe_available,
        db,
        settings,
        std::sync::Arc::new(EmbedderPort(luma_vector::default_embedder()))
            as std::sync::Arc<dyn luma_engine::ports::Embedder>,
        module_services,
        luma_acquisition::JOBS,
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
    // (mDNS advertising moved into the `dev.luma.mdns` module — install its .lmod
    // to advertise the server over `_luma._tcp` / `luma.local`.)
    supervisor.spawn_enabled(&*state);

    // `into_make_service_with_connect_info` so handlers can read the client's
    // socket address (LAN/WAN classification for playback sessions).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .context("server error")?;

    Ok(())
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
