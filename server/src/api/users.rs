//! Accounts, sessions, profile avatar, and playback-progress handlers.
//!
//! Auth is by opaque bearer token (see [`crate::auth`]). The catalogue/stream
//! endpoints stay open (LAN trust model); only these per-user routes require a
//! valid session via the [`AuthUser`] extractor.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::error::lerr;
use crate::api::handlers::{blocking, query};
use crate::auth::{self, AuthUser};
use crate::db;
use crate::i18n::{self, ReqLocale};
use crate::model::{Permission, User};
use crate::quickconnect::PollState;
use crate::state::SharedState;

/// Max avatar upload size (raw image bytes).
pub const MAX_AVATAR_BYTES: usize = 8 * 1024 * 1024;

// ----- auth -------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterBody {
    pub email: String,
    pub username: String,
    pub password: String,
    /// Invitation token. Required for every account after the bootstrap owner —
    /// registration is invite-only (an admin with `users.manage` mints invites).
    #[serde(rename = "inviteToken", default)]
    pub invite_token: Option<String>,
}

/// `POST /api/auth/register` → `{ token, user }` (also opens a session).
///
/// The **first** account ever created is the owner (gets every permission, no
/// invite needed). After that, registration requires a valid `inviteToken`; the
/// new account inherits the invite's permissions.
pub async fn register(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    Json(body): Json<RegisterBody>,
) -> Response {
    let email = body.email.trim().to_lowercase();
    let username = body.username.trim().to_string();
    if email.is_empty() || !email.contains('@') || username.is_empty() || body.password.len() < 4 {
        return lerr(loc, StatusCode::BAD_REQUEST, "auth.registerInvalid");
    }

    // How many accounts exist already decides whether this is the bootstrap
    // owner (no invite needed) or an invite-gated signup.
    let count = match query(&state.db, move |pool| db::user_count(&pool)).await {
        Ok(n) => n,
        Err(resp) => return resp,
    };

    // Reject a duplicate email *before* consuming any invite. Otherwise a typo or
    // a retry against an already-registered email spends the single-use invite
    // and locks the invitee out. (The UNIQUE constraint in `create_user` remains
    // the atomic backstop for the residual check-then-create window.)
    let email_check = email.clone();
    match query(&state.db, move |pool| db::find_user_by_email(&pool, &email_check)).await {
        Ok(Some(_)) => return lerr(loc, StatusCode::CONFLICT, "auth.emailTaken"),
        Ok(None) => {}
        Err(resp) => return resp,
    }

    // Decide the granted permissions: bootstrap owner → all; otherwise consume
    // the invite (registration is closed without one). Done after the email check
    // so the invite is only burned once we know the account can be created.
    let permissions = if count == 0 {
        Permission::all()
    } else {
        let Some(token) = body.invite_token.clone().filter(|t| !t.trim().is_empty()) else {
            return lerr(loc, StatusCode::FORBIDDEN, "auth.inviteOnly");
        };
        match query(&state.db, move |pool| db::consume_invite(&pool, token.trim())).await {
            Ok(Some(perms)) => perms,
            Ok(None) => return lerr(loc, StatusCode::FORBIDDEN, "auth.inviteInvalid"),
            Err(resp) => return resp,
        }
    };

    let hash = auth::hash_password(&body.password);
    let user =
        match query(&state.db, move |pool| db::create_user(&pool, &email, &username, &hash, &permissions)).await
        {
            Ok(u) => u,
            Err(resp) => return resp,
        };
    issue_session(state, user).await
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    /// Email or username — the profile picker only knows usernames.
    pub email: String,
    pub password: String,
}

/// `POST /api/auth/login` → `{ token, user }`. Accepts email or username.
pub async fn login(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    Json(body): Json<LoginBody>,
) -> Response {
    let identifier = body.email.trim().to_string();
    let found = match query(&state.db, move |pool| db::find_user_by_login(&pool, &identifier)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // Same response whether the email is unknown or the password is wrong.
    let Some((user, hash)) = found else {
        return lerr(loc, StatusCode::UNAUTHORIZED, "auth.invalidCredentials");
    };
    if !auth::verify_password(&body.password, &hash) {
        return lerr(loc, StatusCode::UNAUTHORIZED, "auth.invalidCredentials");
    }
    issue_session(state, user).await
}

/// `POST /api/auth/logout` → 204. No-op if the token is missing/unknown.
pub async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if let Some(token) = auth::bearer_from_headers(&headers) {
        let _ = query(&state.db, move |pool| db::delete_session(&pool, &token)).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

/// `GET /api/auth/me` (Bearer) → `{ user }`.
pub async fn me(AuthUser(user): AuthUser) -> Response {
    Json(json!({ "user": user })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeBody {
    /// Preferred UI locale, e.g. `"fr"` | `"en"`. `null` clears it (fall back to
    /// the device locale). Unknown tags are ignored (left unchanged).
    #[serde(default)]
    pub language: Option<String>,
}

/// `PATCH /api/auth/me` (Bearer) `{ language }` → `{ user }`. Persists the
/// account's preferred UI locale so it follows the profile across devices.
pub async fn update_me(
    State(state): State<SharedState>,
    AuthUser(mut user): AuthUser,
    Json(body): Json<UpdateMeBody>,
) -> Response {
    // Normalise to a supported code; an unknown/garbage tag leaves it unchanged.
    let language = body.language.as_deref().and_then(i18n::normalize);
    let uid = user.id.clone();
    if let Err(resp) = query(&state.db, move |pool| db::set_user_language(&pool, &uid, language)).await {
        return resp;
    }
    user.language = language.map(str::to_string);
    Json(json!({ "user": user })).into_response()
}

/// Mint a session token for `user` and return `{ token, user }`.
async fn issue_session(state: SharedState, user: User) -> Response {
    let token = auth::random_token();
    let expires_at = time::OffsetDateTime::now_utc().unix_timestamp() + auth::SESSION_TTL_SECS;
    let token_db = token.clone();
    let uid = user.id.clone();
    if let Err(resp) = query(&state.db, move |pool| db::create_session(&pool, &token_db, &uid, expires_at)).await
    {
        return resp;
    }
    // Best-effort last-seen stamp for the admin members table.
    let uid = user.id.clone();
    let _ = query(&state.db, move |pool| {
        let _ = db::touch_last_seen(&pool, &uid);
        Ok(())
    })
    .await;
    Json(super::dto::AuthResult { token, user }).into_response()
}

// ----- profiles ---------------------------------------------------------------

/// `GET /api/users` → `PublicUser[]` for the "Qui regarde ?" picker (no emails).
pub async fn list_users(State(state): State<SharedState>) -> Response {
    match query(&state.db, move |pool| db::list_users(&pool)).await {
        Ok(users) => Json(users).into_response(),
        Err(resp) => resp,
    }
}

/// `POST /api/users/avatar` (Bearer, body = raw `image/*`) → `{ avatarUrl }`.
/// The image is transcoded to WebP and stored in the shared image cache.
pub async fn upload_avatar(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    AuthUser(user): AuthUser,
    body: Bytes,
) -> Response {
    if body.is_empty() {
        return lerr(loc, StatusCode::BAD_REQUEST, "error.emptyBody");
    }
    if body.len() > MAX_AVATAR_BYTES {
        return lerr(loc, StatusCode::PAYLOAD_TOO_LARGE, "error.imageTooLarge");
    }

    let data_dir = state.config.data_dir.clone();
    let bytes = body.to_vec();
    let url = match blocking(move || Ok(crate::image::store_upload(&data_dir, &bytes))).await {
        Ok(Some(u)) => u,
        Ok(None) => return lerr(loc, StatusCode::UNSUPPORTED_MEDIA_TYPE, "error.imageUnreadable"),
        Err(resp) => return resp,
    };

    let uid = user.id.clone();
    let url_db = url.clone();
    if let Err(resp) = query(&state.db, move |pool| db::set_user_avatar(&pool, &uid, Some(&url_db))).await {
        return resp;
    }
    Json(json!({ "avatarUrl": url })).into_response()
}

// ----- progress ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgressBody {
    #[serde(rename = "positionMs")]
    pub position_ms: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
}

/// `PUT /api/progress/:id` (Bearer) `{ positionMs, durationMs }` → 204.
pub async fn save_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
    Json(body): Json<ProgressBody>,
) -> Response {
    let pos = body.position_ms.max(0);
    match query(&state.db, move |pool| db::upsert_progress(&pool, &user.id, &item_id, pos, body.duration_ms))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

/// `DELETE /api/progress/:id` (Bearer) → 204 (finished / removed from Continue).
pub async fn delete_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
) -> Response {
    match query(&state.db, move |pool| db::delete_progress(&pool, &user.id, &item_id)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/progress/:id` (Bearer) → `ProgressEntry | null` for one item, so the
/// player can resume without fetching the whole list.
pub async fn get_progress(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Path(item_id): Path<String>,
) -> Response {
    match query(&state.db, move |pool| db::get_progress(&pool, &user.id, &item_id)).await {
        Ok(entry) => Json(entry).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/progress` (Bearer) → `ProgressEntry[]` (all saved positions).
pub async fn list_progress(State(state): State<SharedState>, AuthUser(user): AuthUser) -> Response {
    match query(&state.db, move |pool| db::list_progress(&pool, &user.id)).await {
        Ok(p) => Json(p).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/continue` (Bearer) → `ContinueItem[]` (resumable, newest first).
pub async fn continue_watching(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Response {
    match query(&state.db, move |pool| db::continue_watching(&pool, &user.id)).await {
        Ok(items) => Json(items).into_response(),
        Err(resp) => resp,
    }
}

// ----- quick connect ----------------------------------------------------------

/// `POST /api/auth/quickconnect/initiate` → `{ code, secret, expiresInSec,
/// authorizeUrl? }`. The device shows `code` (and a QR of `authorizeUrl` when
/// the server knows the web app URL) and then polls with `secret`.
pub async fn quick_initiate(State(state): State<SharedState>) -> Response {
    let init = state.quickconnect.initiate();
    let authorize_url = state
        .config
        .web_url
        .as_ref()
        .map(|w| format!("{w}/connect?code={}", init.code));
    Json(super::dto::QuickConnectInit {
        code: init.code,
        secret: init.secret,
        expires_in_sec: init.expires_in,
        authorize_url,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct QuickAuthorizeBody {
    pub code: String,
}

/// `POST /api/auth/quickconnect/authorize` (Bearer) `{ code }` → 204. Approves a
/// device's code for the signed-in user, minting the session the device will
/// receive on its next poll.
pub async fn quick_authorize(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    AuthUser(user): AuthUser,
    Json(body): Json<QuickAuthorizeBody>,
) -> Response {
    let code = body.code.trim().to_string();
    let token = auth::random_token();
    let expires_at = time::OffsetDateTime::now_utc().unix_timestamp() + auth::SESSION_TTL_SECS;
    let token_db = token.clone();
    let uid = user.id.clone();
    if let Err(resp) = query(&state.db, move |pool| db::create_session(&pool, &token_db, &uid, expires_at)).await
    {
        return resp;
    }

    if state.quickconnect.authorize(&code, user, token.clone()) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        // Unknown/expired code → don't leave the just-created session dangling.
        let _ = query(&state.db, move |pool| db::delete_session(&pool, &token)).await;
        lerr(loc, StatusCode::NOT_FOUND, "connect.invalidCode")
    }
}

#[derive(Debug, Deserialize)]
pub struct QuickPollQuery {
    pub secret: String,
}

/// `GET /api/auth/quickconnect/poll?secret=…` → `{ status }` where status is
/// `pending` | `authorized` (then `{ token, user }`) | `expired`.
pub async fn quick_poll(State(state): State<SharedState>, Query(q): Query<QuickPollQuery>) -> Response {
    let status = match state.quickconnect.poll(&q.secret) {
        PollState::Authorized { token, user } => super::dto::QuickPoll::Authorized { token, user },
        PollState::Pending => super::dto::QuickPoll::Pending,
        PollState::Unknown => super::dto::QuickPoll::Expired,
    };
    Json(status).into_response()
}

// ----- invitations ------------------------------------------------------------

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
