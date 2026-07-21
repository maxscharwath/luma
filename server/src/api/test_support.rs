//! Reusable integration-test harness for the KROMA HTTP API.
//!
//! [`test_app`] builds the *real* application router ([`crate::api::router`])
//! over a fresh temp-file SQLite DB, seeds the built-in demo catalogue + the
//! search index, and mints an all-permissions owner session so authenticated
//! endpoints are reachable. Requests are driven in-process with
//! [`tower::ServiceExt::oneshot`], so no socket is bound and each test is fully
//! isolated (unique DB per test, like the kroma-db `#[cfg(test)]` pattern).
//!
//! Nothing here talks to the network, a module sidecar, or `ffmpeg`; the
//! supervisor is built with an empty modules dir and `module_services` is empty.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use crate::config::Config;
use crate::db;
use crate::model::Permission;
use crate::services::settings::{self, LibraryDef, Settings};
use crate::state::{AppState, SharedState};

/// Monotonic counter making per-test temp paths + tokens unique (paired with the
/// pid), mirroring the kroma-db test harness.
static SEQ: AtomicU32 = AtomicU32::new(0);

/// Far-future expiry (unix seconds) so seeded sessions / access tokens are live.
const FUTURE: i64 = 9_999_999_999;

/// A fully wired app under test plus the handles a test needs to drive + assert.
pub struct TestApp {
    /// The real `/api` router (state applied), cloned per request by the helpers.
    pub app: Router,
    /// Shared app state, for direct seeding (users, libraries, settings).
    pub state: SharedState,
    /// Bearer token of the seeded all-permissions owner.
    pub token: String,
    /// Id of the seeded owner account.
    pub user_id: String,
}

fn unique_data_dir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("kroma-apitest-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp data dir");
    dir
}

/// A minimal config: a temp `data_dir`, no media dirs (so nothing is scanned),
/// no TMDB key (network features cleanly return 503), no `web_dir` (no SPA
/// fallback intercepting `/api` misses).
fn test_config(data_dir: PathBuf) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 0,
        media_dirs: Vec::new(),
        movies_dirs: Vec::new(),
        series_dirs: Vec::new(),
        data_dir,
        tmdb_api_key: None,
        tmdb_language: "en-US".into(),
        tmdb_enrich: false,
        web_url: None,
        web_dir: None,
        https_override: None,
        https_port_override: None,
        tls_extra_sans: Vec::new(),
    }
}

/// A supervisor with an empty modules dir: no sidecar is ever spawned, but the
/// host + proxy routes still mount exactly as in production.
fn test_supervisor(data_dir: &Path) -> Arc<kroma_module_supervisor::Supervisor> {
    kroma_module_supervisor::Supervisor::new(kroma_module_supervisor::SupervisorConfig {
        modules_dir: data_dir.join("modules"),
        core_url: "http://127.0.0.1:0".into(),
        host_token: "test-host-token".into(),
        db_path: data_dir.join("kroma.db"),
        data_dir: data_dir.to_path_buf(),
        reserved_ids: Vec::new(),
        server_version: "0.0.0-test".into(),
        log_line: None,
    })
}

/// Build the app: fresh temp DB (+ module-owned tables), demo catalogue, search
/// index, and an all-permissions owner session.
pub fn test_app() -> TestApp {
    build_app(None)
}

/// Like [`test_app`] but with a (fake) TMDB key configured, so the metadata /
/// discover handlers clear their `require_tmdb_key` gate and reach the DB-only
/// branches (item/show lookup + the pre-network 404). Tests built on this MUST
/// only request *unknown* ids so the handler 404s before any network fetch.
pub fn test_app_with_tmdb() -> TestApp {
    build_app(Some("test-tmdb-key"))
}

fn build_app(tmdb_api_key: Option<&str>) -> TestApp {
    let data_dir = unique_data_dir();
    let db = db::init(&data_dir.join("kroma.db")).expect("init db");

    // Mirror `main::apply_module_schema` so any module-owned tables a read path
    // touches (e.g. the acquisition tables the home/discover flows reference)
    // exist. No-op when no module contributes a migration.
    {
        let conn = db.get().expect("db conn");
        for migration in kroma_module_kernel::module_migrations() {
            db::apply_migrations(&conn, migration).expect("apply module schema");
        }
    }

    let mut config = test_config(data_dir.clone());
    config.tmdb_api_key = tmdb_api_key.map(str::to_string);
    let settings = Settings::load(&db);
    let embedder: Arc<dyn kroma_engine::ports::Embedder> = Arc::new(kroma_engine::ports::NoopEmbedder);
    let state = AppState::new(config, false, db.clone(), settings, embedder, HashMap::new(), &[]);

    // Seed the built-in demo catalogue (2 libraries, 2 shows, 10 items) and build
    // the in-RAM search index from it so browse + search endpoints return data.
    let data = crate::services::demo::demo_data();
    db::sync_all(&db, &data.libraries, &data.shows, &data.items, &data.mtimes).expect("seed demo");
    state.search.reindex_from_db(&db).expect("reindex search");

    let supervisor = test_supervisor(&data_dir);
    let app = crate::api::router(state.clone(), supervisor);

    let (user_id, token) = seed_session(&state, "owner@test.dev", "owner", &Permission::all());
    TestApp { app, state, token, user_id }
}

/// Create a user with `perms`, an access token, and a live session bound to it
/// (so `GET /auth/me/sessions` lists a device). Returns `(user_id, bearer)`.
pub fn seed_session(
    state: &SharedState,
    email: &str,
    username: &str,
    perms: &[Permission],
) -> (String, String) {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let user = db::create_user(&state.db, email, username, "test-hash", perms).expect("create user");
    let access = format!("access-{}-{n}", std::process::id());
    db::create_access_token(&state.db, &access, &user.id, FUTURE, true, Some("integration-test"))
        .expect("create access token");
    let token = format!("session-{}-{n}", std::process::id());
    db::create_session(&state.db, &token, &user.id, FUTURE, Some(&access)).expect("create session");
    (user.id, token)
}

/// Like [`seed_session`] but stores a real PBKDF2 hash of `password`, so
/// password-verifying endpoints (change-password) can be driven end to end (the
/// plain `seed_session` stores a sentinel `"test-hash"` that `verify_password`
/// can never match). Returns `(user_id, bearer)`.
pub fn seed_session_pw(
    state: &SharedState,
    email: &str,
    username: &str,
    password: &str,
    perms: &[Permission],
) -> (String, String) {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let hash = crate::services::auth::hash_password(password);
    let user = db::create_user(&state.db, email, username, &hash, perms).expect("create user");
    let access = format!("access-{}-{n}", std::process::id());
    db::create_access_token(&state.db, &access, &user.id, FUTURE, true, Some("integration-test"))
        .expect("create access token");
    let token = format!("session-{}-{n}", std::process::id());
    db::create_session(&state.db, &token, &user.id, FUTURE, Some(&access)).expect("create session");
    (user.id, token)
}

/// Mint a bare device access token for an existing `user_id` with the given
/// `pin_verified` flag (no session). Lets a test drive `POST /auth/token`
/// (exchange) through the PIN gate, which the all-verified [`seed_session`]
/// token can't reach. Returns the raw access token.
pub fn seed_access_token(state: &SharedState, user_id: &str, pin_verified: bool) -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let access = format!("raw-access-{}-{n}", std::process::id());
    db::create_access_token(&state.db, &access, user_id, FUTURE, pin_verified, Some("integration-test"))
        .expect("create access token");
    access
}

/// Add a library definition of a specific `kind` to the settings store (no
/// rescan). Returns the new library id. Lets tests exercise the admin library
/// card's kind-label mapping without kicking `library.scan`.
pub fn seed_library_kind(state: &SharedState, name: &str, kind: &str) -> String {
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let id = format!("lib-{}", SEQ.fetch_add(1, Ordering::Relaxed));
    defs.push(LibraryDef {
        id: id.clone(),
        name: name.into(),
        kind: kind.into(),
        folders: Vec::new(),
        auto_scan: true,
    });
    settings::set_library_defs(&state.settings, &state.db, &defs);
    id
}

/// Add a library definition to the settings store (as the admin create handler
/// does, minus the background rescan). Returns the new library id.
pub fn seed_library(state: &SharedState, name: &str) -> String {
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let id = format!("lib-{}", SEQ.fetch_add(1, Ordering::Relaxed));
    defs.push(LibraryDef {
        id: id.clone(),
        name: name.into(),
        kind: "movies".into(),
        folders: Vec::new(),
        auto_scan: true,
    });
    settings::set_library_defs(&state.settings, &state.db, &defs);
    id
}

/// A demo item id by title, computed from the deterministic demo generator, so
/// tests can hit `/items/:id` without first listing.
pub fn demo_item_id(title: &str) -> String {
    crate::services::demo::demo_data()
        .items
        .into_iter()
        .find(|i| i.title == title)
        .map(|i| i.id)
        .unwrap_or_else(|| panic!("demo item not found: {title}"))
}

/// A demo show id by title.
pub fn demo_show_id(title: &str) -> String {
    crate::services::demo::demo_data()
        .shows
        .into_iter()
        .find(|s| s.title == title)
        .map(|s| s.id)
        .unwrap_or_else(|| panic!("demo show not found: {title}"))
}

/// Build a request, attaching the bearer (when given) and a `ConnectInfo`
/// extension (handlers like `login`/`ping` extract the socket addr, which
/// `oneshot` does not supply on its own).
fn build_request(method: &str, uri: &str, token: Option<&str>, body: Option<String>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    let body = match body {
        Some(json) => {
            builder = builder.header("content-type", "application/json");
            Body::from(json)
        }
        None => Body::empty(),
    };
    let mut req = builder.body(body).expect("build request");
    req.extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 40000))));
    req
}

/// Drive one request against the router and return `(status, headers, parsed-json)`.
/// Extra request headers are attached verbatim (e.g. `CF-Connecting-IP` to give a
/// login test its own brute-force-guard bucket). A non-JSON or empty body (204,
/// `text/plain` logs) parses to [`Value::Null`].
pub async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    json: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Value) {
    use axum::http::{HeaderName, HeaderValue};
    let body = json.map(|v| v.to_string());
    let mut req = build_request(method, uri, token, body);
    for (k, v) in headers {
        req.headers_mut().insert(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    let resp = app.clone().oneshot(req).await.expect("router response");
    let status = resp.status();
    let out_headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, out_headers, value)
}

/// Drive one request against the router and return `(status, parsed-json)`. A
/// non-JSON or empty body (204, `text/plain` logs) parses to [`Value::Null`].
pub async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    json: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _headers, value) = raw(app, method, uri, token, json, &[]).await;
    (status, value)
}

/// `GET` convenience over [`send`].
pub async fn get(app: &Router, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    send(app, "GET", uri, token, None).await
}
