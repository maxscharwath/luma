//! Integration tests for the admin console endpoints (`src/api/admin/*`), driven
//! with an all-permissions owner. Covers the capability guards plus the
//! dashboard reads and a couple of clean mutations (no background scan / network).

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_library, seed_session, send, test_app};
use crate::model::Permission;

// ----- guards -----------------------------------------------------------------

#[tokio::test]
async fn admin_users_requires_authentication() {
    let t = test_app();
    let (status, _) = get(&t.app, "/api/admin/users", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_users_forbids_a_non_admin() {
    let t = test_app();
    let (_, member) = seed_session(&t.state, "viewer@test.dev", "viewer", &[Permission::Playback]);
    let (status, _) = get(&t.app, "/api/admin/users", Some(&member)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ----- dashboard reads --------------------------------------------------------

#[tokio::test]
async fn server_info_reports_identity_for_an_admin() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/server", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["name"].is_string());
    assert!(body["version"].is_string());
    assert_eq!(body["online"], json!(true));
}

#[tokio::test]
async fn admin_users_lists_the_owner() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/users", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let users = body["users"].as_array().expect("users array");
    assert!(users.iter().any(|u| u["id"] == json!(t.user_id)));
    assert!(body["libraryCount"].is_number());
}

#[tokio::test]
async fn stats_overview_counts_the_demo_catalogue() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/stats/overview", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    // Demo seed: 2 libraries, 2 shows, 10 items (6 movies + 4 episodes).
    assert_eq!(body["items"], json!(10));
    assert_eq!(body["shows"], json!(2));
    assert_eq!(body["libraries"], json!(2));
    assert_eq!(body["users"], json!(1));
}

#[tokio::test]
async fn storage_reports_volumes_and_cache() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/storage", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["volumes"].is_array());
    assert!(body["cache"].is_object());
    assert!(body["cache"]["bytes"].is_number());
}

#[tokio::test]
async fn settings_view_returns_grouped_schema() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/admin/settings?view=general", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["view"], json!("general"));
    assert!(body["groups"].is_array());
}

// ----- mutations (no scan, no network) ----------------------------------------

#[tokio::test]
async fn settings_put_persists_a_known_key() {
    let t = test_app();
    let (status, body) =
        send(&t.app, "PUT", "/api/admin/settings", Some(&t.token), Some(json!({ "serverName": "Ma Kroma" }))).await;
    assert_eq!(status, StatusCode::OK);
    let updated = body["updated"].as_array().expect("updated array");
    assert!(updated.iter().any(|k| k == &json!("serverName")));

    // The new value is reflected in the server identity card.
    let (_, info) = get(&t.app, "/api/admin/server", Some(&t.token)).await;
    assert_eq!(info["name"], json!("Ma Kroma"));
}

#[tokio::test]
async fn settings_put_ignores_an_unknown_key() {
    let t = test_app();
    let (status, body) =
        send(&t.app, "PUT", "/api/admin/settings", Some(&t.token), Some(json!({ "totallyMadeUp": 1 }))).await;
    assert_eq!(status, StatusCode::OK);
    // Unknown keys are dropped (not persisted), so nothing is reported written.
    assert_eq!(body["updated"], json!([]));
}

#[tokio::test]
async fn admin_libraries_lists_a_seeded_definition() {
    let t = test_app();
    // No library defs configured -> empty list.
    let (status, body) = get(&t.app, "/api/admin/libraries", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["libraries"].as_array().map(Vec::len), Some(0));

    // Seed one, then it shows up.
    let id = seed_library(&t.state, "Mes Films");
    let (status, body) = get(&t.app, "/api/admin/libraries", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let libs = body["libraries"].as_array().expect("libraries array");
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0]["id"], json!(id));
    assert_eq!(libs[0]["name"], json!("Mes Films"));
}

#[tokio::test]
async fn admin_library_patch_toggles_auto_scan() {
    let t = test_app();
    let id = seed_library(&t.state, "Séries");
    // A metadata-only patch (no `folders`) takes the no-rescan path -> 204.
    let (status, _) = send(
        &t.app,
        "PATCH",
        &format!("/api/admin/libraries/{id}"),
        Some(&t.token),
        Some(json!({ "autoScan": false })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = get(&t.app, "/api/admin/libraries", Some(&t.token)).await;
    let lib = &body["libraries"].as_array().unwrap()[0];
    assert_eq!(lib["autoScan"], json!(false));

    // An unknown library id is a clean 404.
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/admin/libraries/ghost",
        Some(&t.token),
        Some(json!({ "autoScan": true })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_user_patch_renames_a_member() {
    let t = test_app();
    let (member_id, _) = seed_session(&t.state, "bob@test.dev", "bob", &[Permission::Playback]);

    let (status, _) = send(
        &t.app,
        "PATCH",
        &format!("/api/admin/users/{member_id}"),
        Some(&t.token),
        Some(json!({ "username": "Bobby" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = get(&t.app, "/api/admin/users", Some(&t.token)).await;
    let member = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == json!(member_id))
        .expect("member present");
    assert_eq!(member["username"], json!("Bobby"));
}

#[tokio::test]
async fn admin_user_delete_rejects_removing_the_last_owner() {
    let t = test_app();
    // The seeded owner is the only `users.manage` holder; deleting self is barred
    // and it is also the last owner -- the handler returns 400 either way.
    let (status, _) =
        send(&t.app, "DELETE", &format!("/api/admin/users/{}", t.user_id), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
