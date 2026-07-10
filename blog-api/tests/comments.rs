mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn pending_comment_not_visible_until_approved() {
    let app = build_test_app();

    let (status, _) = send(
        &app,
        json_post(
            "/api/comments",
            json!({
                "slug": "hello-world",
                "author": "Alice",
                "body": "Great post!",
                "hp": ""
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Not visible on the public endpoint yet (still pending).
    let (status, body) = send(&app, get("/api/comments?slug=hello-world")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Visible to admin as pending.
    let (status, pending) = send(&app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let pending = pending.as_array().unwrap();
    assert_eq!(pending.len(), 1);
    let id = pending[0]["id"].as_i64().unwrap();
    assert_eq!(pending[0]["status"], "pending");

    // Approve it.
    let (status, _) = send(
        &app,
        post_auth(&format!("/api/admin/comments/{id}/approve"), ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Now visible on the public endpoint.
    let (status, body) = send(&app, get("/api/comments?slug=hello-world")).await;
    assert_eq!(status, StatusCode::OK);
    let comments = body.as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["author"], "Alice");
    assert_eq!(comments[0]["status"], "approved");
}

#[tokio::test]
async fn auto_approve_comment_visible_immediately() {
    let app = build_test_app_auto_approve();

    let (status, _) = send(
        &app,
        json_post(
            "/api/comments",
            json!({
                "slug": "hello-world",
                "author": "Alice",
                "body": "No moderation queue here.",
                "hp": ""
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Visible on the public endpoint right away — no approval step.
    let (status, body) = send(&app, get("/api/comments?slug=hello-world")).await;
    assert_eq!(status, StatusCode::OK);
    let comments = body.as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["author"], "Alice");
    assert_eq!(comments[0]["status"], "approved");

    // Nothing sitting in the pending queue.
    let (status, pending) = send(&app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 0);

    // The honeypot still applies with auto-approve on.
    let (status, _) = send(
        &app,
        json_post(
            "/api/comments",
            json!({"slug": "hello-world", "author": "Bot", "body": "spam", "hp": "x"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(&app, get("/api/comments?slug=hello-world")).await;
    assert_eq!(body.as_array().unwrap().len(), 1, "honeypot submission must not persist");
}

#[tokio::test]
async fn honeypot_filled_submission_is_silently_dropped() {
    let app = build_test_app();

    let (status, _) = send(
        &app,
        json_post(
            "/api/comments",
            json!({
                "slug": "hello-world",
                "author": "Bot",
                "body": "buy cheap stuff",
                "hp": "i-am-a-bot"
            }),
        ),
    )
    .await;
    // Looks like success to the caller...
    assert_eq!(status, StatusCode::OK);

    // ...but nothing was persisted.
    let (status, pending) = send(&app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rejects_missing_slug_on_get() {
    let app = build_test_app();
    let (status, _) = send(&app, get("/api/comments")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_oversized_body() {
    let app = build_test_app();
    let long_body = "x".repeat(5001);
    let (status, _) = send(
        &app,
        json_post(
            "/api/comments",
            json!({"slug": "hello-world", "author": "Alice", "body": long_body, "hp": ""}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
