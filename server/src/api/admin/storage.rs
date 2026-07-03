//! Storage + cache management: volume totals, media/cache usage, and a
//! cache-clear action.

use std::path::Path;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::api::util::{blocking, query};
use crate::api::extract::AuthUser;
use crate::db;
use crate::model::Permission;
use crate::state::SharedState;
use axum::routing::{get, post};
use axum::Router;

/// Storage usage + cache maintenance. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/storage", get(storage))
        .route("/cache/clear", post(clear_cache))
        .route("/cache/reset-metadata", post(reset_metadata))
}

/// `GET /api/admin/storage` → volumes, totals, and cache usage.
pub async fn storage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let data_dir = state.config.data_dir.clone();
    let (volumes, media_bytes, transcode, images, counts) = query(&state.db, move |pool| {
        let volumes = crate::infra::metrics::read_disks();
        let media = db::total_media_bytes(&pool).unwrap_or(0).max(0) as u64;
        let transcode = dir_stats(&data_dir.join("hls"));
        let images = dir_stats(&data_dir.join("images"));
        let counts = db::metadata_counts(&pool).unwrap_or((0, 0, 0));
        Ok((volumes, media, transcode, images, counts))
    })
    .await?;

    let total: u64 = volumes.iter().map(|v| v.total_bytes).sum();
    let used: u64 = volumes.iter().map(|v| v.used_bytes).sum();
    let (transcode_bytes, _) = transcode;
    let (images_bytes, images_count) = images;
    let (enriched_items, enriched_shows, embeddings) = counts;
    Ok(Json(crate::api::dto::StorageInfo {
        volumes,
        total_bytes: total,
        used_bytes: used,
        available_bytes: total.saturating_sub(used),
        media_bytes,
        cache: crate::api::dto::CacheInfo {
            dir: state.config.data_dir.join("hls").to_string_lossy().into_owned(),
            bytes: transcode_bytes + images_bytes,
            limit: state.settings.get_str("cacheLimit", "80 Go"),
            transcode_limit: state.settings.get_str("transcodeCacheLimit", "20 Go"),
            transcode_bytes,
            images_bytes,
            images_count,
            enriched_items: enriched_items.max(0) as u64,
            enriched_shows: enriched_shows.max(0) as u64,
            embeddings: embeddings.max(0) as u64,
        },
    })
    .into_response())
}

/// `POST /api/admin/cache/clear` → wipe transcode + image caches.
pub async fn clear_cache(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let data_dir = state.config.data_dir.clone();
    let freed = blocking(move || {
        let transcode = data_dir.join("hls");
        let images = data_dir.join("images");
        let storyboards = data_dir.join("storyboards");
        let freed = dir_size(&transcode) + dir_size(&images) + dir_size(&storyboards);
        clear_dir(&transcode);
        clear_dir(&images);
        clear_dir(&storyboards);
        Ok(freed)
    })
    .await?;
    // The pipeline's skip logic is keyed on input signatures, not output presence,
    // so wiping these dirs would otherwise leave the ledger `done` and the outputs
    // gone forever. Re-queue the stages whose durable outputs we just deleted:
    // storyboards regenerate from local video (kick it now, it's gate-bounded);
    // TMDB art re-downloads on the next metadata run (re-queued, not forced, so a
    // disk-clear never stampedes TMDB).
    let now = crate::services::jobs::now_ms();
    let _ = query(&state.db, move |pool| {
        db::pipeline::requeue_stage(&pool, "storyboard", now)?;
        db::pipeline::requeue_stage(&pool, "metadata", now)?;
        Ok(())
    })
    .await;
    let _ = state.jobs.trigger(state.clone(), crate::services::jobs::JobKey("pipeline.storyboard"), "clear-cache");
    Ok(Json(json!({ "freedBytes": freed })).into_response())
}

/// `POST /api/admin/cache/reset-metadata` → drop every resolved TMDB metadata
/// (DB JSON, season casts and title embeddings) and the in-memory lookup cache,
/// forcing a full re-fetch on the next enrichment run. Does NOT delete on-disk
/// images use `clear_cache` for that. Returns how many rows were cleared.
pub async fn reset_metadata(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let now = crate::services::jobs::now_ms();
    let (items, shows) = query(&state.db, move |pool| {
        let cleared = db::reset_all_metadata(&pool)?;
        // The metadata signature (`title:year`) is unchanged by a reset, and the
        // embed signature is just the model dim, so neither stage would re-run on
        // its own leaving metadata NULL and embeddings gone forever. Re-queue both
        // ledgers so the enrich + embed actually happen again.
        db::pipeline::requeue_stage(&pool, "metadata", now)?;
        db::pipeline::requeue_stage(&pool, "embed", now)?;
        Ok(cleared)
    })
    .await?;
    state.metadata_cache.clear();
    // Kick the re-enrich now (this is a deliberate destructive action); `embed`
    // chains after `metadata` via its `AfterJob` trigger.
    let _ = state.jobs.trigger(state.clone(), crate::services::jobs::JobKey("pipeline.metadata"), "reset-metadata");
    Ok(Json(json!({ "items": items, "shows": shows })).into_response())
}

/// Recursive `(bytes, file_count)` of a directory tree (zero if missing).
fn dir_stats(path: &Path) -> (u64, u64) {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .fold((0u64, 0u64), |(bytes, count), m| (bytes + m.len(), count + 1))
}

/// Recursive byte size of a directory tree (0 if missing).
fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Remove a directory's contents (keeping the directory itself).
fn clear_dir(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}
