//! On-device subtitle generation endpoints: kick off a Whisper transcription or an
//! LLM translation, poll its live progress, cancel it, and list/serve/delete the
//! generated tracks. Generation is fire-and-poll: `generate` registers the work and
//! returns a `genId` immediately, then runs on a blocking thread reporting progress
//! into [`crate::services::subtitles::GenRegistry`]; the client polls `generations`.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::json_error;
use crate::api::util::query;
use crate::db;
use crate::services::settings;
use crate::services::subtitles::{self, GenMode, GenSpec, Quality};
use crate::state::SharedState;
use axum::routing::{delete, get, post};
use axum::Router;

/// On-device subtitle generation (Whisper) plus management of downloaded tracks.
/// Authenticated subtitle generation/management endpoints (gated by the session
/// middleware).
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/items/{id}/subtitles/generate", post(generate))
        .route("/items/{id}/subtitles/capabilities", get(capabilities))
        .route("/items/{id}/subtitles/generations", get(generations))
        .route("/items/{id}/subtitles/generations/{gen}", delete(cancel_generation))
        .route("/items/{id}/subtitles/downloaded", get(list))
        .route("/items/{id}/subtitles/downloaded/{dl}", delete(delete_downloaded))
}

/// Public: serve a generated/downloaded subtitle's WebVTT bytes. The player
/// fetches this URL as a plain `fetch()` (and can't attach a bearer), so like
/// the embedded-subtitle + stream byte routes it stays outside the session gate.
pub fn public_routes() -> Router<SharedState> {
    Router::new().route("/items/{id}/subtitles/dl/{dl}", get(file))
}

/// Talks to the Whisper module's `.kmod` sidecar (tv.kroma.whisper) over the port
/// bridge instead of transcribing in-process, so the heavy candle model + its
/// Metal/CUDA deps run out of the core. Transcription is long and drives live
/// progress + mid-run cancel, which don't fit `kroma-http`'s buffered request/
/// response, so a shared `whisper_jobs` DB row is the side-channel: the HTTP call
/// blocks on a helper thread while THIS thread polls the row to drive the
/// (thread-bound) `on_stage`/`on_progress` callbacks and writes the cancel flag.
pub struct WhisperClient {
    resolve: kroma_port_bridge::Resolver,
    pool: kroma_db::Pool,
}

impl WhisperClient {
    pub fn new(resolve: kroma_port_bridge::Resolver, pool: kroma_db::Pool) -> Self {
        Self { resolve, pool }
    }

    /// Whether the whisper sidecar is currently running (its port resolves).
    pub fn available(&self) -> bool {
        (self.resolve)().is_some()
    }
}

impl kroma_engine::ports::Whisper for WhisperClient {
    fn transcribe(
        &self,
        data_dir: &std::path::Path,
        model_spec: &str,
        input: &std::path::Path,
        track: u32,
        lang: Option<&str>,
        on_stage: &dyn Fn(&str),
        on_progress: &dyn Fn(usize, usize),
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Option<String> {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc::TryRecvError;
        use std::time::Duration;

        let (base, token) = (self.resolve)()?;
        kroma_whisper::ensure_jobs_table(&self.pool);
        // A per-run coordination row; nanosecond clock + track avoids collisions
        // across concurrent generations.
        let job_id = format!(
            "wj-{}-{track}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        if let Ok(conn) = self.pool.get() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO whisper_jobs (id, stage, done, total, cancel) VALUES (?1,'',0,0,0)",
                [&job_id],
            );
        }

        // The blocking HTTP call (minutes) runs on a helper thread; its result
        // returns over the channel so THIS thread can poll progress meanwhile.
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let body = serde_json::json!({
                "job_id": job_id,
                "data_dir": data_dir.to_string_lossy(),
                "model_spec": model_spec,
                "input": input.to_string_lossy(),
                "track": track,
                "lang": lang,
            });
            std::thread::spawn(move || {
                let text: Option<String> = kroma_http::Fetch::new()
                    .header("authorization", format!("Bearer {token}"))
                    .max_time(3 * 60 * 60)
                    .post_json(&format!("{base}/_port/whisper/transcribe"), &body)
                    .and_then(|r| r.ensure_ok())
                    .and_then(|r| r.json::<Option<String>>())
                    .ok()
                    .flatten();
                let _ = tx.send(text);
            });
        }

        let mut last_stage = String::new();
        let result = loop {
            match rx.try_recv() {
                Ok(text) => break text,
                Err(TryRecvError::Disconnected) => break None,
                Err(TryRecvError::Empty) => {}
            }
            // One pooled connection per tick: push the cancel flag (if latched)
            // then read progress off the same row.
            if let Ok(conn) = self.pool.get() {
                if cancel.load(Ordering::Relaxed) {
                    let _ = conn.execute("UPDATE whisper_jobs SET cancel = 1 WHERE id = ?1", [&job_id]);
                }
                if let Ok((stage, done, total)) = conn.query_row(
                    "SELECT stage, done, total FROM whisper_jobs WHERE id = ?1",
                    [&job_id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
                ) {
                    if !stage.is_empty() && stage != last_stage {
                        on_stage(&stage);
                        last_stage = stage;
                    }
                    if total > 0 {
                        on_progress(done as usize, total as usize);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        };

        if let Ok(conn) = self.pool.get() {
            let _ = conn.execute("DELETE FROM whisper_jobs WHERE id = ?1", [&job_id]);
        }
        result
    }
}

/// A generated/cached subtitle as the client sees it, with its WebVTT URL.
#[derive(Debug, Serialize)]
pub struct DownloadedSubView {
    pub id: String,
    pub language: Option<String>,
    pub label: String,
    pub provider: String,
    pub url: String,
}

fn to_view(item_id: &str, s: db::DownloadedSub) -> DownloadedSubView {
    DownloadedSubView {
        url: format!("/api/items/{item_id}/subtitles/dl/{}.vtt", s.id),
        id: s.id,
        language: s.language,
        label: s.label,
        provider: s.provider,
    }
}

/// Which generation actions this server build + config enable (so the player hides
/// empty buttons). `transcribe` needs the in-process Whisper feature; `translate`
/// needs a default LLM provider configured (the admin IA page).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubCapabilities {
    pub transcribe: bool,
    pub translate: bool,
}

/// `GET /api/items/:id/subtitles/capabilities`. Server config, not item-specific,
/// but kept under the item path for client convenience.
pub async fn capabilities(State(state): State<SharedState>, Path(_id): Path<String>) -> Response {
    // Transcription is available when the whisper sidecar (tv.kroma.whisper .kmod)
    // is installed + running, not on a compile-time core feature.
    let transcribe = kroma_module_host::service::<WhisperClient>(&state)
        .is_some_and(|w| w.available());
    let translate = settings::default_provider(&state.settings).is_some();
    Json(SubCapabilities { transcribe, translate }).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReq {
    /// `"transcribe"` (default) | `"translate"`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Target language label, e.g. "Français".
    pub lang: String,
    /// Transcribe: spoken language to force (name or code); omit to auto-detect.
    #[serde(default)]
    pub spoken_lang: Option<String>,
    /// Transcribe: model tier `"fast"` | `"balanced"` (default) | `"accurate"`.
    #[serde(default)]
    pub quality: Option<String>,
    /// Transcribe: audio-relative track index (default 0).
    #[serde(default)]
    pub audio_track: Option<u32>,
    /// Translate: the embedded subtitle track index to translate from.
    #[serde(default)]
    pub source_track: Option<usize>,
    /// Translate: a generated/cached subtitle id to translate from.
    #[serde(default)]
    pub source_sub_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenStarted {
    gen_id: String,
}

/// `POST /api/items/:id/subtitles/generate` → register + start a generation, return
/// its `genId`. The work runs on a blocking thread; poll `generations` for progress.
pub async fn generate(State(state): State<SharedState>, Path(id): Path<String>, Json(req): Json<GenerateReq>) -> Response {
    let item = match query(&state.db, {
        let id = id.clone();
        move |pool| db::get_item(&pool, &id)
    })
    .await
    {
        Ok(Some(it)) => it,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "item not found"),
        Err(resp) => return resp,
    };
    let Some(abs) = item.abs_path.clone() else {
        return json_error(StatusCode::NOT_FOUND, "no media file for item");
    };

    let mode = GenMode::parse(req.mode.as_deref().unwrap_or("transcribe"));

    // Cheap, synchronous config gates (no I/O) so the client gets a real error
    // instead of a genId that fails the instant it starts.
    if mode == GenMode::Transcribe && !cfg!(feature = "whisper-local") {
        return json_error(StatusCode::BAD_REQUEST, "on-device transcription is not available in this build");
    }
    if mode == GenMode::Translate && settings::default_provider(&state.settings).is_none() {
        return json_error(StatusCode::BAD_REQUEST, "no LLM provider configured for translation (admin IA page)");
    }

    let mode_label = if mode == GenMode::Translate { "translate" } else { "transcribe" };
    let target_lang = req.lang.trim().to_string();

    // Dedup: if an identical generation is already in flight (e.g. a double-click),
    // return its id instead of racing a second worker on the same output file/DB row.
    if let Some(existing) = state.subtitle_gen.find_running(&id, mode_label, &target_lang) {
        return (StatusCode::ACCEPTED, Json(GenStarted { gen_id: existing })).into_response();
    }

    let handle = state.subtitle_gen.start(&id, mode_label, Some(target_lang.clone()));
    let gen_id = handle.id().to_string();

    // Everything below runs OFF the request path. Translate resolves its source
    // WebVTT server-side (a cached track, or an embedded text track demuxed with
    // ffmpeg), which alone can take up to subtitles::TIMEOUT (150s); awaiting it
    // here would break fire-and-poll (the client would have nothing to poll and a
    // proxy/browser could time out). So we return the genId now and do ALL source
    // resolution + model work in a background task, marking the entry failed on error.
    let state = state.clone();
    let item_id = id.clone();
    let spoken_lang = req.spoken_lang.clone().filter(|s| !s.trim().is_empty());
    let quality = Quality::parse(req.quality.as_deref().unwrap_or("balanced"));
    let audio_track = req.audio_track.unwrap_or(0);
    tokio::spawn(async move {
        let source_vtt = if mode == GenMode::Translate {
            match resolve_source(&state, &item_id, &abs, &req).await {
                Ok(vtt) => Some(vtt),
                Err(reason) => {
                    handle.fail(&reason);
                    return;
                }
            }
        } else {
            None
        };
        let spec = GenSpec { mode, target_lang, spoken_lang, quality, audio_track, source_vtt };
        let settings = state.settings.clone();
        let data_dir = state.config.data_dir.clone();
        let pool = state.db.clone();
        // The whisper transcriber is the out-of-process sidecar proxy (registered
        // as a service in the composition root); translate-only generations don't
        // need it, so a missing one only fails a transcribe.
        let whisper = kroma_module_host::service::<WhisperClient>(&state);
        // The model (ffmpeg + Whisper / LLM) is blocking: run it on the blocking pool
        // and finalize the registry entry with its result.
        let _ = tokio::task::spawn_blocking(move || {
            let result = match whisper.as_ref() {
                Some(whisper) => subtitles::generate(
                    &settings,
                    &data_dir,
                    &pool,
                    &item_id,
                    std::path::Path::new(&abs),
                    &spec,
                    &handle,
                    whisper.as_ref(),
                ),
                None => Err("the Whisper module is not installed".to_string()),
            };
            match result {
                Ok(sub) => handle.done(&sub.id),
                Err(_) if handle.cancelled() => handle.fail("cancelled"),
                Err(reason) => {
                    // Surface the *real* reason (LLM/Whisper error, bad config, …) both
                    // in the server log and on the polled generation, instead of a blank
                    // "generation failed" the client can't act on.
                    tracing::warn!(item = %item_id, mode = mode_label, "subtitle generation failed: {reason}");
                    handle.fail(&reason);
                }
            }
        })
        .await;
    });

    (StatusCode::ACCEPTED, Json(GenStarted { gen_id })).into_response()
}

/// Resolve the WebVTT source for a translate request (cached id or embedded track).
/// Runs in the background task, so the `Err` is a human message recorded on the
/// generation via `handle.fail`, not an HTTP response.
async fn resolve_source(state: &SharedState, item_id: &str, abs: &str, req: &GenerateReq) -> Result<String, String> {
    if let Some(sub_id) = req.source_sub_id.as_deref().filter(|s| !s.trim().is_empty()) {
        let sub_id = sub_id.to_string();
        let sub = query(&state.db, move |pool| {
            let conn = pool.get()?;
            Ok(db::downloaded_sub(&conn, &sub_id)?)
        })
        .await
        .map_err(|_| "could not read the source subtitle from the database".to_string())?;
        let Some(sub) = sub else {
            return Err("source subtitle not found".to_string());
        };
        return match tokio::fs::read_to_string(&sub.path).await {
            Ok(text) => Ok(subtitles::to_vtt(&text)),
            Err(_) => Err("source subtitle file missing".to_string()),
        };
    }
    if let Some(track) = req.source_track {
        return match crate::api::stream::extract_webvtt(abs, track).await {
            Some(bytes) => Ok(subtitles::to_vtt(&String::from_utf8_lossy(&bytes))),
            None => Err("could not read the source subtitle track".to_string()),
        };
    }
    let _ = item_id;
    Err("translate needs a source subtitle (sourceTrack or sourceSubId)".to_string())
}

/// `GET /api/items/:id/subtitles/generations` → live + recently-finished generations.
pub async fn generations(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    Json(state.subtitle_gen.views_for(&id)).into_response()
}

/// `DELETE /api/items/:id/subtitles/generations/:gen` → request cancellation.
pub async fn cancel_generation(State(state): State<SharedState>, Path((_id, gen)): Path<(String, String)>) -> Response {
    if state.subtitle_gen.cancel(&gen) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        json_error(StatusCode::NOT_FOUND, "generation not found")
    }
}

/// `GET /api/items/:id/subtitles/downloaded` → this item's generated subtitles.
pub async fn list(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let item = id.clone();
    match query(&state.db, move |pool| {
        let conn = pool.get()?;
        Ok(db::downloaded_subs_for_item(&conn, &item)?)
    })
    .await
    {
        Ok(subs) => Json(subs.into_iter().map(|s| to_view(&id, s)).collect::<Vec<_>>()).into_response(),
        Err(resp) => resp,
    }
}

/// `DELETE /api/items/:id/subtitles/downloaded/:dl` → remove a generated track
/// (DB row + cached WebVTT file).
pub async fn delete_downloaded(State(state): State<SharedState>, Path((_id, dl)): Path<(String, String)>) -> Response {
    let dl_id = dl.trim_end_matches(".vtt").to_string();
    let pool = state.db.clone();
    let path = match tokio::task::spawn_blocking(move || db::delete_downloaded_sub(&pool, &dl_id)).await {
        Ok(Ok(p)) => p,
        _ => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not delete subtitle"),
    };
    match path {
        Some(p) => {
            let _ = tokio::fs::remove_file(&p).await;
            StatusCode::NO_CONTENT.into_response()
        }
        None => json_error(StatusCode::NOT_FOUND, "subtitle not found"),
    }
}

/// `GET /api/items/:id/subtitles/dl/:dl.vtt` → serve a cached generated WebVTT.
pub async fn file(State(state): State<SharedState>, Path((_id, dl)): Path<(String, String)>) -> Response {
    let dl_id = dl.trim_end_matches(".vtt").to_string();
    let sub = match query(&state.db, move |pool| {
        let conn = pool.get()?;
        Ok(db::downloaded_sub(&conn, &dl_id)?)
    })
    .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "subtitle not found"),
        Err(resp) => return resp,
    };
    match tokio::fs::read(&sub.path).await {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => json_error(StatusCode::NOT_FOUND, "subtitle file missing"),
    }
}
