//! Bridges for the downloads module's provider ports: `DownloadGrabPort` (grab /
//! list-files / gate / activate / drop) and `DownloadDbPort` (the ledger
//! reads/writes acquisition's import pass needs). Provided by the torrents
//! sidecar, consumed by the acquisition sidecar. Every `&dyn HostCtx` argument is
//! dropped from the wire and re-supplied locally on the provider side (which runs
//! the call against its OWN host: the download manager + engine live there).

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use luma_module_host::HostCtx;
use luma_module_sdk::ports::{
    DownloadDbPort, DownloadGrabPort, DownloadRow, GrabSpec, TorrentFileEntry,
};
use serde::Deserialize;
use serde_json::json;

use crate::{call, call_raw, Resolver};

/// Routes the torrents sidecar mounts for its two download provider ports.
pub fn download_routes<S: HostCtx + Clone + Send + Sync + 'static>(
    grab: Arc<dyn DownloadGrabPort>,
    db: Arc<dyn DownloadDbPort>,
) -> Router<S> {
    Router::new()
        .route("/_port/downloadgrab/grab", post(grab_h::<S>))
        .route("/_port/downloadgrab/list_files", post(list_files_h::<S>))
        .route("/_port/downloadgrab/gate_open", post(gate_open_h::<S>))
        .route("/_port/downloadgrab/activate", post(activate_h::<S>))
        .route("/_port/downloadgrab/drop_data", post(drop_data_h::<S>))
        .route("/_port/downloaddb/completed", post(completed_h::<S>))
        .route("/_port/downloaddb/mark_imported", post(mark_imported_h::<S>))
        .route("/_port/downloaddb/set_status", post(set_status_h::<S>))
        .layer(Extension(grab))
        .layer(Extension(db))
}

/// Run a blocking provider call into the `Result<T, String>` wire envelope.
async fn blocking_env<T: Send + 'static>(
    job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Json<Result<T, String>> {
    Json(
        tokio::task::spawn_blocking(job)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| format!("{e:#}"))),
    )
}

// --- DownloadGrabPort --------------------------------------------------------

async fn grab_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(grab): Extension<Arc<dyn DownloadGrabPort>>,
    Json(spec): Json<GrabSpec>,
) -> Json<Result<DownloadRow, String>> {
    blocking_env(move || grab.grab(&host, spec)).await
}

#[derive(Deserialize)]
struct MagnetReq {
    magnet_or_url: String,
}

async fn list_files_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(grab): Extension<Arc<dyn DownloadGrabPort>>,
    Json(req): Json<MagnetReq>,
) -> Json<Result<Vec<TorrentFileEntry>, String>> {
    blocking_env(move || grab.list_files(&host, &req.magnet_or_url)).await
}

async fn gate_open_h<S: HostCtx + Clone + Send + Sync + 'static>(
    Extension(grab): Extension<Arc<dyn DownloadGrabPort>>,
) -> Json<bool> {
    Json(grab.gate_open())
}

async fn activate_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(grab): Extension<Arc<dyn DownloadGrabPort>>,
    Json(row): Json<DownloadRow>,
) -> Json<()> {
    // Infallible on the contract; run it on a blocking thread (engine add) and ack.
    let _ = tokio::task::spawn_blocking(move || grab.activate(&host, &row)).await;
    Json(())
}

async fn drop_data_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(grab): Extension<Arc<dyn DownloadGrabPort>>,
    Json(row): Json<DownloadRow>,
) -> Json<()> {
    let _ = tokio::task::spawn_blocking(move || grab.drop_data(&host, &row)).await;
    Json(())
}

// --- DownloadDbPort ----------------------------------------------------------

async fn completed_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn DownloadDbPort>>,
) -> Json<Result<Vec<DownloadRow>, String>> {
    blocking_env(move || db.completed_downloads(&host)).await
}

#[derive(Deserialize)]
struct MarkImportedReq {
    id: String,
    paths: Vec<String>,
    now_ms: i64,
}

async fn mark_imported_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn DownloadDbPort>>,
    Json(req): Json<MarkImportedReq>,
) -> Json<Result<(), String>> {
    blocking_env(move || db.mark_download_imported(&host, &req.id, &req.paths, req.now_ms)).await
}

#[derive(Deserialize)]
struct SetStatusReq {
    id: String,
    status: String,
    error: Option<String>,
}

async fn set_status_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn DownloadDbPort>>,
    Json(req): Json<SetStatusReq>,
) -> Json<Result<bool, String>> {
    blocking_env(move || db.set_download_status(&host, &req.id, &req.status, req.error.as_deref())).await
}

// --- Consumer-side clients ---------------------------------------------------

pub struct DownloadGrabClient {
    resolve: Resolver,
}
pub struct DownloadDbClient {
    resolve: Resolver,
}

impl DownloadGrabClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}
impl DownloadDbClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

impl DownloadGrabPort for DownloadGrabClient {
    fn grab(&self, _host: &dyn HostCtx, spec: GrabSpec) -> anyhow::Result<DownloadRow> {
        call(&self.resolve, "downloadgrab/grab", &spec)
    }
    fn list_files(
        &self,
        _host: &dyn HostCtx,
        magnet_or_url: &str,
    ) -> anyhow::Result<Vec<TorrentFileEntry>> {
        call(&self.resolve, "downloadgrab/list_files", &json!({ "magnet_or_url": magnet_or_url }))
    }
    fn gate_open(&self) -> bool {
        // A transient bridge hiccup shouldn't silently disable acquisition; grab()
        // re-checks the gate authoritatively on the provider side, so default open.
        call_raw(&self.resolve, "downloadgrab/gate_open", &json!({})).unwrap_or(true)
    }
    fn activate(&self, _host: &dyn HostCtx, row: &DownloadRow) {
        let _: anyhow::Result<()> = call_raw(&self.resolve, "downloadgrab/activate", row);
    }
    fn drop_data(&self, _host: &dyn HostCtx, row: &DownloadRow) {
        let _: anyhow::Result<()> = call_raw(&self.resolve, "downloadgrab/drop_data", row);
    }
}

impl DownloadDbPort for DownloadDbClient {
    fn completed_downloads(&self, _host: &dyn HostCtx) -> anyhow::Result<Vec<DownloadRow>> {
        call(&self.resolve, "downloaddb/completed", &json!({}))
    }
    fn mark_download_imported(
        &self,
        _host: &dyn HostCtx,
        id: &str,
        paths: &[String],
        now_ms: i64,
    ) -> anyhow::Result<()> {
        call(
            &self.resolve,
            "downloaddb/mark_imported",
            &json!({ "id": id, "paths": paths, "now_ms": now_ms }),
        )
    }
    fn set_download_status(
        &self,
        _host: &dyn HostCtx,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> anyhow::Result<bool> {
        call(
            &self.resolve,
            "downloaddb/set_status",
            &json!({ "id": id, "status": status, "error": error }),
        )
    }
}
