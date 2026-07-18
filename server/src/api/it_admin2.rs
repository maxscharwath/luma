//! Integration tests for the admin dashboard reads + clean settings mutations
//! that don't touch the network or kick a background job: metrics, live-session
//! terminate, analytics (`admin/stats.rs`), the storage/settings guards
//! (`admin/storage.rs`, `admin/settings.rs`), and the module-store catalog
//! (`admin/store/*`) driven against a deliberately-unreachable local registry.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_session, send, test_app};
use crate::model::Permission;

/// A member with only `playback` fails every admin capability gate.
fn member(t: &crate::api::test_support::TestApp, tag: &str) -> String {
    let (_id, token) = seed_session(
        &t.state,
        &format!("{tag}@test.dev"),
        tag,
        &[Permission::Playback],
    );
    token
}

// ----- dashboard --------------------------------------------------------------

#[tokio::test]
async fn metrics_snapshot_is_admin_only() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/metrics", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_object());

    let m = member(&t, "metrics-member");
    let (status, _) = get(&t.app, "/api/admin/metrics", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn terminate_session_is_idempotent_for_an_unknown_id() {
    let t = test_app();
    // No such live session -> still a clean ack (the client should stop anyway).
    let (status, body) = send(
        &t.app,
        "POST",
        "/api/admin/sessions/ghost/stop",
        Some(&t.token),
        Some(json!({ "message": "Session terminée par l'administrateur" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], json!(true));

    let m = member(&t, "terminate-member");
    let (status, _) = send(&t.app, "POST", "/api/admin/sessions/ghost/stop", Some(&m), Some(json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ----- analytics --------------------------------------------------------------

#[tokio::test]
async fn stats_top_users_and_history_return_their_shapes() {
    let t = test_app();

    let (status, top) = get(&t.app, "/api/admin/stats/top-users?days=7", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(top["users"].is_array());

    let (status, hist) = get(&t.app, "/api/admin/stats/history?days=28", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    // 28 days -> at least one weekly bucket; totals default to zero with no history.
    assert!(hist["buckets"].as_array().map(|b| !b.is_empty()).unwrap_or(false));
    assert_eq!(hist["totalFilmsMs"], json!(0));
    assert_eq!(hist["totalTvMs"], json!(0));
}

#[tokio::test]
async fn stats_are_admin_only() {
    let t = test_app();
    let m = member(&t, "stats-member");
    for uri in ["/api/admin/stats/top-users", "/api/admin/stats/history", "/api/admin/stats/overview"] {
        let (status, _) = get(&t.app, uri, Some(&m)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} should be admin-only");
    }
}

// ----- storage / settings guards ----------------------------------------------

#[tokio::test]
async fn cache_clear_requires_settings_manage() {
    let t = test_app();
    // Guard fires before any filesystem/job work, so this never wipes a cache.
    let m = member(&t, "cache-member");
    let (status, _) = send(&t.app, "POST", "/api/admin/cache/clear", Some(&m), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn settings_read_and_write_require_settings_manage() {
    let t = test_app();
    let m = member(&t, "settings-member");
    let (status, _) = get(&t.app, "/api/admin/settings?view=general", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) =
        send(&t.app, "PUT", "/api/admin/settings", Some(&m), Some(json!({ "serverName": "x" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn settings_put_persists_live_reconfig_keys() {
    let t = test_app();

    // transcodeCacheLimit takes the HLS-budget refresh branch.
    let (status, body) = send(
        &t.app,
        "PUT",
        "/api/admin/settings",
        Some(&t.token),
        Some(json!({ "transcodeCacheLimit": "10 Go" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["updated"].as_array().unwrap().iter().any(|k| k == "transcodeCacheLimit"));

    // mediaConcurrency takes the ffmpeg-gate capacity branch.
    let (status, body) = send(
        &t.app,
        "PUT",
        "/api/admin/settings",
        Some(&t.token),
        Some(json!({ "mediaConcurrency": "3" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["updated"].as_array().unwrap().iter().any(|k| k == "mediaConcurrency"));

    // A view we didn't touch still renders (network view groups + values).
    let (status, view) = get(&t.app, "/api/admin/settings?view=transcoder", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["view"], json!("transcoder"));
}

// ----- module store catalog ---------------------------------------------------

#[tokio::test]
async fn store_catalog_requires_settings_manage() {
    let t = test_app();
    let m = member(&t, "store-member");
    let (status, _) = get(&t.app, "/api/admin/store/catalog", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn store_catalog_reports_an_unreachable_registry_cleanly() {
    let t = test_app();
    // Point the store at a local port that refuses instantly: the fetch fails,
    // the handler returns 200 with the failure in `error` + an empty module list
    // (never the network). Exercises registry_url + fetch(error) + unreachable().
    t.state.settings.set_patch(
        &t.state.db,
        [("moduleRegistryUrl".to_string(), json!("http://127.0.0.1:9/none.json"))].into_iter().collect(),
    );
    let (status, body) = get(&t.app, "/api/admin/store/catalog", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["registryUrl"], json!("http://127.0.0.1:9/none.json"));
    assert_eq!(body["modules"].as_array().map(Vec::len), Some(0));
    assert!(body["error"].is_string());
}
