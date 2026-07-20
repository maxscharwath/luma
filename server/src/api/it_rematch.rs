//! Integration tests for the "fix a wrong TMDB match" handlers
//! (`src/api/rematch.rs`): the `library.manage` gate, the path vocabulary, and
//! the pin/clear round trip.
//!
//! The candidate *listing* needs a live TMDB call, so these cover everything up
//! to it: authorization, unknown kinds/ids, and `POST` (which touches only the
//! DB). The ranking itself is unit-tested in `services::rematch`.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{demo_item_id, demo_show_id, seed_session, send, test_app_with_tmdb};
use crate::db;
use crate::model::Permission;

#[tokio::test]
async fn rematch_requires_library_manage() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "viewer@test.dev", "viewer", &[Permission::Playback]);
    let id = demo_item_id("The Matrix");

    let (status, _) =
        send(&t.app, "GET", &format!("/api/rematch/movie/{id}/candidates"), Some(&token), None)
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &t.app,
        "POST",
        &format!("/api/rematch/movie/{id}"),
        Some(&token),
        Some(json!({ "tmdbId": 438631 })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rematch_rejects_an_anonymous_caller() {
    let t = test_app_with_tmdb();
    let id = demo_item_id("The Matrix");
    let (status, _) = send(
        &t.app,
        "POST",
        &format!("/api/rematch/movie/{id}"),
        None,
        Some(json!({ "tmdbId": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pinning_a_match_stores_it_and_clears_the_stale_metadata() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "lib@test.dev", "lib", &[Permission::LibraryManage]);
    let id = demo_item_id("The Matrix");

    let (status, _) = send(
        &t.app,
        "POST",
        &format!("/api/rematch/movie/{id}"),
        Some(&token),
        Some(json!({ "tmdbId": 438631 })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let conn = t.state.db.get().unwrap();
    assert_eq!(db::tmdb_pin::get(&conn, db::metadata_core::ITEM, &id).unwrap(), Some(438631));
    // The old identity is gone, so nothing can serve the wrong title while the
    // re-enrichment runs.
    assert!(db::get_item(&t.state.db, &id).unwrap().unwrap().metadata.is_none());
}

#[tokio::test]
async fn a_null_tmdb_id_clears_the_pin() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "lib2@test.dev", "lib2", &[Permission::LibraryManage]);
    let id = demo_item_id("The Matrix");

    let path = format!("/api/rematch/movie/{id}");
    let (status, _) =
        send(&t.app, "POST", &path, Some(&token), Some(json!({ "tmdbId": 438631 }))).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, _) =
        send(&t.app, "POST", &path, Some(&token), Some(json!({ "tmdbId": null }))).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let conn = t.state.db.get().unwrap();
    assert_eq!(db::tmdb_pin::get(&conn, db::metadata_core::ITEM, &id).unwrap(), None);
}

#[tokio::test]
async fn a_show_pins_under_its_own_subject_kind() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "lib3@test.dev", "lib3", &[Permission::LibraryManage]);
    let id = demo_show_id("The Office");

    let (status, _) = send(
        &t.app,
        "POST",
        &format!("/api/rematch/show/{id}"),
        Some(&token),
        Some(json!({ "tmdbId": 95396 })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let conn = t.state.db.get().unwrap();
    assert_eq!(db::tmdb_pin::get(&conn, db::metadata_core::SHOW, &id).unwrap(), Some(95396));
    // A show and a movie never collide, even on an identical subject id.
    assert_eq!(db::tmdb_pin::get(&conn, db::metadata_core::ITEM, &id).unwrap(), None);
}

#[tokio::test]
async fn an_unknown_kind_is_a_404() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "lib4@test.dev", "lib4", &[Permission::LibraryManage]);
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/rematch/person/whoever",
        Some(&token),
        Some(json!({ "tmdbId": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_unknown_element_does_not_leave_a_dangling_pin() {
    let t = test_app_with_tmdb();
    let (_uid, token) =
        seed_session(&t.state, "lib5@test.dev", "lib5", &[Permission::LibraryManage]);
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/rematch/movie/does-not-exist",
        Some(&token),
        Some(json!({ "tmdbId": 603 })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let conn = t.state.db.get().unwrap();
    assert_eq!(
        db::tmdb_pin::get(&conn, db::metadata_core::ITEM, "does-not-exist").unwrap(),
        None
    );
}
