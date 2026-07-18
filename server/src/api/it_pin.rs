//! Integration tests for the profile-PIN handlers (`src/api/pin.rs`): set /
//! rotate / clear (with the current-PIN guard), verify, and the brute-force
//! lockout. All DB-only; the lockout static is keyed by the (unique) user id so
//! these can't contaminate one another.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{seed_session, send, test_app};
use crate::model::Permission;

#[tokio::test]
async fn pin_set_verify_and_clear_flow() {
    let t = test_app();
    let (_uid, token) = seed_session(&t.state, "pin@test.dev", "pinner", &[Permission::Playback]);

    // A non-4-digit PIN is rejected.
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me/pin", Some(&token), Some(json!({ "pin": "12" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Setting a fresh PIN needs no `current`.
    let (status, body) =
        send(&t.app, "PATCH", "/api/auth/me/pin", Some(&token), Some(json!({ "pin": "1234" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["hasPin"], json!(true));

    // Verify: correct -> 204, wrong -> 401.
    let (status, _) =
        send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "1234" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) =
        send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "0000" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Rotating a set PIN without the current one is rejected.
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me/pin", Some(&token), Some(json!({ "pin": "5678" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // With the correct current it rotates.
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/auth/me/pin",
        Some(&token),
        Some(json!({ "pin": "5678", "current": "1234" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Clearing needs the current PIN; then hasPin flips off.
    let (status, body) = send(
        &t.app,
        "DELETE",
        "/api/auth/me/pin",
        Some(&token),
        Some(json!({ "current": "5678" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["hasPin"], json!(false));

    // With no PIN set, verify is a permissive 204 (nothing to gate).
    let (status, _) =
        send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "9999" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn pin_verify_locks_out_after_five_wrong_tries() {
    let t = test_app();
    let (_uid, token) = seed_session(&t.state, "pinlock@test.dev", "pinlock", &[Permission::Playback]);
    send(&t.app, "PATCH", "/api/auth/me/pin", Some(&token), Some(json!({ "pin": "1234" }))).await;

    // Four wrong tries are plain 401s.
    for _ in 0..4 {
        let (status, _) =
            send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "0000" }))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    // The fifth trips the fixed cooldown -> 429 with a retryAfter.
    let (status, body) =
        send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "0000" }))).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["retryAfter"].as_i64().unwrap_or(0) > 0);

    // While locked, even the correct PIN is refused with 429.
    let (status, _) =
        send(&t.app, "POST", "/api/auth/pin/verify", Some(&token), Some(json!({ "pin": "1234" }))).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
