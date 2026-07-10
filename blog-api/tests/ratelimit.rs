mod common;

use axum::http::StatusCode;
use common::*;
use std::time::Duration;

#[tokio::test]
async fn rate_limit_is_keyed_by_forwarded_ip_and_route() {
    let limit = 3u32;
    let app = build_test_app_with_limit(limit, Duration::from_secs(60));

    // N requests from the same forwarded IP on the same route succeed.
    for _ in 0..limit {
        let (status, _) = send(
            &app,
            get_with_ip("/api/comments?slug=hello-world", "10.1.1.1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // The (N+1)th request from the same IP+route trips the limiter.
    let (status, _) = send(
        &app,
        get_with_ip("/api/comments?slug=hello-world", "10.1.1.1"),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // A different forwarded IP on the SAME route still succeeds: proves
    // the header (not the socket peer, which is identical/absent for all
    // of these requests) is what's used as the key.
    let (status, _) = send(
        &app,
        get_with_ip("/api/comments?slug=hello-world", "10.1.1.2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The SAME original IP on a DIFFERENT route still succeeds: proves
    // keying is (ip, route), not just ip.
    let (status, _) = send(
        &app,
        get_with_ip("/api/reactions?slug=hello-world", "10.1.1.1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
