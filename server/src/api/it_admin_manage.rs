//! Integration tests for admin library + member management (`admin/libraries.rs`,
//! `admin/users.rs`), restricted to the mutations that never kick the background
//! `library.scan` job (metadata-only edits, validation + not-found + permission
//! branches) so the suite stays deterministic.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_library, seed_library_kind, seed_session, send, test_app};
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
async fn library_cards_map_each_kind_to_its_label() {
    let t = test_app();
    // Seed one library per kind (settings-only, no scan) and check the card label.
    seed_library_kind(&t.state, "Musique", "music");
    seed_library_kind(&t.state, "Photos", "photo");
    seed_library_kind(&t.state, "Bizarre", "quux"); // unknown -> the "film" default.

    let (status, body) = get(&t.app, "/api/admin/libraries", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let by_name = |name: &str| -> String {
        body["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["name"] == json!(name))
            .unwrap_or_else(|| panic!("library {name} missing"))["kind"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(by_name("Musique"), "music");
    assert_eq!(by_name("Photos"), "photo");
    assert_eq!(by_name("Bizarre"), "film");
}

#[tokio::test]
async fn library_browse_reads_a_real_directory_and_404s_a_missing_one() {
    let t = test_app();

    // Browsing an existing absolute path returns its sub-dirs + a parent link.
    let dir = t.state.config.data_dir.to_string_lossy().to_string();
    let (status, body) =
        get(&t.app, &format!("/api/admin/libraries/browse?path={dir}"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].is_array());
    assert!(body["path"].is_string());
    assert!(body["parent"].is_string(), "a non-root dir exposes its parent");

    // A non-existent path fails canonicalisation -> a clean 404.
    let (status, _) =
        get(&t.app, "/api/admin/libraries/browse?path=/no/such/kroma/dir/xyz", Some(&t.token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
