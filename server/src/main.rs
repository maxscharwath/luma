//! LUMA a self-hosted, direct-play media streaming server.
//!
//! Scans a media library (Plex-style movie/show detection), persists it in
//! SQLite, exposes metadata over a JSON REST API, and range-streams the original
//! files to clients. It never transcodes: clients decode HEVC/H.265/AV1
//! themselves. `ffprobe` is used only to read metadata.

mod api;
mod config;
mod db;
mod domain;
mod i18n;
mod infra;
mod model;
mod services;
mod state;

use std::sync::OnceLock;
use std::time::Instant;

use anyhow::Context;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;
use crate::state::AppState;

// On the Linux/musl single binary, musl's malloc is a global-lock design that
// collapses under our thread mix (tokio workers + rayon walks + candle tensors);
// mimalloc removes that contention. macOS dev keeps the system allocator.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Process start time, for the admin "Disponibilité" / uptime readout.
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// When this process started (monotonic). Seeded on first call.
pub fn process_started() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    process_started();
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
    let state = AppState::new(config, ffprobe_available, db, settings);
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

    // Managed Cloudflare Tunnel connector: bring the tunnel up at boot if the admin
    // enabled it with a token (installs with their own tunnel leave it off), and
    // keep it alive via a watchdog. No-op otherwise.
    state.remote.clone().spawn_boot(state.clone());

    // mDNS advertising is a runtime-toggleable setting (Réseau → Découverte locale).
    let local_discovery = state.settings.get_bool("localDiscovery", true);

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    info!("LUMA listening on http://{addr}  (API under /api)");

    // Advertise over mDNS so LAN clients can auto-discover us, unless disabled in
    // settings. Best-effort: held alive until the process exits; failure (no
    // multicast, etc.) is non-fatal.
    let _mdns = if local_discovery {
        match infra::discovery::advertise(addr.port(), "LUMA") {
            Ok(daemon) => Some(daemon),
            Err(e) => {
                warn!(error = %e, "mDNS advertising unavailable; clients must use an explicit address");
                None
            }
        }
    } else {
        info!("local discovery (mDNS) disabled in settings");
        None
    };

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
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("luma_server=info,tower_http=info,axum=info"));

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
