//! Integration tests for the problem-report flow: users file reports
//! (`/api/reports`), admins triage them (`/api/admin/reports`). Driven through the
//! real router over the demo catalogue, so a report can target a real movie/show.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{demo_item_id, demo_show_id, get, seed_session, send, test_app};
use crate::model::Permission;

fn member(t: &crate::api::test_support::TestApp, tag: &str) -> String {
    let (_id, token) = seed_session(&t.state, &format!("{tag}@test.dev"), tag, &[Permission::Playback]);
    token
}

#[tokio::test]
async fn any_user_can_file_a_report_and_the_title_is_resolved() {
    let t = test_app();
    let m = member(&t, "reporter");
    let movie = demo_item_id("The Matrix");

    let (status, body) = send(
        &t.app,
        "POST",
        "/api/reports",
        Some(&m),
        Some(json!({ "subjectKind": "movie", "subjectId": movie, "category": "audio", "message": "  no sound  " })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // The server resolves + snapshots the real title and trims the message.
    assert_eq!(body["subjectTitle"], json!("The Matrix"));
    assert_eq!(body["category"], json!("audio"));
    assert_eq!(body["status"], json!("open"));
    assert_eq!(body["message"], json!("no sound"));

    // The reporter sees it in their own list.
    let (status, mine) = get(&t.app, "/api/reports/mine", Some(&m)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine.as_array().unwrap().len(), 1);
    assert_eq!(mine[0]["reportedByName"], json!("reporter"));
}

#[tokio::test]
async fn report_on_unknown_subject_is_404() {
    let t = test_app();
    let m = member(&t, "ghostreporter");
    let (status, _) = send(
        &t.app,
        "POST",
        "/api/reports",
        Some(&m),
        Some(json!({ "subjectKind": "movie", "subjectId": "does-not-exist", "category": "video" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_show_report_resolves_the_show_title() {
    let t = test_app();
    let m = member(&t, "showreporter");
    let show = demo_show_id("The Office");
    let (status, body) = send(
        &t.app,
        "POST",
        "/api/reports",
        Some(&m),
        Some(json!({ "subjectKind": "show", "subjectId": show, "category": "metadata" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["subjectTitle"], json!("The Office"));
    // No message is fine (optional).
    assert_eq!(body["message"], json!(null));
}

#[tokio::test]
async fn triage_queue_requires_reports_manage() {
    let t = test_app();
    let m = member(&t, "peon");
    // A plain member can't reach the admin queue or its actions.
    let (status, _) = get(&t.app, "/api/admin/reports", Some(&m)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&t.app, "POST", "/api/admin/reports/x/resolve", Some(&m), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The owner (all permissions) can.
    let (status, body) = get(&t.app, "/api/admin/reports", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["reports"].is_array());
    assert!(body["counts"].is_object());
}

#[tokio::test]
async fn resolve_dismiss_reopen_and_delete_transitions() {
    let t = test_app();
    let m = member(&t, "flag");
    let movie = demo_item_id("The Matrix");
    let (_, created) = send(
        &t.app,
        "POST",
        "/api/reports",
        Some(&m),
        Some(json!({ "subjectKind": "movie", "subjectId": movie, "category": "video" })),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    // Resolve records the acting admin.
    let (status, body) =
        send(&t.app, "POST", &format!("/api/admin/reports/{id}/resolve"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("resolved"));
    assert_eq!(body["resolvedBy"], json!(t.user_id));

    // Reopen clears the resolver fields.
    let (status, body) =
        send(&t.app, "POST", &format!("/api/admin/reports/{id}/reopen"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("open"));
    assert_eq!(body["resolvedBy"], json!(null));

    // Dismiss, then delete.
    let (status, body) =
        send(&t.app, "POST", &format!("/api/admin/reports/{id}/dismiss"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("dismissed"));

    let (status, _) =
        send(&t.app, "DELETE", &format!("/api/admin/reports/{id}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // A second delete is a 404.
    let (status, _) =
        send(&t.app, "DELETE", &format!("/api/admin/reports/{id}"), Some(&t.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn queue_filters_by_status_and_category() {
    let t = test_app();
    let m = member(&t, "filterer");
    let movie = demo_item_id("The Matrix");
    // File two reports of different categories.
    for category in ["audio", "video"] {
        send(
            &t.app,
            "POST",
            "/api/reports",
            Some(&m),
            Some(json!({ "subjectKind": "movie", "subjectId": movie, "category": category })),
        )
        .await;
    }

    // Counts reflect the whole queue regardless of the active filter.
    let (_, body) = get(&t.app, "/api/admin/reports?category=audio", Some(&t.token)).await;
    assert_eq!(body["counts"]["total"], json!(2));
    assert_eq!(body["counts"]["open"], json!(2));
    let list = body["reports"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["category"], json!("audio"));

    // An open-status filter still returns both; a resolved filter returns none yet.
    let (_, body) = get(&t.app, "/api/admin/reports?status=open", Some(&t.token)).await;
    assert_eq!(body["reports"].as_array().unwrap().len(), 2);
    let (_, body) = get(&t.app, "/api/admin/reports?status=resolved", Some(&t.token)).await;
    assert_eq!(body["reports"].as_array().unwrap().len(), 0);
}
