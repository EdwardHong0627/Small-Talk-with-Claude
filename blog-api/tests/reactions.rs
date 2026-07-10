mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn reaction_counts_and_daily_cap() {
    let app = build_test_app();

    // First reaction from this IP for (slug, kind) succeeds.
    let (status, body) = send(
        &app,
        json_post_with_ip(
            "/api/reactions",
            json!({"slug": "hello-world", "kind": "like", "client_id": "client-a"}),
            "10.0.0.1",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["like"], 1);

    // Same ip, same slug, same kind, again today -> rejected by daily cap.
    let (status, _) = send(
        &app,
        json_post_with_ip(
            "/api/reactions",
            json!({"slug": "hello-world", "kind": "like", "client_id": "client-a"}),
            "10.0.0.1",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // A different IP reacting with the same kind still succeeds.
    let (status, body) = send(
        &app,
        json_post_with_ip(
            "/api/reactions",
            json!({"slug": "hello-world", "kind": "like", "client_id": "client-b"}),
            "10.0.0.2",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["like"], 2);

    // A different kind from the same original IP also succeeds (cap is
    // per (ip, slug, kind), not just per ip/slug).
    let (status, body) = send(
        &app,
        json_post_with_ip(
            "/api/reactions",
            json!({"slug": "hello-world", "kind": "clap", "client_id": "client-a"}),
            "10.0.0.1",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["clap"], 1);

    // GET reflects the final counts.
    let (status, body) = send(&app, get("/api/reactions?slug=hello-world")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["like"], 2);
    assert_eq!(body["clap"], 1);
}

#[tokio::test]
async fn rejects_missing_fields() {
    let app = build_test_app();
    let (status, _) = send(
        &app,
        json_post_with_ip(
            "/api/reactions",
            json!({"slug": "", "kind": "like", "client_id": "c"}),
            "10.0.0.5",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
