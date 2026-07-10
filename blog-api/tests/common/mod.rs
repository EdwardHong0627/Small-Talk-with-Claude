use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use blog_api::ratelimit::RateLimiter;
use blog_api::{db, AppState};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

pub const ADMIN_TOKEN: &str = "test-admin-token";

/// Rate limit high enough that ordinary tests won't accidentally trip it.
const GENEROUS_LIMIT: u32 = 1000;

#[allow(dead_code)]
pub fn build_test_app() -> Router {
    build_test_app_with_limit(GENEROUS_LIMIT, Duration::from_secs(60))
}

#[allow(dead_code)]
pub fn build_test_app_with_limit(limit: u32, window: Duration) -> Router {
    build_test_app_full(limit, window, false)
}

/// Build a test app with comment auto-approval (moderation off) enabled.
#[allow(dead_code)]
pub fn build_test_app_auto_approve() -> Router {
    build_test_app_full(GENEROUS_LIMIT, Duration::from_secs(60), true)
}

#[allow(dead_code)]
pub fn build_test_app_full(limit: u32, window: Duration, auto_approve: bool) -> Router {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    db::migrate(&conn).expect("migrate");
    let state = AppState {
        conn: Arc::new(Mutex::new(conn)),
        admin_token: ADMIN_TOKEN.to_string(),
        rate_limiter: Arc::new(RateLimiter::new(limit, window)),
        auto_approve,
    };
    blog_api::build_app(state)
}

#[allow(dead_code)]
pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.expect("request failed");
    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let value: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[allow(dead_code)]
pub fn json_post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[allow(dead_code)]
pub fn json_post_with_ip(uri: &str, body: Value, ip: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-forwarded-for", ip)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[allow(dead_code)]
pub fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

#[allow(dead_code)]
pub fn get_with_ip(uri: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .unwrap()
}

#[allow(dead_code)]
pub fn get_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[allow(dead_code)]
pub fn post_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[allow(dead_code)]
pub fn delete_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}
