//! Integration tests for the per-user playback surface (`src/api/playback.rs`):
//! the live-session heartbeat, watched markers, up-next / next-episode, and the
//! progress edge cases (clamping, empty reads, malformed bodies). Demo items
//! carry no real file, so nothing here touches ffmpeg or the disk.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{demo_item_id, demo_show_id, get, send, test_app};

// ----- live-session heartbeat -------------------------------------------------

#[tokio::test]
async fn ping_upserts_then_stop_ends_the_session() {
    let t = test_app();
    let item = demo_item_id("The Matrix");

    // First beat: creates the live session (item snapshot built from the DB).
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/playback/ping",
        Some(&t.token),
        Some(json!({ "sessionId": "sess-1", "itemId": item, "positionMs": 1000, "durationMs": 8_160_000 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, admin) = get(&t.app, "/api/admin/sessions", Some(&t.token)).await;
    assert_eq!(admin["sessions"].as_array().map(Vec::len), Some(1));

    // Second beat on the same session takes the update path.
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/playback/ping",
        Some(&t.token),
        Some(json!({ "sessionId": "sess-1", "itemId": item, "positionMs": 2000, "state": "paused" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Stop ends it and the live list drops back to empty.
    let (status, _) =
        send(&t.app, "POST", "/api/playback/stop", Some(&t.token), Some(json!({ "sessionId": "sess-1" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, admin) = get(&t.app, "/api/admin/sessions", Some(&t.token)).await;
    assert_eq!(admin["sessions"].as_array().map(Vec::len), Some(0));
}

// ----- progress edge cases ----------------------------------------------------

#[tokio::test]
async fn progress_clamps_negative_positions_to_zero() {
    let t = test_app();
    let item = demo_item_id("Sintel");
    let (status, _) = send(
        &t.app,
        "PUT",
        &format!("/api/progress/{item}"),
        Some(&t.token),
        Some(json!({ "positionMs": -500, "durationMs": 888_000 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, entry) = get(&t.app, &format!("/api/progress/{item}"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entry["positionMs"], json!(0));
}

#[tokio::test]
async fn progress_accepts_a_missing_duration() {
    let t = test_app();
    let item = demo_item_id("Big Buck Bunny");
    // `durationMs` is optional; a body without it still saves.
    let (status, _) = send(
        &t.app,
        "PUT",
        &format!("/api/progress/{item}"),
        Some(&t.token),
        Some(json!({ "positionMs": 3000 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn progress_rejects_a_body_missing_the_position() {
    let t = test_app();
    let item = demo_item_id("Big Buck Bunny");
    let (status, _) = send(
        &t.app,
        "PUT",
        &format!("/api/progress/{item}"),
        Some(&t.token),
        Some(json!({ "durationMs": 100 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn progress_read_for_an_unknown_item_is_null() {
    let t = test_app();
    let (status, entry) = get(&t.app, "/api/progress/does-not-exist", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(entry.is_null());
}

// ----- watched markers --------------------------------------------------------

#[tokio::test]
async fn watched_marker_add_list_and_clear() {
    let t = test_app();
    let item = demo_item_id("The Matrix");

    let (status, _) = send(&t.app, "PUT", &format!("/api/watched/{item}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = get(&t.app, "/api/watched", Some(&t.token)).await;
    assert_eq!(list, json!([item]));

    let (status, _) = send(&t.app, "DELETE", &format!("/api/watched/{item}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = get(&t.app, "/api/watched", Some(&t.token)).await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn marking_watched_clears_the_resume_position() {
    let t = test_app();
    let item = demo_item_id("Dune Part Two");

    // Save a resume position, then mark the title watched.
    send(
        &t.app,
        "PUT",
        &format!("/api/progress/{item}"),
        Some(&t.token),
        Some(json!({ "positionMs": 500_000, "durationMs": 9_960_000 })),
    )
    .await;
    let (status, _) = send(&t.app, "PUT", &format!("/api/watched/{item}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Watched drops it from Continue watching (resume position cleared).
    let (_, entry) = get(&t.app, &format!("/api/progress/{item}"), Some(&t.token)).await;
    assert!(entry.is_null(), "resume position should be gone after watched");
}

// ----- up-next / next-episode -------------------------------------------------

#[tokio::test]
async fn up_next_points_at_the_first_episode_of_a_fresh_show() {
    let t = test_app();
    let show = demo_show_id("Planet Earth II");

    let (status, up) = get(&t.app, &format!("/api/shows/{show}/up-next"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    // No progress yet -> the very first episode (S1E1 "Islands"), not resuming.
    assert_eq!(up["item"]["episodeTitle"], json!("Islands"));
    assert_eq!(up["resume"], json!(false));

    // An unknown show has nothing to play next.
    let (status, up) = get(&t.app, "/api/shows/ghost/up-next", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(up.is_null());
}

#[tokio::test]
async fn next_episode_walks_the_sequence_then_ends() {
    let t = test_app();
    let ep1 = demo_item_id("Islands"); // Planet Earth II S1E1
    let ep2 = demo_item_id("Mountains"); // S1E2 (last)

    let (status, next) = get(&t.app, &format!("/api/items/{ep1}/next"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(next["id"], json!(ep2));

    // The last episode has no successor.
    let (status, next) = get(&t.app, &format!("/api/items/{ep2}/next"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(next.is_null());

    // A movie is not part of any show sequence.
    let movie = demo_item_id("The Matrix");
    let (_, next) = get(&t.app, &format!("/api/items/{movie}/next"), Some(&t.token)).await;
    assert!(next.is_null());
}
