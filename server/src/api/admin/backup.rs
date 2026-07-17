//! Portable backup: export the server's identity state (accounts, settings,
//! history, resume positions, invites, cron overrides, custom avatars) as a ZIP
//! optionally password-encrypted and import it on another server. Import
//! restores the rows, reloads the settings store, then kicks a re-scan so the
//! catalogue regenerates with the same item IDs (the library defs travel inside
//! `settings`). See [`crate::services::backup`].

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::api::error::{json_error, lerr};
use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::infra::events::ServerEvent;
use crate::model::Permission;
use crate::services::backup::ImportError;
use crate::state::SharedState;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

/// Backup export / import. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/backup/export", get(export_backup))
        .route(
            "/backup/import",
            post(import_backup).layer(DefaultBodyLimit::max(MAX_BACKUP_BYTES)),
        )
}

/// Max accepted import body. The whole `.kroma` is buffered in memory; backups are
/// normally KB–MB, but lift the small axum default (2 MiB) so a large library
/// (many accounts/avatars) isn't rejected with an opaque 413.
pub const MAX_BACKUP_BYTES: usize = 256 * 1024 * 1024;

/// Optional encryption password sent hex-encoded in `X-Backup-Password` so an
/// arbitrary (non-ASCII) password survives an HTTP header. Empty/absent → none.
fn header_password(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("x-backup-password")?.to_str().ok()?;
    let bytes = hex::decode(raw).ok()?;
    let s = String::from_utf8(bytes).ok()?;
    (!s.is_empty()).then_some(s)
}

fn header_flag(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// `GET /api/admin/backup/export` → download a `.kroma` backup (encrypted when a
/// `X-Backup-Password` header is sent, otherwise a compressed archive both
/// share the extension and the import auto-detects). Contains credentials
/// (password hashes, API keys) → gated by `SettingsManage`.
pub async fn export_backup(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    headers: HeaderMap,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let data_dir = state.config.data_dir.clone();
    let password = header_password(&headers);
    let bytes = query(&state.db, move |pool| {
        crate::services::backup::export(&pool, &data_dir, password.as_deref())
    })
    .await?;

    let ts = crate::services::scan::now_iso8601().replace(':', "-");
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"kroma-backup-{ts}.kroma\""))
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .unwrap())
}

/// `POST /api/admin/backup/import` body = a backup file (`.kroma`/`.zip`/legacy
/// `.json`) → restore it, then re-scan. `X-Backup-Password` decrypts an encrypted
/// file; `X-Backup-Reset: 1` wipes the portable tables first (clean A→B clone).
/// `SettingsManage`.
pub async fn import_backup(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let locale = super::user_locale(&user);
    let password = header_password(&headers);
    let reset = header_flag(&headers, "x-backup-reset");

    let pool = state.db.clone();
    let data_dir = state.config.data_dir.clone();
    let bytes = body.to_vec();
    let outcome = tokio::task::spawn_blocking(move || {
        crate::services::backup::import(&pool, &data_dir, &bytes, password.as_deref(), reset)
    })
    .await;

    let summary = match outcome {
        Ok(Ok(summary)) => summary,
        Ok(Err(ImportError::PasswordRequired)) => {
            return Err(lerr(locale, StatusCode::BAD_REQUEST, "admin.backupPasswordRequired"))
        }
        Ok(Err(ImportError::WrongPassword)) => {
            return Err(lerr(locale, StatusCode::BAD_REQUEST, "admin.backupWrongPassword"))
        }
        Ok(Err(ImportError::Invalid(e))) => {
            tracing::warn!(error = %e, "rejected backup import");
            return Err(lerr(locale, StatusCode::BAD_REQUEST, "admin.backupInvalid"));
        }
        Ok(Err(ImportError::Db(e))) => {
            tracing::error!(error = %e, "backup import database error");
            return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"));
        }
        Err(e) => {
            tracing::error!(error = %e, "backup import task join error");
            return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"));
        }
    };

    // Reflect the restored config, then regenerate the catalogue (same item IDs)
    // so progress/history re-link to their items. Route through the job manager so
    // the rescan shares the single-flight guard with /api/scan and watch-triggered
    // runs, instead of racing a second walk + sync on the same DB.
    state.settings.reload(&state.db);
    state.events.publish(ServerEvent::SettingsUpdated);
    let rescan_started = !matches!(
        state.jobs.trigger(state.clone(), crate::services::jobs::JobKey("library.scan"), "backup-import"),
        Err(crate::services::jobs::TriggerError::Unknown)
    );

    let counts: serde_json::Map<String, serde_json::Value> =
        summary.into_iter().map(|(t, n)| (t, json!(n))).collect();
    Ok(Json(json!({ "imported": counts, "rescanStarted": rescan_started })).into_response())
}
