mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn contact_message_stored_and_listed_by_admin() {
    let app = build_test_app();

    let (status, _) = send(
        &app,
        json_post(
            "/api/contact",
            json!({
                "name": "Alice",
                "email": "alice@example.com",
                "message": "Loved the article!",
                "hp": ""
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // No bearer token -> unauthorized.
    let (status, _) = send(&app, get("/api/admin/contact")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Wrong bearer token -> unauthorized.
    let (status, _) = send(&app, get_auth("/api/admin/contact", "wrong-token")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Correct bearer token -> lists the message.
    let (status, body) = send(&app, get_auth("/api/admin/contact", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let messages = body.as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["email"], "alice@example.com");
}

#[tokio::test]
async fn honeypot_filled_contact_is_silently_dropped() {
    let app = build_test_app();

    let (status, _) = send(
        &app,
        json_post(
            "/api/contact",
            json!({
                "name": "Bot",
                "email": "bot@example.com",
                "message": "spam",
                "hp": "gotcha"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(&app, get_auth("/api/admin/contact", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn empty_name_contact_accepted() {
    // The UI labels the name field "(optional)" — the API must accept an
    // empty name.
    let app = build_test_app();

    let (status, _) = send(
        &app,
        json_post(
            "/api/contact",
            json!({
                "name": "",
                "email": "alice@example.com",
                "message": "No name given.",
                "hp": ""
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(&app, get_auth("/api/admin/contact", ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let messages = body.as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["name"], "");
}

#[tokio::test]
async fn rejects_invalid_email() {
    let app = build_test_app();
    let (status, _) = send(
        &app,
        json_post(
            "/api/contact",
            json!({"name": "Alice", "email": "not-an-email", "message": "hi", "hp": ""}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
