//! Online subtitle endpoints: search a provider, download a chosen track (cached
//! as WebVTT + recorded), list an item's downloaded tracks, and serve them. The
//! provider work shells out via curl, so it runs on a blocking thread.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::json_error;
use crate::api::util::{blocking, query};
use crate::db;
use crate::services::settings;
use crate::services::subtitles::{self, Creds};
use crate::state::SharedState;

/// OpenSubtitles credentials from the first configured `opensubtitles` provider.
fn creds(state: &SharedState) -> Creds {
    settings::subtitle_providers(&state.settings)
        .into_iter()
        .find(|p| p.kind == "opensubtitles")
        .map(|p| Creds { os_api_key: p.api_key, os_username: p.username, os_password: p.password })
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Comma-separated language codes (e.g. `fr,en`); empty = all.
    #[serde(default)]
    pub lang: Option<String>,
}

/// `GET /api/items/:id/subtitles/search?lang=fr,en` → provider hits for this title.
pub async fn search(State(state): State<SharedState>, Path(id): Path<String>, Query(q): Query<SearchQuery>) -> Response {
    let item = match query(&state.db, move |pool| db::get_item(&pool, &id)).await {
        Ok(Some(item)) => item,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "item not found"),
        Err(resp) => return resp,
    };
    let c = creds(&state);
    if c.os_api_key.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "no subtitle provider configured (set the OpenSubtitles API key in admin settings)");
    }
    let langs: Vec<String> = q
        .lang
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let title = item.title.clone();
    let year = item.year.map(|y| y as i64);
    match blocking(move || Ok(subtitles::search(&c, &title, year, &langs))).await {
        Ok(hits) => Json(hits).into_response(),
        Err(resp) => resp,
    }
}

#[derive(Debug, Deserialize)]
pub struct DownloadReq {
    pub provider: String,
    /// Provider-specific id from the search hit (`RemoteSub.id`).
    pub remote_id: String,
    pub language: Option<String>,
    pub label: String,
}

/// A downloaded subtitle as the client sees it, with its WebVTT URL.
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

/// `POST /api/items/:id/subtitles/download` → fetch + cache the chosen track.
pub async fn download(State(state): State<SharedState>, Path(id): Path<String>, Json(req): Json<DownloadReq>) -> Response {
    let c = creds(&state);
    if c.os_api_key.is_empty() || c.os_username.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "OpenSubtitles credentials are not configured (admin settings)");
    }
    let data_dir = state.config.data_dir.clone();
    let pool = state.db.clone();
    let item = id.clone();
    let DownloadReq { provider, remote_id, language, label } = req;
    let res = blocking(move || {
        Ok(subtitles::download(
            &c,
            &data_dir,
            &pool,
            &item,
            &provider,
            &remote_id,
            language.as_deref(),
            &label,
        ))
    })
    .await;
    match res {
        Ok(Some(sub)) => Json(to_view(&id, sub)).into_response(),
        Ok(None) => json_error(StatusCode::BAD_GATEWAY, "download failed (provider error, quota, or bad credentials)"),
        Err(resp) => resp,
    }
}

/// Which subtitle actions are configured (so the player hides empty buttons).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubCapabilities {
    /// An OpenSubtitles provider with an API key → "Search online".
    pub search: bool,
    /// A Whisper provider (cloud with a key, or local with a model/binary) → transcribe.
    pub transcribe: bool,
    /// A translate provider AND a default LLM configured → translate.
    pub translate: bool,
}

/// `GET /api/items/:id/subtitles/capabilities` → which actions the configured
/// providers enable. Not item-specific (server config), but kept under the item
/// path for consistency.
pub async fn capabilities(State(state): State<SharedState>, Path(_id): Path<String>) -> Response {
    let providers = settings::subtitle_providers(&state.settings);
    let search = providers.iter().any(|p| p.kind == "opensubtitles" && !p.api_key.trim().is_empty());
    let transcribe = providers.iter().any(|p| match p.kind.as_str() {
        "whisper" => !p.api_key.trim().is_empty(),
        "whisperLocal" => !p.model.trim().is_empty() || !p.base_url.trim().is_empty(),
        _ => false,
    });
    let translate = providers.iter().any(|p| p.kind == "translate") && settings::default_provider(&state.settings).is_some();
    Json(SubCapabilities { search, transcribe, translate }).into_response()
}

/// `GET /api/items/:id/subtitles/downloaded` → this item's cached online subs.
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReq {
    /// Which provider to use (else the first configured AI provider).
    pub provider_id: Option<String>,
    /// Target language (name or code), e.g. "French".
    pub lang: String,
    /// For `translate`: the source track's WebVTT text to translate.
    pub source_vtt: Option<String>,
    /// For `whisper`: the audio-relative track to transcribe (default 0).
    pub audio_track: Option<u32>,
}

/// `POST /api/items/:id/subtitles/generate` → transcribe or translate with an AI
/// provider, caching the result like a download. SLOW (ffmpeg + model); a long
/// movie via cloud Whisper can exceed the request budget - prefer short content
/// or a local backend for full films.
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
    let providers = settings::subtitle_providers(&state.settings);
    let provider = req
        .provider_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|pid| providers.iter().find(|p| p.id == pid).cloned())
        .or_else(|| providers.into_iter().find(|p| subtitles::is_ai_kind(&p.kind)));
    let Some(provider) = provider else {
        return json_error(StatusCode::BAD_REQUEST, "no AI subtitle provider configured (add one in admin settings)");
    };
    let settings_clone = state.settings.clone();
    let data_dir = state.config.data_dir.clone();
    let pool = state.db.clone();
    let item_id = id.clone();
    let lang = req.lang.clone();
    let audio_track = req.audio_track.unwrap_or(0);
    let source = req.source_vtt.clone();
    let res = blocking(move || {
        Ok(subtitles::generate(
            &settings_clone,
            &provider,
            &data_dir,
            &pool,
            &item_id,
            std::path::Path::new(&abs),
            audio_track,
            &lang,
            source.as_deref(),
        ))
    })
    .await;
    match res {
        Ok(Some(sub)) => Json(to_view(&id, sub)).into_response(),
        Ok(None) => json_error(StatusCode::BAD_GATEWAY, "generation failed (provider error, missing backend, or unsupported)"),
        Err(resp) => resp,
    }
}

/// `GET /api/items/:id/subtitles/dl/:dl.vtt` → serve a cached downloaded WebVTT.
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
