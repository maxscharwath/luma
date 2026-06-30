//! HTTP request extractors: the [`AuthUser`] bearer-token gate.
//!
//! Resolves an `Authorization: Bearer <token>` header against the `sessions`
//! table so any handler can opt into authentication simply by taking
//! [`AuthUser`] as an argument.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Response;

use crate::api::error::json_error;
use crate::model::User;
use crate::state::SharedState;

/// An authenticated user, resolved from an `Authorization: Bearer <token>`
/// header against the `sessions` table. Handlers that take this as an argument
/// are automatically gated a missing/expired/unknown token yields `401`.
pub struct AuthUser(pub User);

#[async_trait]
impl FromRequestParts<SharedState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_from_headers(&parts.headers)
            .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
        let pool = state.db.clone();
        let user = tokio::task::spawn_blocking(move || crate::db::session_user(&pool, &token))
            .await
            .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "invalid or expired session"))?;
        Ok(AuthUser(user))
    }
}

/// Optionally-authenticated user: `Some(user)` for a valid Bearer token, `None`
/// otherwise. Never rejects for endpoints that are public but personalise when
/// signed in (e.g. catalogue cards with per-user progress).
pub struct OptionalAuthUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<SharedState> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = bearer_from_headers(&parts.headers) else {
            return Ok(OptionalAuthUser(None));
        };
        let pool = state.db.clone();
        let user = tokio::task::spawn_blocking(move || crate::db::session_user(&pool, &token))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten();
        Ok(OptionalAuthUser(user))
    }
}

/// Extract the bearer token from a header map's `Authorization` header, if any.
/// Public so handlers (e.g. logout) can read the token without the [`AuthUser`]
/// extractor.
pub fn bearer_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let h = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = h.to_str().ok()?;
    s.strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
