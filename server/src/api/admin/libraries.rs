//! Library management: list / create / update / delete libraries and trigger
//! rescans. Library edits persist to the settings store and kick a background
//! rescan so the catalogue reflects the change.

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::error::lerr;
use crate::api::util::query;
use crate::api::extract::AuthUser;
use crate::db;
use crate::infra::events::ServerEvent;
use crate::model::Permission;
use crate::services::settings::{self, LibraryDef};
use crate::state::SharedState;
use axum::routing::{get, patch, post};
use axum::Router;

/// Admin library management. Paths are relative to the `/api/admin` nest.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/libraries", get(list_libraries).post(create_library))
        .route("/libraries/browse", get(browse_libraries))
        .route("/libraries/{id}", patch(update_library).delete(delete_library))
        .route("/libraries/{id}/scan", post(scan_library))
}

/// `GET /api/admin/libraries` → library cards (folders, size, item count).
pub async fn list_libraries(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require_any_admin(&user)?;
    let defs = settings::library_defs(&state.settings, &state.config);
    let stats = query(&state.db, move |pool| db::library_stats(&pool)).await?;
    let last_scan = crate::services::activity::snapshot(&state.activity).last_scan_at;

    let libraries: Vec<crate::api::dto::AdminLibrary> = defs
        .iter()
        .map(|d| {
            let st = stats.iter().find(|s| s.id == d.id);
            crate::api::dto::AdminLibrary {
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
pub struct BrowseQuery {
    #[serde(default)]
    pub path: Option<String>,
}

/// `GET /api/admin/libraries/browse?path=<abs>` → list the browseable
/// sub-directories of `path` so the admin UI can pick library folders off the
/// NAS filesystem instead of typing paths. With no `path`, returns the roots
/// (Synology `volumeN` dirs; falls back to `/` on a dev box with no volumes).
///
/// Response JSON:
/// `{ "path": "<current abs path|\"\">", "parent": "<abs path>"|null,
///    "entries": [ { "name": "Films", "path": "/volume1/video/Films" }, … ] }`
pub async fn browse_libraries(
    AuthUser(user): AuthUser,
    Query(q): Query<BrowseQuery>,
) -> Result<Response, Response> {
    super::require(&user, Permission::LibraryManage)?;
    let raw = q.path.unwrap_or_default();
    // Never resolve a traversal segment, even before touching the filesystem.
    if raw.contains("..") {
        return Err(lerr(super::user_locale(&user), StatusCode::FORBIDDEN, "error.forbidden"));
    }
    match tokio::task::spawn_blocking(move || browse_dirs(raw)).await {
        Ok(Ok(body)) => Ok(Json(body).into_response()),
        Ok(Err(BrowseErr::Forbidden)) => {
            Err(lerr(super::user_locale(&user), StatusCode::FORBIDDEN, "error.forbidden"))
        }
        Ok(Err(BrowseErr::NotFound)) => {
            Err(lerr(super::user_locale(&user), StatusCode::NOT_FOUND, "error.itemNotFound"))
        }
        Err(_) => Err(lerr(super::user_locale(&user), StatusCode::INTERNAL_SERVER_ERROR, "error.internal")),
    }
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
    super::require(&user, Permission::LibraryManage)?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(lerr(super::user_locale(&user), StatusCode::BAD_REQUEST, "admin.nameRequired"));
    }
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let id = crate::services::scan::short_hash(&format!("lib|{name}|{}", crate::services::auth::random_token()));
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
    pub kind: Option<String>,
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
    super::require(&user, Permission::LibraryManage)?;
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let Some(def) = defs.iter_mut().find(|d| d.id == id) else {
        return Err(lerr(super::user_locale(&user), StatusCode::NOT_FOUND, "error.libraryNotFound"));
    };
    let mut needs_scan = false;
    if let Some(name) = body.name.filter(|n| !n.trim().is_empty()) {
        def.name = name.trim().to_string();
    }
    if let Some(kind) = body.kind {
        def.kind = kind;
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
    super::require(&user, Permission::LibraryManage)?;
    let mut defs = settings::library_defs(&state.settings, &state.config);
    let before = defs.len();
    defs.retain(|d| d.id != id);
    if defs.len() == before {
        return Err(lerr(super::user_locale(&user), StatusCode::NOT_FOUND, "error.libraryNotFound"));
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
    super::require(&user, Permission::LibraryManage)?;
    spawn_rescan(state.clone());
    Ok(Json(json!({ "started": true })).into_response())
}

/// Filesystem-browse failure, mapped to an HTTP status by `browse_libraries`.
enum BrowseErr {
    /// Path escapes the allowed volume roots (403).
    Forbidden,
    /// Path is missing or not a directory (404).
    NotFound,
}

/// Blocking directory walk backing `GET /libraries/browse`. Runs on a
/// `spawn_blocking` thread. See `browse_libraries` for the response shape.
fn browse_dirs(raw: String) -> Result<Value, BrowseErr> {
    let roots = volume_roots();
    let raw = raw.trim();

    // No path → the roots: Synology volumes, or `/` on a dev machine with none.
    if raw.is_empty() {
        if !roots.is_empty() {
            return Ok(json!({ "path": "", "parent": Value::Null, "entries": to_entries(roots) }));
        }
        let entries = read_subdirs(Path::new("/"))?;
        return Ok(json!({ "path": "/", "parent": Value::Null, "entries": entries }));
    }

    let canon = std::fs::canonicalize(raw).map_err(|_| BrowseErr::NotFound)?;
    if !canon.is_dir() {
        return Err(BrowseErr::NotFound);
    }
    // When volume roots exist, confine browsing to within them.
    if !roots.is_empty() && !roots.iter().any(|r| canon.starts_with(r)) {
        return Err(BrowseErr::Forbidden);
    }

    let entries = read_subdirs(&canon)?;
    let is_root = canon == Path::new("/") || roots.contains(&canon);
    let parent = if is_root {
        Value::Null
    } else {
        canon
            .parent()
            .map(|p| Value::String(p.to_string_lossy().to_string()))
            .unwrap_or(Value::Null)
    };
    Ok(json!({ "path": canon.to_string_lossy(), "parent": parent, "entries": entries }))
}

/// Top-level `/` directories named `volume…` (Synology `/volume1`, `/volumeUSB1`).
/// Empty on a non-Synology host, which flips the browse into its dev fallback.
fn volume_roots() -> Vec<PathBuf> {
    std::fs::read_dir("/")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("volume"))
                    .unwrap_or(false)
        })
        .collect()
}

/// Immediate sub-directories of `dir` as browse entries: directories only,
/// skipping hidden/system names (`.`, `@`, `#` → e.g. `@eaDir`, `#recycle`).
fn read_subdirs(dir: &Path) -> Result<Vec<Value>, BrowseErr> {
    let rd = std::fs::read_dir(dir).map_err(|_| BrowseErr::NotFound)?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(['.', '@', '#']) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false) || entry.path().is_dir();
        if is_dir {
            dirs.push(entry.path());
        }
    }
    Ok(to_entries(dirs))
}

/// Sort paths case-insensitively by file name and map to `{ name, path }` entries.
fn to_entries(mut paths: Vec<PathBuf>) -> Vec<Value> {
    paths.sort_by_key(|p| p.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
    paths
        .iter()
        .map(|p| {
            json!({
                "name": p.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
                "path": p.to_string_lossy(),
            })
        })
        .collect()
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

/// Background rescan triggered by library edits. Routes through the job manager
/// (the same `library.scan` job as `POST /api/scan`) so it shares the single-
/// flight guard no concurrent walk + sync racing on the DB and picks up the full
/// follow-up pipeline (probe + search reindex + enrich), instead of spawning its
/// own partial pass. A no-op when a scan is already running (it covers the edit).
fn spawn_rescan(state: SharedState) {
    let _ = state.jobs.trigger(state.clone(), crate::services::jobs::JobKey("library.scan"), "library-edit");
}
