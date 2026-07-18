//! Integration tests for the auth / accounts endpoints (`src/api/accounts.rs`).
//! Real requests through the wired router; asserts status + JSON shape.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_session, send, test_app};
use crate::model::Permission;

#[tokio::test]
async fn auth_config_reports_accounts_exist() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/auth/config", None).await;
    assert_eq!(status, StatusCode::OK);
    // The owner was seeded, so an account exists; the roster is private by default.
    assert_eq!(body["hasAccounts"], json!(true));
    assert_eq!(body["publicUserList"], json!(false));
}

#[tokio::test]
async fn me_requires_a_valid_bearer() {
    let t = test_app();

    let (status, _) = get(&t.app, "/api/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no token -> 401");

    let (status, _) = get(&t.app, "/api/auth/me", Some("not-a-real-token")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "unknown token -> 401");

    let (status, body) = get(&t.app, "/api/auth/me", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["id"], json!(t.user_id));
    assert_eq!(body["user"]["username"], json!("owner"));
}

#[tokio::test]
async fn patch_me_updates_the_display_name() {
    let t = test_app();
    let (status, body) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&t.token), Some(json!({ "username": "Renamed" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], json!("Renamed"));

    // The change persisted: a fresh read reflects it.
    let (_, me) = get(&t.app, "/api/auth/me", Some(&t.token)).await;
    assert_eq!(me["user"]["username"], json!("Renamed"));
}

#[tokio::test]
async fn patch_me_rejects_an_empty_username() {
    let t = test_app();
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&t.token), Some(json!({ "username": "   " }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sessions_list_flags_the_current_device() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/auth/me/sessions", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let sessions = body.as_array().expect("sessions array");
    // The owner session was minted from one access token -> exactly one device,
    // flagged as the caller's current one.
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["current"], json!(true));
    assert_eq!(sessions[0]["userAgent"], json!("integration-test"));
}

#[tokio::test]
async fn sessions_list_requires_auth() {
    let t = test_app();
    let (status, _) = get(&t.app, "/api/auth/me/sessions", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_roster_is_private_by_default_and_opens_with_the_setting() {
    let t = test_app();

    // Off by default: the roster is not enumerable (empty list, not every account).
    let (status, body) = get(&t.app, "/api/users", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0));

    // Enable it, then the seeded owner is listed (public fields only).
    t.state
        .settings
        .set_patch(&t.state.db, [("publicUserList".to_string(), json!(true))].into_iter().collect());
    let (status, body) = get(&t.app, "/api/users", None).await;
    assert_eq!(status, StatusCode::OK);
    let users = body.as_array().expect("users array");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["username"], json!("owner"));
    // The public projection never leaks the email.
    assert!(users[0].get("email").is_none());
}

#[tokio::test]
async fn logout_is_a_no_content_noop_without_a_token() {
    let t = test_app();
    let (status, _) = send(&t.app, "POST", "/api/auth/logout", None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_second_account_can_get_its_own_session() {
    let t = test_app();
    let (uid, token) = seed_session(&t.state, "member@test.dev", "member", &[Permission::Playback]);
    let (status, body) = get(&t.app, "/api/auth/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["id"], json!(uid));
    assert_ne!(uid, t.user_id);
}
