//! `/api/reports` user-submitted problem reports (the "signaler un probleme"
//! flow). Any authenticated user (`playback`) can file a report on a movie / show
//! / episode; the admin triage queue lives under `/api/admin/reports`
//! (`crate::api::admin::reports`). The reported title is resolved + snapshotted
//! server-side, so a client can't spoof it and a since-deleted title still 404s.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::api::error::lerr;
use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::db;
use crate::i18n;
use crate::infra::events::ServerEvent;
use crate::model::{CreateReportBody, Kind, MediaItem, Permission, ReportSubjectKind, User};
use crate::services::auth::random_token;
use crate::services::jobs::now_ms;
use crate::services::scan::short_hash;
use crate::state::SharedState;

/// Free-text message cap (chars), so a report note can't balloon the row.
const MAX_MESSAGE: usize = 2000;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/reports", post(create))
        .route("/reports/mine", get(list_mine))
}

fn locale(user: &User) -> &'static str {
    user.language.as_deref().and_then(i18n::normalize).unwrap_or(i18n::DEFAULT_LOCALE)
}

fn require(user: &User, perm: Permission) -> Result<(), Response> {
    if user.can(perm) {
        Ok(())
    } else {
        Err(lerr(locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

/// A human-readable snapshot label for the reported subject. Episodes get a
/// `Show S01E02 - Episode title` line; movies use their own title.
fn subject_label(item: &MediaItem) -> String {
    if item.kind == Kind::Episode {
        let show = item.show_title.as_deref().unwrap_or(item.title.as_str());
        if let (Some(s), Some(e)) = (item.season, item.episode) {
            let ep = item.episode_title.as_deref().unwrap_or(item.title.as_str());
            return format!("{show} S{s:02}E{e:02} - {ep}");
        }
    }
    item.title.clone()
}

/// `POST /api/reports` file a problem report. Open to any user with `playback`
/// (the default). The subject title is resolved from the catalog (404 when the
/// movie/show/episode is unknown), never trusted from the client.
pub async fn create(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateReportBody>,
) -> Result<Response, Response> {
    require(&user, Permission::Playback)?;
    let loc = locale(&user);
    let kind = body.subject_kind;
    let subject_id = body.subject_id.clone();
    let category = body.category;
    let message = body
        .message
        .map(|m| m.trim().chars().take(MAX_MESSAGE).collect::<String>())
        .filter(|m| !m.is_empty());
    let uid = user.id.clone();

    let created = query(&state.db, move |pool| {
        let title = match kind {
            ReportSubjectKind::Show => db::show_title(&pool, &subject_id)?,
            ReportSubjectKind::Movie | ReportSubjectKind::Episode => {
                db::get_item(&pool, &subject_id)?.map(|it| subject_label(&it))
            }
        };
        let Some(title) = title else {
            return Ok(None);
        };
        let id = short_hash(&format!("report|{subject_id}|{}", random_token()));
        db::insert_report(
            &pool,
            &db::NewReport {
                id: id.clone(),
                subject_kind: kind,
                subject_id,
                subject_title: title,
                category,
                message,
                reported_by: Some(uid),
            },
            now_ms(),
        )?;
        let conn = pool.get()?;
        db::get_report(&conn, &id).map_err(Into::into)
    })
    .await?;

    match created {
        Some(report) => {
            state.events.publish(ServerEvent::ReportUpdated {
                id: report.id.clone(),
                status: report.status.as_str().into(),
            });
            Ok(Json(report).into_response())
        }
        None => Err(lerr(loc, StatusCode::NOT_FOUND, "error.itemNotFound")),
    }
}

/// `GET /api/reports/mine` the caller's own reports, newest-first.
pub async fn list_mine(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user, Permission::Playback)?;
    let uid = user.id.clone();
    let reports = query(&state.db, move |pool| {
        let conn = pool.get()?;
        db::list_reports(&conn, Some(&uid)).map_err(Into::into)
    })
    .await?;
    Ok(Json(reports).into_response())
}
