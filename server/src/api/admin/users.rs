//! Member management: the full account list plus permission / username edits and
//! account removal (the "Membres & partage" table).

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::error::lerr;
use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::db;
use crate::infra::events::ServerEvent;
use crate::model::Permission;
use crate::state::SharedState;
use axum::routing::{get, patch};
use axum::Router;

/// Admin user management. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}", patch(update_user).delete(delete_user))
}

/// `GET /api/admin/users` → full member list (the "Membres & partage" table).
pub async fn list_users(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::UsersManage)?;
    let (mut users, library_count) =
        query(&state.db, move |pool| Ok((db::admin_users(&pool)?, db::counts(&pool)?.0))).await?;
    for u in &mut users {
        u.online = state.playback.user_online(&u.id);
    }
    Ok(Json(crate::api::dto::AdminUsers { users, library_count }).into_response())
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserBody {
    #[serde(default)]
    pub permissions: Option<Vec<Permission>>,
    #[serde(default)]
    pub username: Option<String>,
}

/// `PATCH /api/admin/users/:id` → update permissions and/or username.
pub async fn update_user(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::UsersManage)?;
    let id2 = id.clone();
    let all = query(&state.db, move |pool| db::admin_users(&pool)).await?;
    let Some(target) = all.iter().find(|u| u.id == id2) else {
        return Err(lerr(super::user_locale(&user), StatusCode::NOT_FOUND, "error.userNotFound"));
    };

    if let Some(perms) = body.permissions.clone() {
        // Don't strip the last owner of its management rights.
        let owners = all
            .iter()
            .filter(|u| u.permissions.contains(&Permission::UsersManage))
            .count();
        let target_is_owner = target.permissions.contains(&Permission::UsersManage);
        let removes_owner = !perms.contains(&Permission::UsersManage);
        if target_is_owner && removes_owner && owners <= 1 {
            return Err(lerr(
                super::user_locale(&user),
                StatusCode::BAD_REQUEST,
                "admin.cantRemoveLastOwner",
            ));
        }
        let id3 = id.clone();
        query(&state.db, move |pool| db::update_user_permissions(&pool, &id3, &perms)).await?;
    }
    if let Some(name) = body.username.clone().filter(|n| !n.trim().is_empty()) {
        let id3 = id.clone();
        let name = name.trim().to_string();
        query(&state.db, move |pool| db::set_user_username(&pool, &id3, &name)).await?;
    }
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `DELETE /api/admin/users/:id` → remove an account.
pub async fn delete_user(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    super::require(&user, Permission::UsersManage)?;
    if id == user.id {
        return Err(lerr(super::user_locale(&user), StatusCode::BAD_REQUEST, "admin.cantDeleteSelf"));
    }
    let id2 = id.clone();
    let all = query(&state.db, move |pool| db::admin_users(&pool)).await?;
    let Some(target) = all.iter().find(|u| u.id == id2) else {
        return Err(lerr(super::user_locale(&user), StatusCode::NOT_FOUND, "error.userNotFound"));
    };
    let owners = all
        .iter()
        .filter(|u| u.permissions.contains(&Permission::UsersManage))
        .count();
    if target.permissions.contains(&Permission::UsersManage) && owners <= 1 {
        return Err(lerr(super::user_locale(&user), StatusCode::BAD_REQUEST, "admin.cantDeleteLastOwner"));
    }
    query(&state.db, move |pool| db::delete_user(&pool, &id)).await?;
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(StatusCode::NO_CONTENT.into_response())
}
