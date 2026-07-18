//! Integration tests for admin library + member management (`admin/libraries.rs`,
//! `admin/users.rs`), restricted to the mutations that never kick the background
//! `library.scan` job (metadata-only edits, validation + not-found + permission
//! branches) so the suite stays deterministic.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_library, seed_session, send, test_app};
use crate::model::Permission;

fn member(t: &crate::api::test_support::TestApp, tag: &str) -> String {
    let (_id, token) = seed_session(&t.state, &format!("{tag}@test.dev"), tag, &[Permission::Playback]);
    token
}

// ----- libraries (no-scan paths) ----------------------------------------------

#[tokio::test]
async fn library_browse_lists_directories_and_blocks_traversal() {
    let t = test_app();

    // No path -> the roots (dev fallback lists `/`). Always a `{ path, entries }`.
    let (status, body) = get(&t.app, "/api/admin/libraries/browse", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    assert!(body.get("path").is_some());

    // A traversal segment is refused before any filesystem access.
    let (status, _) = get(&t.app, "/api/admin/libraries/browse?path=/etc/..", Some(&t.token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Browsing needs library.manage.
    let m = member(&t, "browse-member");
    let (status, _) = get(&t.app, "/api/admin/libraries/browse", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn library_create_rejects_a_blank_name() {
    let t = test_app();
    // Validation fails before the definition is added (so no rescan is spawned).
    let (status, _) =
        send(&t.app, "POST", "/api/admin/libraries", Some(&t.token), Some(json!({ "name": "   " }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let m = member(&t, "libcreate-member");
    let (status, _) =
        send(&t.app, "POST", "/api/admin/libraries", Some(&m), Some(json!({ "name": "Nope" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn library_rename_takes_the_no_scan_path() {
    let t = test_app();
    let id = seed_library(&t.state, "Ancien");
    // Renaming (no `folders` key) does not flag a rescan -> 204.
    let (status, _) = send(
        &t.app,
        "PATCH",
        &format!("/api/admin/libraries/{id}"),
        Some(&t.token),
        Some(json!({ "name": "Nouveau", "kind": "shows" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = get(&t.app, "/api/admin/libraries", Some(&t.token)).await;
    let lib = &body["libraries"].as_array().unwrap()[0];
    assert_eq!(lib["name"], json!("Nouveau"));
    // "shows" maps to the "tv" card label.
    assert_eq!(lib["kind"], json!("tv"));
}

#[tokio::test]
async fn library_delete_and_scan_guard_branches() {
    let t = test_app();

    // Deleting an unknown library is a 404 (returned before any rescan spawn).
    let (status, _) =
        send(&t.app, "DELETE", "/api/admin/libraries/ghost", Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Both delete + scan are library.manage-gated (guard runs before scan spawn).
    let m = member(&t, "libmanage-member");
    let (status, _) = send(&t.app, "DELETE", "/api/admin/libraries/whatever", Some(&m), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&t.app, "POST", "/api/admin/libraries/whatever/scan", Some(&m), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ----- members ----------------------------------------------------------------

#[tokio::test]
async fn admin_user_permission_edit_and_last_owner_guard() {
    let t = test_app();
    let (member_id, _) = seed_session(&t.state, "grant@test.dev", "grant", &[Permission::Playback]);

    // Promote the member to also manage libraries.
    let (status, _) = send(
        &t.app,
        "PATCH",
        &format!("/api/admin/users/{member_id}"),
        Some(&t.token),
        Some(json!({ "permissions": ["playback", "library.manage"] })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = get(&t.app, "/api/admin/users", Some(&t.token)).await;
    let m = body["users"].as_array().unwrap().iter().find(|u| u["id"] == json!(member_id)).unwrap();
    assert!(m["permissions"].as_array().unwrap().iter().any(|p| p == "library.manage"));

    // Stripping the sole owner of users.manage is barred (would lock out admin).
    let (status, _) = send(
        &t.app,
        "PATCH",
        &format!("/api/admin/users/{}", t.user_id),
        Some(&t.token),
        Some(json!({ "permissions": ["playback"] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // An unknown target is a 404.
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/admin/users/nobody",
        Some(&t.token),
        Some(json!({ "username": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_user_delete_removes_a_member_but_404s_the_unknown() {
    let t = test_app();
    let (member_id, _) = seed_session(&t.state, "gone@test.dev", "gone", &[Permission::Playback]);

    let (status, _) =
        send(&t.app, "DELETE", &format!("/api/admin/users/{member_id}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = get(&t.app, "/api/admin/users", Some(&t.token)).await;
    assert!(!body["users"].as_array().unwrap().iter().any(|u| u["id"] == json!(member_id)));

    let (status, _) =
        send(&t.app, "DELETE", "/api/admin/users/nobody", Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn member_cannot_reach_user_management() {
    let t = test_app();
    let m = member(&t, "usermgmt-member");
    let (status, _) = send(&t.app, "PATCH", "/api/admin/users/x", Some(&m), Some(json!({ "username": "y" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&t.app, "DELETE", "/api/admin/users/x", Some(&m), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
