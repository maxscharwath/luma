//! `/api/rematch/*` correcting a wrong TMDB match on one catalog element.
//!
//! Two endpoints behind `library.manage`: list the ranked TMDB candidates for an
//! element, and pin the right one (or clear the pin, restoring automatic
//! matching). Pinning re-runs the metadata stage in the background, so the
//! response is an ack; clients pick the new art up from the `ItemUpdated` /
//! `ShowUpdated` event.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::error::lerr;
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::i18n;
use crate::model::{Permission, User};
use crate::services::rematch::{self, Subject};
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/rematch/{kind}/{id}/candidates", get(candidates))
        .route("/rematch/{kind}/{id}", post(apply))
}

fn locale(user: &User) -> &'static str {
    user.language.as_deref().and_then(i18n::normalize).unwrap_or(i18n::DEFAULT_LOCALE)
}

/// Correcting metadata is a library-management action, not a settings one.
fn require_manage(user: &User) -> Result<(), Response> {
    if user.can(Permission::LibraryManage) {
        Ok(())
    } else {
        Err(lerr(locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

/// `movie` | `show` out of the path, or a 404 (an unknown kind addresses nothing).
fn subject_of(user: &User, kind: &str) -> Result<Subject, Response> {
    Subject::parse(kind)
        .ok_or_else(|| lerr(locale(user), StatusCode::NOT_FOUND, "error.itemNotFound"))
}

#[derive(Debug, Deserialize)]
pub struct CandidateParams {
    /// Free-text override for the search. Absent = search the parsed title.
    #[serde(default)]
    q: Option<String>,
}

/// `GET /api/rematch/{kind}/{id}/candidates?q=` ranked TMDB candidates.
pub async fn candidates(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path((kind, id)): Path<(String, String)>,
    Query(params): Query<CandidateParams>,
) -> Result<Response, Response> {
    require_manage(&user)?;
    let subject = subject_of(&user, &kind)?;
    let out = blocking(move || rematch::candidates(&state, subject, &id, params.q.as_deref()))
        .await?;
    Ok(Json(out).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyBody {
    /// The chosen TMDB id, or `null` to clear the pin and let matching resolve
    /// the element automatically again.
    #[serde(default)]
    tmdb_id: Option<u64>,
}

/// `POST /api/rematch/{kind}/{id}` pin (or clear) the match and re-enrich.
pub async fn apply(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path((kind, id)): Path<(String, String)>,
    Json(body): Json<ApplyBody>,
) -> Result<Response, Response> {
    require_manage(&user)?;
    let subject = subject_of(&user, &kind)?;
    blocking(move || rematch::apply(&state, subject, &id, body.tmdb_id)).await?;
    Ok(StatusCode::ACCEPTED.into_response())
}
