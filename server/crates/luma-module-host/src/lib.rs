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

// The axum `Response` is intentionally the Err type of request guards so handlers
// short-circuit with `?`; boxing every guard for `result_large_err` would churn
// dozens of signatures for no real gain on these error paths.
#![allow(clippy::result_large_err)]

use std::any::{Any, TypeId};
use std::path::Path;
use std::sync::Arc;

pub use axum::async_trait;
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

/// Run a blocking DB closure off the async runtime, mapping any failure to a
/// uniform 500. The shared combinator admin handlers (app + module crates) use.
pub async fn blocking<T, F>(f: F) -> Result<T, Response>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            tracing::error!(error = %e, "database error");
            Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
        }
        Err(e) => {
            tracing::error!(error = %e, "task join error");
            Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
        }
    }
}

/// Register a peer PORT (a trait object) for the service registry: returns the
/// `(TypeId, value)` to insert. The registry stores concrete `Any` values, so the
/// port `Arc<dyn P>` is wrapped in an outer `Arc` keyed by `Arc<dyn P>`'s TypeId.
/// The port traits themselves live in `the SDK ports module (luma_module_sdk::ports)`; this is only the generic
/// plumbing, so the seam names no port trait (and no module).
pub fn port_service<P: ?Sized + Any + Send + Sync>(
    port: Arc<P>,
) -> (TypeId, Arc<dyn Any + Send + Sync>) {
    (TypeId::of::<Arc<P>>(), Arc::new(port))
}

/// Resolve a peer PORT registered via [`port_service`]. `None` when no provider
/// registered it (e.g. the providing module is absent / disabled).
pub fn resolve_port<P: ?Sized + Any + Send + Sync>(host: &dyn HostCtx) -> Option<Arc<P>> {
    let any = host.get_service(TypeId::of::<Arc<P>>())?;
    any.downcast::<Arc<P>>().ok().map(|boxed| (*boxed).clone())
}

/// Clone the pool and run a blocking DB closure off the async runtime; a thin
/// combinator over [`blocking`] that hands the closure its own [`Pool`].
pub async fn query<T, F>(pool: &Pool, f: F) -> Result<T, Response>
where
    F: FnOnce(Pool) -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    blocking(move || f(pool)).await
}

/// A real-time event a module publishes onto the host's bus (fanned out to
/// WebSocket clients as `{ "type": <topic>, ...payload }`). The module owns its
/// topic string and payload shape; the seam names no module event type, so the
/// core stays generic. The host merges `topic` under the wire `type` key.
pub struct Event {
    /// The wire event type, e.g. `"download.progress"`.
    pub topic: String,
    /// The event fields as a JSON object (merged next to `type` on the wire).
    pub payload: serde_json::Value,
}

impl Event {
    pub fn new(topic: impl Into<String>, payload: serde_json::Value) -> Self {
        Self { topic: topic.into(), payload }
    }
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

    /// A localized JSON error for `user`'s account locale (the app resolves the
    /// message `key` against the shared catalogs).
    fn lerr(&self, user: &User, status: StatusCode, key: &str) -> Response;

    /// A persisted string setting (or `default` when unset).
    fn setting_str(&self, key: &str, default: &str) -> String;
    /// A persisted boolean setting (or `default` when unset).
    fn setting_bool(&self, key: &str, default: bool) -> bool;
    /// A persisted integer setting (or `default` when unset).
    fn setting_i64(&self, key: &str, default: i64) -> i64;
    /// Persist a batch of settings atomically (one write).
    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>);

    /// Publish a module event onto the app's real-time bus (fanned out to
    /// WebSocket clients). The event's topic + payload are generic; the host
    /// forwards them without knowing the module's event types.
    fn publish(&self, event: Event);
    /// Trigger a background job by its key (e.g. `"acquisition.import"`), running
    /// against the app state. No-op if the key is unknown or already running.
    fn trigger_job(&self, key: &'static str, reason: &'static str);

    /// Whether the module with id `id` is currently enabled (a relocated module's
    /// resident loop idles when its own module is toggled off).
    fn module_enabled(&self, id: &str) -> bool;

    /// Resolve a host-registered service by its type, so a relocated module can
    /// reach its own engine / bridge without the seam ever naming a module type
    /// (dependency injection). Prefer the typed [`service`] helper. `None` when no
    /// service of that type is registered.
    fn get_service(&self, type_id: TypeId) -> Option<Arc<dyn Any + Send + Sync>>;
}

/// Resolve a typed host service (dependency injection). The host registers its
/// concrete services (the download manager, the VPN bridge, ...) under their
/// `TypeId`; a module crate looks its own up by type. `None` when unregistered.
/// Takes `&dyn HostCtx` so a route (with `&S`) and a lifecycle hook (with
/// `Arc<dyn HostCtx>`) both call it uniformly.
pub fn service<T: Any + Send + Sync>(host: &dyn HostCtx) -> Option<Arc<T>> {
    host.get_service(TypeId::of::<T>())?.downcast::<T>().ok()
}

/// The backend contract a module crate implements to own its full server-side
/// vertical: the admin routes it serves (behind the host's enabled-gate) and its
/// enable/disable lifecycle. Generic over the host state `S` so the crate depends
/// only on this seam, never on the app; the binary instantiates it at
/// `S = SharedState`. Anything the module needs from the app (its engine, bridge,
/// DB, settings) comes through `host`, so the binary wires nothing per module.
#[async_trait]
pub trait ServerModule<S>: Send + Sync
where
    S: HostCtx + Clone + Send + Sync + 'static,
{
    /// The module id (matches its `module.json` and frontend package).
    fn id(&self) -> &'static str;

    /// SQL run once at DB init (after the core schema) so a module owns its own
    /// tables. `IF NOT EXISTS` DDL only; runs on every boot. Default: no schema.
    fn migrations(&self) -> &'static str {
        ""
    }

    /// Routes served under `/api/admin`, or `None` for a lifecycle-only module
    /// (e.g. a download engine). Mounted behind the module's enabled-gate by the
    /// host, so they 404 while it is disabled.
    fn admin_routes(&self, _host: &S) -> Option<axum::Router<S>> {
        None
    }

    /// Bring the module's live services up: called when it is enabled at runtime
    /// AND at boot for an already-enabled module. Awaited (not detached), so a slow
    /// start completes before a following disable can race it. Takes an owned
    /// `Arc<dyn HostCtx>` so the module can hand a long-lived handle to a spawned
    /// watchdog / supervisor. Default: nothing.
    async fn on_enable(&self, _host: Arc<dyn HostCtx>) {}

    /// Tear the module's live services down: called when it is disabled at runtime
    /// AND at boot for a disabled module, so nothing is left running. Awaited.
    /// Default: nothing.
    async fn on_disable(&self, _host: Arc<dyn HostCtx>) {}
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
    fn lerr(&self, user: &User, status: StatusCode, key: &str) -> Response {
        (**self).lerr(user, status, key)
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
    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>) {
        (**self).set_settings(patch)
    }
    fn publish(&self, event: Event) {
        (**self).publish(event)
    }
    fn trigger_job(&self, key: &'static str, reason: &'static str) {
        (**self).trigger_job(key, reason)
    }
    fn module_enabled(&self, id: &str) -> bool {
        (**self).module_enabled(id)
    }
    fn get_service(&self, type_id: TypeId) -> Option<Arc<dyn Any + Send + Sync>> {
        (**self).get_service(type_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_port_round_trips_through_the_service_registry() {
        // The double-Arc TypeId trick port_service/resolve_port rely on: a port
        // registered as Arc<dyn P> must come back as the same trait object (a
        // silent mismatch would resolve to None and break cross-module calls).
        trait Greeter: Send + Sync {
            fn hi(&self) -> &'static str;
        }
        struct G;
        impl Greeter for G {
            fn hi(&self) -> &'static str {
                "hi"
            }
        }
        let port: Arc<dyn Greeter> = Arc::new(G);
        let (tid, stored) = port_service(port);
        assert_eq!(tid, TypeId::of::<Arc<dyn Greeter>>());
        let back = stored.downcast::<Arc<dyn Greeter>>().expect("stored value downcasts back");
        assert_eq!((*back).hi(), "hi");
    }
}
