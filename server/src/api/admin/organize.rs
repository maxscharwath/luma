//! `/api/admin/organize/*` file naming templates + the library rename tool.
//! Gated on `library.manage` (it moves library files). The naming templates
//! are stored as settings; this exposes them with a live sample and drives the
//! preview + apply of a bulk rename.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::api::error::json_error;
use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::model::{
    NamingTemplatesView, NamingView, OrganizePlan, OrganizeResult, Permission, SampleBody, User,
};
use crate::services::organize::{self, naming::NamingTemplates};
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/organize/naming", get(get_naming).put(save_naming))
        .route("/organize/sample", post(sample))
        .route("/organize/preview", get(preview))
        .route("/organize/apply", post(apply))
}

fn require(user: &User) -> Result<(), Response> {
    if user.can(Permission::LibraryManage) {
        Ok(())
    } else {
        super::require(user, Permission::LibraryManage)
    }
}

fn view_of(tpl: &NamingTemplates) -> NamingTemplatesView {
    NamingTemplatesView {
        movie_folder: tpl.movie_folder.clone(),
        movie_file: tpl.movie_file.clone(),
        series_folder: tpl.series_folder.clone(),
        season_folder: tpl.season_folder.clone(),
        episode_file: tpl.episode_file.clone(),
    }
}

fn templates_of(body: &NamingTemplatesView) -> NamingTemplates {
    NamingTemplates {
        movie_folder: body.movie_folder.clone(),
        movie_file: body.movie_file.clone(),
        series_folder: body.series_folder.clone(),
        season_folder: body.season_folder.clone(),
        episode_file: body.episode_file.clone(),
    }
}

/// `GET /api/admin/organize/naming` current templates + a sample.
pub async fn get_naming(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user)?;
    let tpl = NamingTemplates::from_settings(&state.settings);
    Ok(Json(NamingView { templates: view_of(&tpl), sample: organize::sample(&tpl) }).into_response())
}

/// `POST /api/admin/organize/sample` render the sample for the given (unsaved)
/// templates, for the live preview as the admin types.
pub async fn sample(
    State(_state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SampleBody>,
) -> Result<Response, Response> {
    require(&user)?;
    Ok(Json(organize::sample(&templates_of(&body))).into_response())
}

/// `PUT /api/admin/organize/naming` persist the templates.
pub async fn save_naming(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NamingTemplatesView>,
) -> Result<Response, Response> {
    require(&user)?;
    let mut patch: BTreeMap<String, Value> = BTreeMap::new();
    patch.insert("namingMovieFolder".into(), json!(body.movie_folder.trim()));
    patch.insert("namingMovieFile".into(), json!(body.movie_file.trim()));
    patch.insert("namingSeriesFolder".into(), json!(body.series_folder.trim()));
    patch.insert("namingSeasonFolder".into(), json!(body.season_folder.trim()));
    patch.insert("namingEpisodeFile".into(), json!(body.episode_file.trim()));
    state.settings.set_patch(&state.db, patch);
    Ok(Json(json!({ "ok": true })).into_response())
}

/// `GET /api/admin/organize/preview` the non-destructive rename plan.
pub async fn preview(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user)?;
    let plan: OrganizePlan = blocking(move || organize::plan(&state)).await?;
    Ok(Json(plan).into_response())
}

/// `POST /api/admin/organize/apply` execute the rename (destructive).
pub async fn apply(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user)?;
    let result: OrganizeResult = match tokio::task::spawn_blocking(move || {
        organize::apply(&state, &|line| tracing::info!(target: "organize", "{line}"))
    })
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(json_error(axum::http::StatusCode::BAD_REQUEST, &format!("{e:#}"))),
        Err(_) => {
            return Err(json_error(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal error",
            ))
        }
    };
    Ok(Json(result).into_response())
}
