//! Bridges for the indexer's provider ports: `IndexerDbPort`, `IndexerSearchPort`
//! and `TorrentFetchPort` (consumed by torrents + acquisition). All take a
//! `&dyn HostCtx`, re-supplied locally on the provider side; the boundary types
//! derive serde.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use kroma_module_host::HostCtx;
use kroma_module_sdk::ports::{
    DownloadTarget, IndexerDbPort, IndexerRow, IndexerSearchPort, Query, SearchOutcome,
    TorrentFetchPort,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{call, call_raw, Resolver};

/// Routes the indexer sidecar mounts for its three provider ports.
pub fn indexer_routes<S: HostCtx + Clone + Send + Sync + 'static>(
    db: Arc<dyn IndexerDbPort>,
    search: Arc<dyn IndexerSearchPort>,
    fetch: Arc<dyn TorrentFetchPort>,
) -> Router<S> {
    Router::new()
        .route("/_port/indexerdb/list", post(list_h::<S>))
        .route("/_port/indexerdb/enabled", post(enabled_h::<S>))
        .route("/_port/indexerdb/get", post(get_h::<S>))
        .route("/_port/indexerdb/note", post(note_h::<S>))
        .route("/_port/indexersearch/search", post(search_h::<S>))
        .route("/_port/indexersearch/resolve", post(resolve_h::<S>))
        .route("/_port/torrentfetch/fetch", post(fetch_h::<S>))
        .layer(Extension(db))
        .layer(Extension(search))
        .layer(Extension(fetch))
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

// --- IndexerDbPort -----------------------------------------------------------

async fn list_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn IndexerDbPort>>,
) -> Json<Result<Vec<IndexerRow>, String>> {
    blocking_env(move || db.list_indexers(&host)).await
}

async fn enabled_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn IndexerDbPort>>,
) -> Json<Result<Vec<IndexerRow>, String>> {
    blocking_env(move || db.enabled_indexers(&host)).await
}

#[derive(Deserialize)]
struct IdReq {
    id: String,
}

async fn get_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn IndexerDbPort>>,
    Json(req): Json<IdReq>,
) -> Json<Result<Option<IndexerRow>, String>> {
    blocking_env(move || db.get_indexer(&host, &req.id)).await
}

#[derive(Deserialize)]
struct NoteReq {
    id: String,
    ok: bool,
    error: Option<String>,
    now_ms: i64,
}

async fn note_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(db): Extension<Arc<dyn IndexerDbPort>>,
    Json(req): Json<NoteReq>,
) -> Json<Result<(), String>> {
    blocking_env(move || db.note_indexer_result(&host, &req.id, req.ok, req.error.as_deref(), req.now_ms))
        .await
}

// --- IndexerSearchPort -------------------------------------------------------

#[derive(Deserialize)]
struct SearchReq {
    row: IndexerRow,
    query: Query,
    categories: Vec<u32>,
}

async fn search_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(search): Extension<Arc<dyn IndexerSearchPort>>,
    Json(req): Json<SearchReq>,
) -> Json<Result<SearchOutcome, String>> {
    blocking_env(move || search.search(&host, &req.row, &req.query, &req.categories)).await
}

#[derive(Deserialize)]
struct ResolveReq {
    row: IndexerRow,
    title: String,
    details_url: Option<String>,
    magnet_or_url: String,
}

async fn resolve_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(search): Extension<Arc<dyn IndexerSearchPort>>,
    Json(req): Json<ResolveReq>,
) -> Json<Result<DownloadTarget, String>> {
    blocking_env(move || {
        search.resolve_download(&host, &req.row, &req.title, req.details_url.as_deref(), &req.magnet_or_url)
    })
    .await
}

// --- TorrentFetchPort (tri-state Option<Result<..>>) -------------------------

#[derive(Serialize, Deserialize, Default)]
struct FetchResp {
    /// False = "not this port's indexer" (the caller does a plain fetch).
    found: bool,
    error: Option<String>,
    data: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct FetchReq {
    indexer_id: String,
    url: String,
}

async fn fetch_h<S: HostCtx + Clone + Send + Sync + 'static>(
    State(host): State<S>,
    Extension(fetch): Extension<Arc<dyn TorrentFetchPort>>,
    Json(req): Json<FetchReq>,
) -> Json<FetchResp> {
    let resp = tokio::task::spawn_blocking(move || fetch.fetch_torrent(&host, &req.indexer_id, &req.url))
        .await
        .ok()
        .flatten();
    Json(match resp {
        None => FetchResp::default(),
        Some(Ok(data)) => FetchResp { found: true, error: None, data: Some(data) },
        Some(Err(e)) => FetchResp { found: true, error: Some(format!("{e:#}")), data: None },
    })
}

// --- Consumer-side clients ---------------------------------------------------

pub struct IndexerDbClient {
    resolve: Resolver,
}
pub struct IndexerSearchClient {
    resolve: Resolver,
}
pub struct TorrentFetchClient {
    resolve: Resolver,
}

impl IndexerDbClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}
impl IndexerSearchClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}
impl TorrentFetchClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

impl IndexerDbPort for IndexerDbClient {
    fn list_indexers(&self, _host: &dyn HostCtx) -> anyhow::Result<Vec<IndexerRow>> {
        call(&self.resolve, "indexerdb/list", &json!({}))
    }
    fn enabled_indexers(&self, _host: &dyn HostCtx) -> anyhow::Result<Vec<IndexerRow>> {
        call(&self.resolve, "indexerdb/enabled", &json!({}))
    }
    fn get_indexer(&self, _host: &dyn HostCtx, id: &str) -> anyhow::Result<Option<IndexerRow>> {
        call(&self.resolve, "indexerdb/get", &json!({ "id": id }))
    }
    fn note_indexer_result(
        &self,
        _host: &dyn HostCtx,
        id: &str,
        ok: bool,
        error: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        call(
            &self.resolve,
            "indexerdb/note",
            &json!({ "id": id, "ok": ok, "error": error, "now_ms": now_ms }),
        )
    }
}

impl IndexerSearchPort for IndexerSearchClient {
    fn search(
        &self,
        _host: &dyn HostCtx,
        row: &IndexerRow,
        query: &Query,
        categories: &[u32],
    ) -> anyhow::Result<SearchOutcome> {
        call(
            &self.resolve,
            "indexersearch/search",
            &json!({ "row": row, "query": query, "categories": categories }),
        )
    }
    fn resolve_download(
        &self,
        _host: &dyn HostCtx,
        row: &IndexerRow,
        title: &str,
        details_url: Option<&str>,
        magnet_or_url: &str,
    ) -> anyhow::Result<DownloadTarget> {
        call(
            &self.resolve,
            "indexersearch/resolve",
            &json!({ "row": row, "title": title, "details_url": details_url, "magnet_or_url": magnet_or_url }),
        )
    }
}

impl TorrentFetchPort for TorrentFetchClient {
    fn fetch_torrent(
        &self,
        _host: &dyn HostCtx,
        indexer_id: &str,
        url: &str,
    ) -> Option<anyhow::Result<Vec<u8>>> {
        let resp: FetchResp = call_raw(
            &self.resolve,
            "torrentfetch/fetch",
            &json!({ "indexer_id": indexer_id, "url": url }),
        )
        .ok()?;
        if !resp.found {
            return None;
        }
        match resp.error {
            Some(e) => Some(Err(anyhow::anyhow!(e))),
            None => Some(Ok(resp.data.unwrap_or_default())),
        }
    }
}
