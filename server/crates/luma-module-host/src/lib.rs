//! The host seam between the running app and a module's backend.
//!
//! A module crate's server half (routes + services) needs a few things from the
//! app: the DB pool, capability gating, settings, the event bus, and so on. If it
//! took `&SharedState` it would depend on `luma-engine` (the whole app) and the
//! two would form a dependency cycle (luma-engine already depends on the module
//! crates). Instead it names ONLY the [`HostCtx`] trait defined here, plus the
//! shared HTTP extractors/helpers. The binary's `AppState` implements `HostCtx`,
//! so `Router<SharedState>` handlers and generic `Router<S: HostCtx>` module
//! handlers both work, and a module crate depends only on this leaf.

use std::path::Path;

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use luma_db::Pool;
use luma_domain::{Permission, User};

/// Build a JSON error response `{ "error": "<message>" }` with the given status.
/// The one definition; `luma-engine` and the binary re-export it so existing
/// call sites are unchanged.
pub fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// The slice of the running app a module's backend can reach. The binary's
/// `AppState` (as `Arc<AppState>` = `SharedState`) implements it; a module crate
/// names only this trait, never the app, so it stays a leaf and breaks the cycle.
///
/// The trait is grown as subsystems are relocated (settings accessors, event
/// publish, job triggers, the VPN proxy URL, ...); it starts with the DB pool and
/// capability gating, which the shared extractors + every admin route need.
pub trait HostCtx: Send + Sync + 'static {
    /// The SQLite connection pool.
    fn db(&self) -> &Pool;

    /// The server data directory (per-module scratch lives under it).
    fn data_dir(&self) -> &Path;

    /// Gate a handler on a capability. Returns a localized `403` response on
    /// failure (the app resolves the caller's locale).
    fn require(&self, user: &User, perm: Permission) -> Result<(), Response>;

    /// Gate on holding ANY management capability (unlocks the console shell).
    fn require_any_admin(&self, user: &User) -> Result<(), Response>;

    /// A persisted string setting (or `default` when unset).
    fn setting_str(&self, key: &str, default: &str) -> String;
    /// A persisted boolean setting (or `default` when unset).
    fn setting_bool(&self, key: &str, default: bool) -> bool;
    /// A persisted integer setting (or `default` when unset).
    fn setting_i64(&self, key: &str, default: i64) -> i64;
    /// Persist a string setting.
    fn set_setting_str(&self, key: &str, value: &str);
    /// Persist a batch of settings atomically (one write).
    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>);
}

/// The router state is `Arc<AppState>` (= `SharedState`), but the orphan rule
/// forbids `impl HostCtx for Arc<AppState>` in the app crate (foreign `Arc`,
/// covered local type). This blanket impl - legal here because the trait is
/// local - lifts any `T: HostCtx` to `Arc<T>`, so `AppState: HostCtx` (in the
/// app) gives `Arc<AppState>: HostCtx` for free, which the extractors + generic
/// `Router<S>` module handlers require.
impl<T: HostCtx + ?Sized> HostCtx for std::sync::Arc<T> {
    fn db(&self) -> &Pool {
        (**self).db()
    }
    fn data_dir(&self) -> &Path {
        (**self).data_dir()
    }
    fn require(&self, user: &User, perm: Permission) -> Result<(), Response> {
        (**self).require(user, perm)
    }
    fn require_any_admin(&self, user: &User) -> Result<(), Response> {
        (**self).require_any_admin(user)
    }
    fn setting_str(&self, key: &str, default: &str) -> String {
        (**self).setting_str(key, default)
    }
    fn setting_bool(&self, key: &str, default: bool) -> bool {
        (**self).setting_bool(key, default)
    }
    fn setting_i64(&self, key: &str, default: i64) -> i64 {
        (**self).setting_i64(key, default)
    }
    fn set_setting_str(&self, key: &str, value: &str) {
        (**self).set_setting_str(key, value)
    }
    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>) {
        (**self).set_settings(patch)
    }
}

/// An authenticated user, resolved from an `Authorization: Bearer <token>`
/// header against the `sessions` table. Generic over any [`HostCtx`], so it works
/// with the app's concrete `SharedState` AND a module crate's generic
/// `Router<S: HostCtx>`. A missing/expired/unknown token yields `401`.
pub struct AuthUser(pub User);

#[async_trait]
impl<S: HostCtx> FromRequestParts<S> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = bearer_from_headers(&parts.headers)
            .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
        let pool = state.db().clone();
        let user = tokio::task::spawn_blocking(move || luma_db::session_user(&pool, &token))
            .await
            .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "invalid or expired session"))?;
        Ok(AuthUser(user))
    }
}

/// Optionally-authenticated user: `Some(user)` for a valid Bearer token, `None`
/// otherwise. Never rejects for endpoints that are public but personalise when
/// signed in.
pub struct OptionalAuthUser(pub Option<User>);

#[async_trait]
impl<S: HostCtx> FromRequestParts<S> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(token) = bearer_from_headers(&parts.headers) else {
            return Ok(OptionalAuthUser(None));
        };
        let pool = state.db().clone();
        let user = tokio::task::spawn_blocking(move || luma_db::session_user(&pool, &token))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten();
        Ok(OptionalAuthUser(user))
    }
}

/// Extract the bearer token from a header map's `Authorization` header, if any.
pub fn bearer_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let h = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = h.to_str().ok()?;
    s.strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
