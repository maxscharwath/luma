//! Integration tests for the registration-invite lifecycle (`src/api/invites.rs`)
//! and the invite-gated `POST /auth/register` consume path (`src/api/accounts.rs`).
//! All DB-only: minting, listing, public validity check, redemption, and revoke.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_session, send, test_app};
use crate::model::Permission;

fn member(t: &crate::api::test_support::TestApp, tag: &str) -> String {
    let (_id, token) = seed_session(&t.state, &format!("{tag}@test.dev"), tag, &[Permission::Playback]);
    token
}

/// Mint an invite as the owner and return its token.
async fn mint_invite(t: &crate::api::test_support::TestApp, perms: serde_json::Value) -> String {
    let (status, body) =
        send(&t.app, "POST", "/api/invites", Some(&t.token), Some(json!({ "permissions": perms }))).await;
    assert_eq!(status, StatusCode::OK);
    body["token"].as_str().expect("invite token").to_string()
}

#[tokio::test]
async fn invite_create_list_check_and_delete() {
    let t = test_app();

    let token = mint_invite(&t, json!(["playback"])).await;

    // It shows up in the pending list.
    let (status, list) = get(&t.app, "/api/invites", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().iter().any(|i| i["token"] == json!(token)));

    // The public check validates it without a session (the invitee isn't a user).
    let (status, chk) = get(&t.app, &format!("/api/invites/{token}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(chk["valid"], json!(true));

    // Revoke it.
    let (status, _) = send(&t.app, "DELETE", &format!("/api/invites/{token}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Now the public check reports it invalid.
    let (_, chk) = get(&t.app, &format!("/api/invites/{token}"), None).await;
    assert_eq!(chk["valid"], json!(false));
}

#[tokio::test]
async fn register_consumes_an_invite_and_inherits_its_permissions() {
    let t = test_app();
    let token = mint_invite(&t, json!(["playback", "requests.create"])).await;

    let (status, body) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({
            "email": "joiner@test.dev",
            "username": "joiner",
            "password": "s3cret",
            "inviteToken": token,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], json!("joiner"));
    let perms = body["user"]["permissions"].as_array().expect("permissions");
    assert!(perms.iter().any(|p| p == "requests.create"), "inherits the invite's perms");
    assert!(body["token"].is_string() && body["accessToken"].is_string());

    // The invite is single-use: a second attempt with the same token is refused.
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({
            "email": "second@test.dev",
            "username": "second",
            "password": "s3cret",
            "inviteToken": token,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_requires_a_valid_invite_after_the_owner_exists() {
    let t = test_app();

    // No invite token -> registration is closed (invite-only) -> 403.
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": "noinvite@test.dev", "username": "noinvite", "password": "s3cret" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A garbage token -> 403.
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({
            "email": "badinvite@test.dev",
            "username": "badinvite",
            "password": "s3cret",
            "inviteToken": "not-a-real-invite",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_rejects_a_malformed_body_and_a_duplicate_email() {
    let t = test_app();

    // Invalid email (no '@') -> 400, before any invite is consulted.
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": "nope", "username": "x", "password": "s3cret" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The seeded owner already owns owner@test.dev; a fresh invite + that email
    // is a 409 (the email pre-check fires before the invite is burned).
    let token = mint_invite(&t, json!(["playback"])).await;
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({
            "email": "owner@test.dev",
            "username": "dupe",
            "password": "s3cret",
            "inviteToken": token,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn invite_management_requires_users_manage() {
    let t = test_app();
    let m = member(&t, "invite-member");
    let (status, _) =
        send(&t.app, "POST", "/api/invites", Some(&m), Some(json!({ "permissions": ["playback"] }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = get(&t.app, "/api/invites", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
