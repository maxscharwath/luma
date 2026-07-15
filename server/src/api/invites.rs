//! Registration invitations. After the bootstrap owner, registration is
//! invite-only: an admin with `users.manage` mints a single-use token the
//! invitee redeems on the join page.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::error::lerr;
use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::services::auth;
use crate::db;
use crate::i18n;
use crate::model::{Permission, User};
use crate::state::SharedState;
use axum::routing::{get, post};
use axum::Router;

/// Invitation management (registration is invite-only after the owner account).
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/invites", post(create_invite).get(list_invites))
        .route("/invites/{token}", get(check_invite).delete(delete_invite))
}

/// Default invite lifetime, and the bounds accepted from clients.
const INVITE_TTL_DAYS_DEFAULT: i64 = 7;

/// Gate a handler behind a permission → 403 when the user lacks it. Localised via
/// the user's account preference (these endpoints are always authenticated).
fn require(user: &User, perm: Permission) -> Result<(), Response> {
    if user.can(perm) {
        Ok(())
    } else {
        let loc = user.language.as_deref().and_then(i18n::normalize).unwrap_or(i18n::DEFAULT_LOCALE);
        Err(lerr(loc, StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteBody {
    /// Permissions the invited account will receive (default `[playback]`).
    #[serde(default)]
    pub permissions: Option<Vec<Permission>>,
    #[serde(rename = "expiresInDays", default)]
    pub expires_in_days: Option<i64>,
}

/// `POST /api/invites` (Bearer + `users.manage`) → mint a registration invite.
pub async fn create_invite(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateInviteBody>,
) -> Result<Response, Response> {
    require(&user, Permission::UsersManage)?;
    let token = auth::random_token();
    let permissions = body.permissions.unwrap_or_else(|| vec![Permission::Playback]);
    let days = body.expires_in_days.unwrap_or(INVITE_TTL_DAYS_DEFAULT).clamp(1, 365);
    let expires_at = time::OffsetDateTime::now_utc().unix_timestamp() + days * 24 * 3600;

    let token_db = token.clone();
    let perms = permissions.clone();
    let uid = user.id.clone();
    query(&state.db, move |pool| db::create_invite(&pool, &token_db, &perms, &uid, expires_at)).await?;
    let url = state
        .config
        .web_url
        .as_ref()
        .map(|w| format!("{w}/join?invite={token}"));
    Ok(Json(super::dto::InviteCreated { token, url, permissions, expires_at }).into_response())
}

/// `GET /api/invites` (Bearer + `users.manage`) → pending invites.
pub async fn list_invites(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user, Permission::UsersManage)?;
    let invites = query(&state.db, move |pool| db::list_invites(&pool)).await?;
    Ok(Json(invites).into_response())
}

/// `GET /api/invites/:token` (public) → `{ valid, expiresAt? }`, so the join page
/// can validate before showing the form (the invitee isn't a user yet).
pub async fn check_invite(State(state): State<SharedState>, Path(token): Path<String>) -> Response {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    match query(&state.db, move |pool| db::get_invite(&pool, &token)).await {
        Ok(Some(inv)) => {
            Json(json!({ "valid": !inv.used && inv.expires_at > now, "expiresAt": inv.expires_at }))
                .into_response()
        }
        Ok(None) => Json(json!({ "valid": false })).into_response(),
        Err(resp) => resp,
    }
}

/// `DELETE /api/invites/:token` (Bearer + `users.manage`) → revoke an invite.
pub async fn delete_invite(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(token): Path<String>,
) -> Result<Response, Response> {
    require(&user, Permission::UsersManage)?;
    query(&state.db, move |pool| db::delete_invite(&pool, &token)).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
