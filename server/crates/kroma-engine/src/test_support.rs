//! Reusable `#[cfg(test)]` harness for services that need a full [`SharedState`].
//!
//! [`test_state`] builds a minimal, real [`AppState`] over a fresh temp-file
//! SQLite DB (unique per test, like the kroma-db `#[cfg(test)]` pattern), a no-op
//! [`Embedder`](crate::ports::Embedder), no TMDB key and no `web_dir`. Nothing here
//! talks to the network, a module sidecar, or `ffmpeg`; `module_services` is empty
//! and `module_jobs` is `&[]`, exactly like the binary's `api::test_support`.
//!
//! The small `seed_*` helpers insert the catalog rows a service under test reads
//! (a library, a movie, a show + episode, a pipeline-ledger task) via raw SQL with
//! test-controlled literals, mirroring the seeding style of the existing db tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::config::Config;
use crate::db;
use crate::ports::{Embedder, NoopEmbedder};
use crate::services::settings::Settings;
use crate::state::{AppState, SharedState};

/// Monotonic counter making per-test temp paths unique (paired with the pid),
/// mirroring the kroma-db test harness.
static SEQ: AtomicU32 = AtomicU32::new(0);

/// A unique temp data dir for one test (removed + recreated so a rerun is clean).
fn unique_data_dir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("kroma-engine-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp data dir");
    dir
}

/// A minimal [`Config`]: a temp `data_dir`, no media dirs (nothing to scan), no
/// TMDB key (network features cleanly no-op), no `web_dir`. Same field literal the
/// binary's `api::test_support::test_config` builds.
fn test_config(data_dir: PathBuf) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 0,
        data_dir,
        tmdb_language: "en-US".into(),
        ..Default::default()
    }
}

/// Build a minimal, real [`SharedState`]: fresh temp DB, loaded settings, a no-op
/// embedder, empty module services, no module jobs, `ffprobe_available = false`.
pub(crate) fn test_state() -> SharedState {
    let data_dir = unique_data_dir();
    let db = db::init(&data_dir.join("kroma.db")).expect("init db");
    let config = test_config(data_dir);
    let settings = Settings::load(&db);
    let embedder: Arc<dyn Embedder> = Arc::new(NoopEmbedder);
    AppState::new(config, false, db, settings, embedder, HashMap::new(), &[])
}

/// Insert a library row (idempotent). `kind` is `"movies" | "shows" | "mixed"`.
pub(crate) fn seed_library(state: &SharedState, id: &str, kind: &str) {
    state
        .db
        .get()
        .unwrap()
        .execute(
            &format!(
                "INSERT OR IGNORE INTO libraries (id,name,kind,path,added_at) \
                 VALUES ('{id}','Lib {id}','{kind}','/x/{id}','t')"
            ),
            [],
        )
        .unwrap();
}

/// Insert a movie item (creating a `movies` library if needed). `abs_path` is a
/// (non-existent) file path so cache-invalidation paths have something to touch.
pub(crate) fn seed_movie(state: &SharedState, id: &str) {
    seed_library(state, "lib-movies", "movies");
    let conn = state.db.get().unwrap();
    conn.execute(
        &format!(
            "INSERT INTO items (id,kind,title,container,library,abs_path,added_at) \
             VALUES ('{id}','movie','Title {id}','mkv','lib-movies','/media/{id}.mkv','t')"
        ),
        [],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO files (id,item_id,abs_path) VALUES ('{id}-f','{id}','/media/{id}.mkv')"),
        [],
    )
    .unwrap();
}

/// Insert a show plus one episode under season 1 (creating a `shows` library if
/// needed). Returns `(show_id, episode_id)` for convenience.
pub(crate) fn seed_show_episode(state: &SharedState, show_id: &str, ep_id: &str) -> (String, String) {
    seed_library(state, "lib-shows", "shows");
    let conn = state.db.get().unwrap();
    conn.execute(
        &format!(
            "INSERT INTO shows (id,library,title,added_at) VALUES ('{show_id}','lib-shows','Show {show_id}','t')"
        ),
        [],
    )
    .unwrap();
    conn.execute(
        &format!(
            "INSERT INTO items (id,kind,title,container,library,show_id,season,episode,abs_path,added_at) \
             VALUES ('{ep_id}','episode','Ep {ep_id}','mkv','lib-shows','{show_id}',1,1,'/media/{ep_id}.mkv','t')"
        ),
        [],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO files (id,item_id,abs_path) VALUES ('{ep_id}-f','{ep_id}','/media/{ep_id}.mkv')"),
        [],
    )
    .unwrap();
    (show_id.to_string(), ep_id.to_string())
}

/// Insert one pipeline-ledger task in an explicit `status`, with an optional error
/// message (for `failed` rows). Use [`crate::db::pipeline::enqueue`] instead when a
/// plain `pending` task suffices.
pub(crate) fn seed_task(
    state: &SharedState,
    stage: &str,
    subject_kind: &str,
    subject_id: &str,
    status: &str,
    error: Option<&str>,
) {
    let err_sql = match error {
        Some(e) => format!("'{e}'"),
        None => "NULL".to_string(),
    };
    state
        .db
        .get()
        .unwrap()
        .execute(
            &format!(
                "INSERT INTO pipeline_tasks \
                   (stage,subject_kind,subject_id,status,error,enqueued_at,updated_at) \
                 VALUES ('{stage}','{subject_kind}','{subject_id}','{status}',{err_sql},1,1)"
            ),
            [],
        )
        .unwrap();
}

/// Record a finished play in `play_history` (for the trending / for-you home rows).
/// `ended_at` is epoch **seconds** (the table's convention), so pass a recent
/// `now`-ish value for the row to fall inside the trending window.
pub(crate) fn seed_play(state: &SharedState, user_id: &str, item_id: &str, ended_at: i64) {
    let id = format!("h-{}", SEQ.fetch_add(1, Ordering::Relaxed));
    state
        .db
        .get()
        .unwrap()
        .execute(
            &format!(
                "INSERT INTO play_history (id,user_id,item_id,kind,title,started_at,ended_at,watched_ms) \
                 VALUES ('{id}','{user_id}','{item_id}','movie','Title {item_id}',{start},{ended_at},1000)",
                start = ended_at - 100
            ),
            [],
        )
        .unwrap();
}

/// Epoch **seconds** "now" for seeding recent `play_history` rows.
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
