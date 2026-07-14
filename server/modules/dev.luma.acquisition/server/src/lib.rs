//! Acquisition orchestration: the quality profile from settings and the search
//! DISPATCH (interactive via [`search`]; the automatic wanted-list pass in
//! [`auto`]), plus grab + import. Its own module so disabling it gates the whole
//! search / grab / auto feature.
//!
//! SDK-only: acquisition names no sibling crate. It reaches the Downloads module
//! (grab / ledger) through [`DownloadGrabPort`](luma_module_sdk::ports::DownloadGrabPort)
//! + [`DownloadDbPort`](luma_module_sdk::ports::DownloadDbPort), the Indexers
//! module through [`IndexerSearchPort`](luma_module_sdk::ports::IndexerSearchPort)
//! + [`IndexerDbPort`](luma_module_sdk::ports::IndexerDbPort), all resolved at
//! runtime through the host port registry. The coupling stays one-way (those
//! modules never call acquisition), so there is no cycle.

// The axum `Response` is intentionally the Err type of request guards so handlers
// short-circuit with `?`; boxing every guard for `result_large_err` would churn
// dozens of signatures for no real gain on these error paths.
#![allow(clippy::result_large_err)]

pub mod auto;
pub mod dtos;
pub mod import;
pub mod jobs;
pub mod routes;
pub mod search;
mod serve;

pub use dtos::*;
pub use serve::acqsearch_routes;

use std::sync::Arc;
use std::time::Duration;

use luma_module_sdk::host::HostCtx;
use luma_module_sdk::scene::{Profile, Res};

use luma_module_sdk::engine::services::jobs::now_ms;
use luma_module_sdk::ports::IndexerRow;

const GB: u64 = 1_073_741_824;

/// The acquisition background jobs this module contributes to the app's job
/// registry (search / import / match). The binary passes this to
/// `AppState::new` so the core registers them without naming the module.
pub const JOBS: &[luma_module_sdk::engine::services::jobs::Builtin] =
    &[jobs::import::SPEC, jobs::search::SPEC, jobs::match_::SPEC];

/// This module's id, shared with `module.json` and the frontend package. The one
/// place callers (route gate, job guards) name the module.
pub const MODULE_ID: &str = "dev.luma.acquisition";

/// This module's registry entry (manifest + packaged icon embedded at compile time).
use luma_module_sdk::EmbeddedModule;
pub const MODULE: EmbeddedModule = luma_module_sdk::embedded_module!();

/// Resolve the Downloads module's grab surface (grab / gate / activate / drop /
/// list-files) from the host port registry. Acquisition reaches it only through
/// the SDK port, so it never names the torrents crate.
pub(crate) fn downloads<S: HostCtx>(
    state: &S,
) -> std::sync::Arc<dyn luma_module_sdk::ports::DownloadGrabPort> {
    luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::DownloadGrabPort>(state)
        .expect("download grab port registered")
}

/// Resolve the downloads-ledger read/write port (completed rows + status flips)
/// the import pass needs, through the SDK port registry.
pub(crate) fn download_db<S: HostCtx>(
    state: &S,
) -> std::sync::Arc<dyn luma_module_sdk::ports::DownloadDbPort> {
    luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::DownloadDbPort>(state)
        .expect("download db port registered")
}

/// Build the decision engine's profile from the admin settings.
pub fn profile_from_settings<S: HostCtx>(state: &S) -> Profile {
    let resolution = match state.setting_str("acqResolution", "1080p").as_str() {
        "720p" => Res::R720,
        "2160p" => Res::R2160,
        _ => Res::R1080,
    };
    let list = |key: &str| -> Vec<String> {
        state
            .setting_str(key, "")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect()
    };
    Profile {
        resolution,
        prefer_hevc: state.setting_bool("acqPreferHevc", true),
        min_seeders: state.setting_i64("acqMinSeeders", 2).max(0) as u32,
        max_size_bytes_movie: (state.setting_i64("acqMaxSizeGbMovie", 15).max(0) as u64) * GB,
        max_size_bytes_episode: (state.setting_i64("acqMaxSizeGbEpisode", 3).max(0) as u64) * GB,
        required_keywords: list("acqRequiredKeywords"),
        forbidden_keywords: list("acqForbiddenKeywords"),
    }
}

/// Run one query against one indexer, whatever its kind, returning normalized
/// releases. This is the single dispatch point the search pipelines call; the
/// native-vs-Torznab dispatch + type conversions live behind the indexer's
/// `IndexerSearchPort`, so acquisition never names the indexer/torznab crates.
pub fn search_indexer<S: HostCtx>(
    state: &S,
    row: &IndexerRow,
    query: &luma_module_sdk::ports::Query,
) -> anyhow::Result<Vec<luma_module_sdk::ports::Release>> {
    let search =
        luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerSearchPort>(state)
            .ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?;
    let outcome = search.search(state, row, query, &row.categories)?;
    // Healthy if we got releases (a partial per-path error alongside real
    // results must not flag the indexer as broken) or the sweep was clean.
    let note_ok = !outcome.releases.is_empty() || outcome.errors.is_empty();
    if let Some(idx) =
        luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerDbPort>(state)
    {
        let _ = idx.note_indexer_result(
            state,
            &row.id,
            note_ok,
            if note_ok { None } else { outcome.errors.first().map(String::as_str) },
            now_ms(),
        );
    }
    // Surface an all-error, no-result sweep as an error (so it reads as a broken
    // indexer, not "nothing found").
    if outcome.releases.is_empty() && !outcome.errors.is_empty() {
        anyhow::bail!("{}", outcome.errors.join("; "));
    }
    Ok(outcome.releases)
}

/// Resolve the grabbable target (magnet / .torrent URL) for a built-in release,
/// following the definition's `download` block if the search row carried no
/// direct link.
pub fn resolve_builtin_download<S: HostCtx>(
    state: &S,
    row: &IndexerRow,
    title: &str,
    details_url: Option<&str>,
    magnet_or_url: &str,
) -> anyhow::Result<String> {
    let search =
        luma_module_sdk::host::resolve_port::<dyn luma_module_sdk::ports::IndexerSearchPort>(state)
            .ok_or_else(|| anyhow::anyhow!("indexer module unavailable"))?;
    Ok(match search.resolve_download(state, row, title, details_url, magnet_or_url)? {
        luma_module_sdk::ports::DownloadTarget::Magnet(m) => m,
        luma_module_sdk::ports::DownloadTarget::TorrentUrl(u) => u,
    })
}

/// The Acquisition module's backend behavior: it serves the search / analyze /
/// add admin routes (behind its enabled-gate) and contributes the search /
/// import / match jobs. Disabling it 404s those routes and no-ops the jobs, so
/// the whole search / grab / auto feature is gated on this module. It reaches the
/// Downloads / Indexer modules through their SDK ports (see the module docs).
///
/// Generic over the host state `S: HostCtx`, like every module. In-core
/// (`S = SharedState`) the three passes run on the core JobManager via [`JOBS`];
/// out-of-process (`S = RemoteHost`, its `.lmod`) the sidecar calls [`start_cron`]
/// instead, since the core scheduler can't reach into the sidecar. It reaches the
/// Downloads / Indexer modules through their SDK ports (see the module docs).
pub struct AcquisitionModule;

#[luma_module_sdk::host::async_trait]
impl<S: HostCtx + Clone + Send + Sync + 'static> luma_module_sdk::host::ServerModule<S>
    for AcquisitionModule
{
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn admin_routes(&self, _host: &S) -> Option<axum::Router<S>> {
        Some(routes::routes::<S>())
    }
}

/// Run the search / import / match passes on resident timers. Called ONLY by the
/// out-of-process sidecar (from its bin), where the core JobManager can't reach:
/// in-core the same passes run via [`JOBS`], so running both would double every
/// pass. Spawn once per process; each tick idles while the module is disabled.
pub fn start_cron(host: Arc<dyn HostCtx>) {
    spawn_pass_loop(host.clone(), Duration::from_secs(30 * 60), "search", run_search);
    spawn_pass_loop(host.clone(), Duration::from_secs(60 * 60), "import", run_import);
    spawn_pass_loop(host, Duration::from_secs(6 * 60 * 60), "match", run_match);
}

/// Spawn a resident timer that runs `pass` every `period`, skipping while the
/// module is disabled. The pass is blocking (DB + network), so it runs on the
/// blocking pool; failures are logged, never fatal to the loop.
fn spawn_pass_loop(
    host: Arc<dyn HostCtx>,
    period: Duration,
    name: &'static str,
    pass: fn(&Arc<dyn HostCtx>) -> anyhow::Result<()>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(period).await;
            if !host.module_enabled(MODULE_ID) {
                continue;
            }
            let h = host.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = pass(&h) {
                    tracing::warn!(target: "acquisition", job = name, error = %format!("{e:#}"), "pass failed");
                }
            })
            .await;
        }
    });
}

fn run_search(host: &Arc<dyn HostCtx>) -> anyhow::Result<()> {
    auto::auto_search_pass(host, &|l| tracing::info!(target: "acquisition", "{l}"), &|| false)?;
    Ok(())
}

fn run_import(host: &Arc<dyn HostCtx>) -> anyhow::Result<()> {
    import::import_pass(host, &|l| tracing::info!(target: "acquisition", "{l}"))?;
    Ok(())
}

fn run_match(host: &Arc<dyn HostCtx>) -> anyhow::Result<()> {
    luma_module_sdk::engine::services::requests::availability_pass(host)?;
    Ok(())
}

/// This module's backend behavior, for the host's generic module roster. Generic
/// over the host state so both the in-core roster (`S = SharedState`) and the
/// `.lmod` binary (`S = RemoteHost`) construct it.
pub fn server_module<S: HostCtx + Clone + Send + Sync + 'static>(
) -> Box<dyn luma_module_sdk::host::ServerModule<S>> {
    Box::new(AcquisitionModule)
}
