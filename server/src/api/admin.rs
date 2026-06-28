//! Admin console API (`/api/admin/*`). Backs the "Admin Serveur" dashboard:
//! live sessions, system metrics, storage, users, libraries, settings and
//! analytics. Every route is gated by a capability (see [`require`] /
//! [`require_any_admin`]); reads need *any* admin capability, writes need the
//! specific one.

use std::collections::BTreeMap;
use std::path::Path;

use axum::extract::{Path as AxPath, Query, State};
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::error::lerr;
use crate::api::handlers::{blocking, query};
use crate::auth::AuthUser;
use crate::db;
use crate::events::ServerEvent;
use crate::i18n;
use crate::model::{Permission, User};
use crate::settings::{self, LibraryDef};
use crate::state::SharedState;

// ----- guards -----------------------------------------------------------------

/// The admin's account locale. Admin endpoints are always authenticated, so the
/// (account-synced) preference is the right source for server-rendered strings —
/// no `Accept-Language` needed. Falls back to the default for an unset/unknown
/// preference.
fn user_locale(user: &User) -> &'static str {
    user.language
        .as_deref()
        .and_then(i18n::normalize)
        .unwrap_or(i18n::DEFAULT_LOCALE)
}

fn require(user: &User, perm: Permission) -> Result<(), Response> {
    if user.can(perm) {
        Ok(())
    } else {
        Err(lerr(user_locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

/// Any management capability unlocks the read-only dashboard panels.
fn require_any_admin(user: &User) -> Result<(), Response> {
    if user.can(Permission::UsersManage)
        || user.can(Permission::LibraryManage)
        || user.can(Permission::SettingsManage)
    {
        Ok(())
    } else {
        Err(lerr(user_locale(user), StatusCode::FORBIDDEN, "error.permissionDenied"))
    }
}

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

// ----- server status ----------------------------------------------------------

/// `GET /api/admin/server` → identity + uptime for the sidebar status card.
pub async fn server_info(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let hostname = sysinfo::System::host_name().unwrap_or_else(|| "luma".into());
    Ok(Json(super::dto::ServerInfo {
        name: settings::server_name(&state.settings),
        hostname,
        version: env!("CARGO_PKG_VERSION"),
        uptime_sec: crate::process_started().elapsed().as_secs(),
        online: true,
        sessions: state.playback.list().len(),
    })
    .into_response())
}

// ----- live sessions ----------------------------------------------------------

/// `GET /api/admin/sessions` → live "En cours de lecture" sessions.
pub async fn sessions(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    Ok(Json(json!({ "sessions": state.playback.list() })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct TerminateBody {
    #[serde(default)]
    pub message: Option<String>,
}

/// `POST /api/admin/sessions/:id/stop` → terminate a live playback session. The
/// owning client (web/TV) receives a `playback.terminate` event over the WS bus,
/// stops the video, and shows `message` (empty → a localized default). Idempotent.
pub async fn terminate_session(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<TerminateBody>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    // Drop it from the registry (grace window blocks re-registration) + log it.
    if let Some(session) = state.playback.terminate(&id) {
        let _ = query(&state.db, move |pool| {
            crate::playback::record(&pool, &session);
            Ok(())
        })
        .await;
    }
    let message = body
        .message
        .map(|m| m.trim().chars().take(200).collect::<String>())
        .unwrap_or_default();
    state
        .events
        .publish(ServerEvent::PlaybackTerminate { session_id: id, message });
    state
        .events
        .publish(ServerEvent::PlaybackStopped { count: state.playback.list().len() });
    Ok(Json(json!({ "ok": true })).into_response())
}

// ----- metrics ----------------------------------------------------------------

/// `GET /api/admin/metrics` → CPU / RAM / bandwidth snapshot + history.
pub async fn metrics(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    Ok(Json(state.metrics.snapshot()).into_response())
}

// ----- storage ----------------------------------------------------------------

/// `GET /api/admin/storage` → volumes, totals, and cache usage.
pub async fn storage(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let data_dir = state.config.data_dir.clone();
    let (volumes, media_bytes, cache_bytes) = query(&state.db, move |pool| {
        let volumes = crate::metrics::read_disks();
        let media = db::total_media_bytes(&pool).unwrap_or(0).max(0) as u64;
        let cache =
            dir_size(&data_dir.join("transcode")) + dir_size(&data_dir.join("images"));
        Ok((volumes, media, cache))
    })
    .await?;

    let total: u64 = volumes.iter().map(|v| v.total_bytes).sum();
    let used: u64 = volumes.iter().map(|v| v.used_bytes).sum();
    Ok(Json(super::dto::StorageInfo {
        volumes,
        total_bytes: total,
        used_bytes: used,
        available_bytes: total.saturating_sub(used),
        media_bytes,
        cache: super::dto::CacheInfo {
            dir: state.config.data_dir.join("transcode").to_string_lossy().into_owned(),
            bytes: cache_bytes,
            limit: state.settings.get_str("cacheLimit", "80 Go"),
        },
    })
    .into_response())
}

/// `POST /api/admin/cache/clear` → wipe transcode + image caches.
pub async fn clear_cache(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let data_dir = state.config.data_dir.clone();
    let freed = blocking(move || {
        let transcode = data_dir.join("transcode");
        let images = data_dir.join("images");
        let freed = dir_size(&transcode) + dir_size(&images);
        clear_dir(&transcode);
        clear_dir(&images);
        Ok(freed)
    })
    .await?;
    Ok(Json(json!({ "freedBytes": freed })).into_response())
}

// ----- users ------------------------------------------------------------------

/// `GET /api/admin/users` → full member list (the "Membres & partage" table).
pub async fn list_users(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require(&user, Permission::UsersManage)?;
    let (mut users, library_count) =
        query(&state.db, move |pool| Ok((db::admin_users(&pool)?, db::counts(&pool)?.0))).await?;
    for u in &mut users {
        u.online = state.playback.user_online(&u.id);
    }
    Ok(Json(super::dto::AdminUsers { users, library_count }).into_response())
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
    require(&user, Permission::UsersManage)?;
    let id2 = id.clone();
    let all = query(&state.db, move |pool| db::admin_users(&pool)).await?;
    let Some(target) = all.iter().find(|u| u.id == id2) else {
        return Err(lerr(user_locale(&user), StatusCode::NOT_FOUND, "error.userNotFound"));
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
                user_locale(&user),
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
    require(&user, Permission::UsersManage)?;
    if id == user.id {
        return Err(lerr(user_locale(&user), StatusCode::BAD_REQUEST, "admin.cantDeleteSelf"));
    }
    let id2 = id.clone();
    let all = query(&state.db, move |pool| db::admin_users(&pool)).await?;
    let Some(target) = all.iter().find(|u| u.id == id2) else {
        return Err(lerr(user_locale(&user), StatusCode::NOT_FOUND, "error.userNotFound"));
    };
    let owners = all
        .iter()
        .filter(|u| u.permissions.contains(&Permission::UsersManage))
        .count();
    if target.permissions.contains(&Permission::UsersManage) && owners <= 1 {
        return Err(lerr(user_locale(&user), StatusCode::BAD_REQUEST, "admin.cantDeleteLastOwner"));
    }
    query(&state.db, move |pool| db::delete_user(&pool, &id)).await?;
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ----- libraries --------------------------------------------------------------

/// `GET /api/admin/libraries` → library cards (folders, size, item count).
pub async fn list_libraries(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let defs = settings::library_defs(&state.settings, &state.config);
    let stats = query(&state.db, move |pool| db::library_stats(&pool)).await?;
    let last_scan = crate::activity::snapshot(&state.activity).last_scan_at;

    let libraries: Vec<super::dto::AdminLibrary> = defs
        .iter()
        .map(|d| {
            let st = stats.iter().find(|s| s.id == d.id);
            super::dto::AdminLibrary {
                id: d.id.clone(),
                name: d.name.clone(),
                kind: kind_label(d, st),
                folders: d.folders.clone(),
                item_count: st.map(|s| s.item_count).unwrap_or(0),
                size_bytes: st.map(|s| s.total_bytes).unwrap_or(0),
                last_scan: last_scan.clone(),
                auto_scan: d.auto_scan,
            }
        })
        .collect();
    Ok(Json(json!({ "libraries": libraries })).into_response())
}

fn kind_label(def: &LibraryDef, _st: Option<&crate::model::LibraryStat>) -> String {
    match def.kind.as_str() {
        "shows" => "tv",
        "movies" => "film",
        "music" => "music",
        "photo" => "photo",
        _ => "film",
    }
    .to_string()
}

#[derive(Debug, Deserialize)]
pub struct CreateLibraryBody {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub folders: Vec<String>,
}

/// `POST /api/admin/libraries` → add a library, then rescan.
pub async fn create_library(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateLibraryBody>,
) -> Result<Response, Response> {
    require(&user, Permission::LibraryManage)?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(lerr(user_locale(&user), StatusCode::BAD_REQUEST, "admin.nameRequired"));
    }
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let id = crate::scan::short_hash(&format!("lib|{name}|{}", crate::auth::random_token()));
    defs.push(LibraryDef {
        id: id.clone(),
        name,
        kind: body.kind.unwrap_or_default(),
        folders: clean_folders(body.folders),
        auto_scan: true,
    });
    settings::set_library_defs(&state.settings, &state.db, &defs);
    spawn_rescan(state.clone());
    Ok(Json(json!({ "id": id })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct UpdateLibraryBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub folders: Option<Vec<String>>,
    #[serde(rename = "autoScan", default)]
    pub auto_scan: Option<bool>,
}

/// `PATCH /api/admin/libraries/:id` → rename / change folders / toggle auto-scan.
pub async fn update_library(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
    Json(body): Json<UpdateLibraryBody>,
) -> Result<Response, Response> {
    require(&user, Permission::LibraryManage)?;
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let Some(def) = defs.iter_mut().find(|d| d.id == id) else {
        return Err(lerr(user_locale(&user), StatusCode::NOT_FOUND, "error.libraryNotFound"));
    };
    let mut needs_scan = false;
    if let Some(name) = body.name.filter(|n| !n.trim().is_empty()) {
        def.name = name.trim().to_string();
    }
    if let Some(folders) = body.folders {
        def.folders = clean_folders(folders);
        needs_scan = true;
    }
    if let Some(auto) = body.auto_scan {
        def.auto_scan = auto;
    }
    settings::set_library_defs(&state.settings, &state.db, &defs);
    if needs_scan {
        spawn_rescan(state.clone());
    }
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `DELETE /api/admin/libraries/:id` → remove a library and rescan (the vanished
/// library + its items are cascade-deleted by the diff-sync).
pub async fn delete_library(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(id): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::LibraryManage)?;
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let before = defs.len();
    defs.retain(|d| d.id != id);
    if defs.len() == before {
        return Err(lerr(user_locale(&user), StatusCode::NOT_FOUND, "error.libraryNotFound"));
    }
    settings::set_library_defs(&state.settings, &state.db, &defs);
    spawn_rescan(state.clone());
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `POST /api/admin/libraries/:id/scan` (and any library) → kick a full rescan.
pub async fn scan_library(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    AxPath(_id): AxPath<String>,
) -> Result<Response, Response> {
    require(&user, Permission::LibraryManage)?;
    spawn_rescan(state.clone());
    Ok(Json(json!({ "started": true })).into_response())
}

/// Clean a folder list: trim, drop empties, dedupe.
fn clean_folders(folders: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    folders
        .into_iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty() && seen.insert(f.clone()))
        .collect()
}

/// Background rescan triggered by library edits — mirrors the `/api/scan`
/// handler but spawned so the admin request returns immediately.
fn spawn_rescan(state: SharedState) {
    tokio::spawn(async move {
        let defs = settings::library_defs(&state.settings, &state.config);
        let has_folders = defs.iter().any(|d| !d.folders.is_empty());
        state.events.publish(ServerEvent::ScanStarted);
        crate::activity::scan_started(&state.activity);

        let res = query(&state.db, move |pool| {
            let mut data = crate::scan::scan_all(&defs);
            if data.items.is_empty() && !has_folders {
                data = crate::demo::demo_data();
            }
            db::sync_all(&pool, &data.libraries, &data.shows, &data.items, &data.mtimes)?;
            Ok(data)
        })
        .await;

        if let Ok(data) = res {
            let (l, s, i) = (data.libraries.len(), data.shows.len(), data.items.len());
            crate::activity::scan_completed(&state.activity, l, s, i, crate::scan::now_iso8601());
            state
                .events
                .publish(ServerEvent::ScanCompleted { items: i, shows: s, libraries: l });
            state.events.publish(ServerEvent::LibraryUpdated);
            crate::probe::spawn_probe_pass(
                state.db.clone(),
                state.ffprobe_available,
                state.events.clone(),
                state.activity.clone(),
            );
            crate::enrich::maybe_spawn(&state, &data.items, &data.shows);
        }
    });
}

// ----- settings ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    #[serde(default)]
    pub view: Option<String>,
}

/// `GET /api/admin/settings?view=general|network|transcoder` → grouped schema +
/// current values.
pub async fn get_settings(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(q): Query<SettingsQuery>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let view = q.view.unwrap_or_else(|| "general".into());
    let groups = settings::groups(&view, &state.settings, &state.config, user_locale(&user));
    Ok(Json(super::dto::SettingsView { view, groups }).into_response())
}

/// `PUT /api/admin/settings` body = `{ key: value, … }` → persist a patch.
pub async fn put_settings(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(patch): Json<BTreeMap<String, Value>>,
) -> Result<Response, Response> {
    require(&user, Permission::SettingsManage)?;
    let written = state.settings.set_patch(&state.db, patch);
    state.events.publish(ServerEvent::SettingsUpdated);
    Ok(Json(json!({ "updated": written })).into_response())
}

// ----- analytics --------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DaysQuery {
    #[serde(default)]
    pub days: Option<i64>,
}

/// `GET /api/admin/stats/top-users?days=7` → per-user watch aggregates.
pub async fn top_users(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(q): Query<DaysQuery>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let days = q.days.unwrap_or(7).clamp(1, 365);
    let since = now_unix() - days * 86_400;
    let users = query(&state.db, move |pool| db::top_users(&pool, since, 12)).await?;
    Ok(Json(json!({ "users": users })).into_response())
}

/// `GET /api/admin/stats/history?days=28` → weekly films-vs-TV watch buckets.
pub async fn history(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Query(q): Query<DaysQuery>,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let days = q.days.unwrap_or(28).clamp(7, 365);
    let now = now_unix();
    let since = now - days * 86_400;
    let rows = query(&state.db, move |pool| db::history_since(&pool, since)).await?;

    // Weekly buckets covering [since, now].
    let week = 7 * 86_400;
    let buckets = ((days + 6) / 7).max(1);
    let mut films = vec![0i64; buckets as usize];
    let mut tv = vec![0i64; buckets as usize];
    for r in &rows {
        let idx = (((r.ended_at - since) / week).clamp(0, buckets - 1)) as usize;
        match r.kind {
            crate::model::Kind::Movie => films[idx] += r.watched_ms,
            _ => tv[idx] += r.watched_ms,
        }
    }
    let out: Vec<super::dto::HistoryBucket> = (0..buckets as usize)
        .map(|i| {
            let start = since + (i as i64) * week;
            super::dto::HistoryBucket {
                label: date_range_label(start, (start + week).min(now)),
                films_ms: films[i],
                tv_ms: tv[i],
            }
        })
        .collect();
    Ok(Json(super::dto::HistoryStats {
        total_films_ms: films.iter().sum::<i64>(),
        total_tv_ms: tv.iter().sum::<i64>(),
        buckets: out,
    })
    .into_response())
}

/// `GET /api/admin/stats/overview` → top-line counts for the users page.
pub async fn overview(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    require_any_admin(&user)?;
    let (libraries, items, shows, users, invites) = query(&state.db, move |pool| {
        let (libraries, items, shows) = db::counts(&pool)?;
        let users = db::admin_users(&pool)?;
        let invites = db::list_invites(&pool)?.len();
        Ok((libraries, items, shows, users, invites))
    })
    .await?;
    let online = users
        .iter()
        .filter(|u| state.playback.user_online(&u.id))
        .count();
    Ok(Json(super::dto::AdminOverview {
        users: users.len(),
        online,
        invites,
        items,
        shows,
        libraries,
    })
    .into_response())
}

// ----- helpers ----------------------------------------------------------------

/// Recursive byte size of a directory tree (0 if missing).
fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Remove a directory's contents (keeping the directory itself).
fn clear_dir(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// "DD/MM–DD/MM" label for a weekly bucket.
fn date_range_label(start: i64, end: i64) -> String {
    let fmt = |ts: i64| {
        time::OffsetDateTime::from_unix_timestamp(ts)
            .map(|d| format!("{:02}/{:02}", d.day(), d.month() as u8))
            .unwrap_or_else(|_| "??".into())
    };
    format!("{}–{}", fmt(start), fmt(end))
}
