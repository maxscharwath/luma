//! The `/_host/register-job` callback a sidecar module POSTs to so its scheduled
//! jobs join the CORE JobManager, showing up in admin Tâches with cron
//! scheduling, a run-now button and run history exactly like an in-core job.
//!
//! The engine stores a remote job's run logic as an injected closure (it must not
//! depend on the module supervisor); this file, which has both the supervisor and
//! the concrete `SharedState`, builds that closure. On each trigger it resolves
//! the module's current local port and does a blocking HTTP POST to the sidecar's
//! `/_job/run/{key}` endpoint, so the pass runs in the module's own process.
//!
//! Mounted on the `/api` router next to `host_router`, guarded by the same shared
//! host token (a sidecar authenticates every core callback with it).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use luma_module_supervisor::Supervisor;

use crate::model::Category;
use crate::services::jobs::{JobContext, RemoteRun};
use crate::state::SharedState;

/// The `/_host/register-job` route, guarded by the shared host token the same way
/// `host_router` guards its callbacks. Merge into the `/api` router (before the
/// `Extension(supervisor)` layer, which this handler reads).
pub fn routes(host_token: String) -> Router<SharedState> {
    Router::new()
        .route("/_host/register-job", post(register_job))
        .route_layer(from_fn_with_state(HostToken(host_token), require_host_token))
}

#[derive(Clone)]
struct HostToken(String);

/// Reject a request whose bearer does not match the shared host token.
async fn require_host_token(
    State(token): State<HostToken>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    let ok = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == token.0);
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "bad host token").into_response()
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterJobBody {
    module_id: String,
    key: String,
    category: String,
    schedule: Option<String>,
}

/// Register (or, on a module respawn, re-register) one sidecar job on the core
/// JobManager. Idempotent: `register_remote` refreshes the run closure but keeps
/// any existing (possibly admin-customized) schedule state.
async fn register_job(
    State(state): State<SharedState>,
    Extension(supervisor): Extension<Arc<Supervisor>>,
    Json(body): Json<RegisterJobBody>,
) -> Response {
    let key = leak_key(&body.key);
    let category = body.category.parse::<Category>().unwrap_or_else(|()| {
        tracing::warn!(
            category = %body.category,
            key = %body.key,
            "unknown job category from module; defaulting to acquisition"
        );
        Category::Acquisition
    });
    let run = remote_run(supervisor.clone(), body.module_id.clone(), body.key.clone());
    state.jobs.register_remote(key, category, body.schedule, run);
    // register_remote seeds only the module's default schedule; overlay any
    // persisted admin override now that the key exists in the schedules map (the
    // startup load_schedules ran before this sidecar was up, so it skipped it).
    state.jobs.load_schedules(&state.db);
    tracing::info!(module = %body.module_id, key = %body.key, "registered remote job");
    StatusCode::NO_CONTENT.into_response()
}

/// Build a remote job's run closure: on each trigger (from the core scheduler or a
/// manual run-now), resolve the module's CURRENT local port and blocking-POST to
/// its `/_job/run/{key}` with the shared host token. The closure runs on the
/// JobManager's blocking thread, so a long import is fine; the timeout is generous
/// for that. A non-2xx (or an unreachable/stopped sidecar) fails the run, which
/// the console records with the error message.
fn remote_run(supervisor: Arc<Supervisor>, module_id: String, key: String) -> RemoteRun {
    let host_token = supervisor.host_token().to_string();
    Arc::new(move |_ctx: &JobContext| -> anyhow::Result<()> {
        // Module not running (disabled, or mid-respawn): a scheduled fire is a
        // no-op, not a failure. Returning Ok keeps the job history clean instead
        // of recording an error every tick while the module is down.
        let Some(port) = supervisor.port_of(&module_id) else {
            tracing::debug!(module = %module_id, "remote job skipped: module not running");
            return Ok(());
        };
        let url = format!("http://127.0.0.1:{port}/_job/run/{key}");
        let resp = luma_http::Fetch::new()
            .header("authorization", format!("Bearer {host_token}"))
            // Imports move whole files across disks; allow up to 30 minutes.
            .max_time(30 * 60)
            .post_json(&url, &serde_json::Value::Null)?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            anyhow::bail!("sidecar returned HTTP {}: {}", resp.status, resp.text());
        }
    })
}

/// Leak a job key to `&'static str` (what `register_remote` needs to key the
/// engine's `'static` maps), caching by key so a module respawn reuses the same
/// leak. Job keys are a small fixed set, so the total leak is bounded.
fn leak_key(key: &str) -> &'static str {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut map = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    if let Some(&leaked) = map.get(key) {
        return leaked;
    }
    let leaked: &'static str = Box::leak(key.to_string().into_boxed_str());
    map.insert(key.to_string(), leaked);
    leaked
}
