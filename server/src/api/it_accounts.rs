//! Integration tests for the self-service account surface (`src/api/accounts.rs`):
//! password change, logout / session revoke, playback-language preferences,
//! uniqueness checks, and the DB-only slice of Quick Connect. Every branch here
//! is reachable without an image encoder, WebAuthn, a second device, or the
//! network. The profile-PIN handlers are covered in `it_pin`.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{get, seed_session, seed_session_pw, send, test_app};
use crate::model::Permission;

// ----- password change --------------------------------------------------------

#[tokio::test]
async fn change_password_succeeds_with_the_correct_current() {
    let t = test_app();
    let (_uid, token) =
        seed_session_pw(&t.state, "pw-ok@test.dev", "pwok", "hunter2", &[Permission::Playback]);
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/auth/me/password",
        Some(&token),
        Some(json!({ "current": "hunter2", "next": "hunter3" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The rotation took: the old password no longer verifies, the new one does.
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/auth/me/password",
        Some(&token),
        Some(json!({ "current": "hunter2", "next": "again" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "old password should be dead");
}

#[tokio::test]
async fn change_password_rejects_a_wrong_current() {
    let t = test_app();
    let (_uid, token) =
        seed_session_pw(&t.state, "pw-bad@test.dev", "pwbad", "correct", &[Permission::Playback]);
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/auth/me/password",
        Some(&token),
        Some(json!({ "current": "nope", "next": "whatever" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_password_rejects_a_too_short_new_password() {
    let t = test_app();
    let (_uid, token) =
        seed_session_pw(&t.state, "pw-short@test.dev", "pwshort", "correct", &[Permission::Playback]);
    let (status, _) = send(
        &t.app,
        "PATCH",
        "/api/auth/me/password",
        Some(&token),
        Some(json!({ "current": "correct", "next": "no" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ----- logout + session revoke ------------------------------------------------

#[tokio::test]
async fn logout_revokes_the_current_session() {
    let t = test_app();
    let (_uid, token) = seed_session(&t.state, "bye@test.dev", "bye", &[Permission::Playback]);
    // Valid session before.
    let (status, _) = get(&t.app, "/api/auth/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(&t.app, "POST", "/api/auth/logout", Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The bearer's session was deleted -> the next authed read is 401.
    let (status, _) = get(&t.app, "/api/auth/me", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_session_by_id_then_404_on_unknown() {
    let t = test_app();

    // A bogus device id is not one of the caller's devices -> 404 (session live).
    let (status, _) =
        send(&t.app, "DELETE", "/api/auth/me/sessions/ghost-device", Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The real device id from the session list revokes cleanly.
    let (_, sessions) = get(&t.app, "/api/auth/me/sessions", Some(&t.token)).await;
    let id = sessions[0]["id"].as_str().expect("device id").to_string();
    let (status, _) =
        send(&t.app, "DELETE", &format!("/api/auth/me/sessions/{id}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

// ----- profile / playback-language preferences --------------------------------

#[tokio::test]
async fn ui_language_sets_then_clears() {
    let t = test_app();
    let (_uid, token) = seed_session(&t.state, "lang@test.dev", "lang", &[Permission::Playback]);

    // A known tag is stored.
    let (status, body) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "language": "fr" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["language"], json!("fr"));

    // An explicit null clears it (the field is omitted, not null, once unset).
    let (status, body) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "language": null }))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["user"].get("language").is_none(), "cleared language should be absent");

    // An unknown/garbage tag is treated as a clear (normalize returns None).
    let (_, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "language": "fr" }))).await;
    let (status, body) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "language": "zz-XX" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["user"].get("language").is_none(), "unknown tag falls back to none");
}

#[tokio::test]
async fn audio_and_subtitle_languages_round_trip() {
    let t = test_app();
    let (_uid, token) = seed_session(&t.state, "media-lang@test.dev", "medialang", &[Permission::Playback]);

    let (status, body) = send(
        &t.app,
        "PATCH",
        "/api/auth/me",
        Some(&token),
        Some(json!({ "audioLanguage": "JA", "subtitleLanguage": "off" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Free-form ISO codes are lower-cased; "off" is the keep-subs-off sentinel.
    assert_eq!(body["user"]["audioLanguage"], json!("ja"));
    assert_eq!(body["user"]["subtitleLanguage"], json!("off"));

    // An empty string clears a media-language preference.
    let (status, body) = send(
        &t.app,
        "PATCH",
        "/api/auth/me",
        Some(&token),
        Some(json!({ "audioLanguage": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["user"].get("audioLanguage").is_none(), "empty audio lang clears it");
    // The untouched subtitle preference persists.
    assert_eq!(body["user"]["subtitleLanguage"], json!("off"));
}

// ----- uniqueness guards ------------------------------------------------------

#[tokio::test]
async fn patch_me_rejects_a_taken_username_and_email() {
    let t = test_app();
    seed_session(&t.state, "occupied@test.dev", "occupied", &[Permission::Playback]);
    let (_uid, token) = seed_session(&t.state, "mover@test.dev", "mover", &[Permission::Playback]);

    // Colliding with another account's username -> 409.
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "username": "occupied" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Colliding with another account's email -> 409.
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "email": "occupied@test.dev" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A malformed email is a 400 before any uniqueness lookup.
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "email": "not-an-email" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Keeping one's own email is a no-op success (not a self-collision).
    let (status, _) =
        send(&t.app, "PATCH", "/api/auth/me", Some(&token), Some(json!({ "email": "mover@test.dev" }))).await;
    assert_eq!(status, StatusCode::OK);
}

// ----- quick connect (DB-only slice) ------------------------------------------

#[tokio::test]
async fn quick_connect_initiate_then_poll_states() {
    let t = test_app();

    let (status, init) = send(&t.app, "POST", "/api/auth/quickconnect/initiate", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let secret = init["secret"].as_str().expect("secret").to_string();
    assert!(!init["code"].as_str().unwrap_or_default().is_empty());
    // No web_url configured in the test config -> no authorize URL for the QR.
    assert!(init["authorizeUrl"].is_null());

    // The freshly-issued code has not been approved yet.
    let (status, poll) =
        get(&t.app, &format!("/api/auth/quickconnect/poll?secret={secret}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(poll["status"], json!("pending"));

    // An unknown secret reads as expired (the device should restart the flow).
    let (status, poll) =
        get(&t.app, "/api/auth/quickconnect/poll?secret=nope", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(poll["status"], json!("expired"));
}
