mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

async fn seed_pending_comment(app: &axum::Router, slug: &str) -> i64 {
    let (status, _) = send(
        app,
        json_post(
            "/api/comments",
            json!({"slug": slug, "author": "Alice", "body": "hi", "hp": ""}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, pending) = send(app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    pending.as_array().unwrap().last().unwrap()["id"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn admin_routes_require_bearer_token() {
    let app = build_test_app();
    seed_pending_comment(&app, "slug-a").await;

    // No auth header.
    let (status, _) = send(&app, get("/api/admin/comments/pending")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, post_auth("/api/admin/comments/1/approve", "bad")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, delete_auth("/api/admin/comments/1", "bad")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, get("/api/admin/contact")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_can_list_approve_and_delete_comments() {
    let app = build_test_app();
    let id = seed_pending_comment(&app, "slug-b").await;

    let (status, pending) = send(&app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 1);

    let (status, _) = send(
        &app,
        post_auth(&format!("/api/admin/comments/{id}/approve"), ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, pending) = send(&app, get_auth("/api/admin/comments/pending", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 0);

    let (status, approved) = send(&app, get("/api/comments?slug=slug-b")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved.as_array().unwrap().len(), 1);

    let (status, _) = send(
        &app,
        delete_auth(&format!("/api/admin/comments/{id}"), ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, approved) = send(&app, get("/api/comments?slug=slug-b")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn approving_unknown_comment_returns_404() {
    let app = build_test_app();
    let (status, _) = send(
        &app,
        post_auth("/api/admin/comments/99999/approve", ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
