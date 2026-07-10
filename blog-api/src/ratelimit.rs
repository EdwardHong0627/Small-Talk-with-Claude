//! Client-IP resolution and a simple in-memory, fixed-window rate limiter.
//!
//! The service only ever accepts TCP connections from Caddy on loopback, so
//! the raw socket peer address is always `127.0.0.1`. The *real* client IP
//! is carried in the `X-Forwarded-For` header set by Caddy. We only trust
//! that header when the socket peer is loopback; for any other (direct,
//! non-proxied) peer we fall back to the socket peer address itself, so a
//! random client can't spoof its IP by setting the header directly.

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::AppState;

/// The resolved client IP for the current request, inserted into request
/// extensions by [`client_ip_middleware`].
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

/// Resolve the client IP for a request given its headers and (optionally)
/// the socket peer address.
///
/// - If `peer_addr` is `None` (e.g. no `ConnectInfo` available, as in unit
///   tests) or is a loopback address, the `X-Forwarded-For` header is
///   trusted (first entry, comma-separated) when present and non-empty.
/// - Otherwise the socket peer address is used.
pub fn client_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> String {
    let trust_forwarded = peer_addr.map(|a| a.ip().is_loopback()).unwrap_or(true);

    if trust_forwarded {
        if let Some(value) = headers.get("x-forwarded-for") {
            if let Ok(s) = value.to_str() {
                let first = s.split(',').next().unwrap_or("").trim();
                if !first.is_empty() {
                    return first.to_string();
                }
            }
        }
    }

    peer_addr
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Middleware that resolves the client IP (see [`client_ip`]) and inserts a
/// [`ClientIp`] extension for downstream middleware/handlers to use.
pub async fn client_ip_middleware(mut req: Request<Body>, next: Next) -> Response {
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let ip = client_ip(req.headers(), peer);
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

struct Window {
    count: u32,
    window_start: Instant,
}

/// A hand-rolled, in-memory, fixed-window rate limiter keyed by an arbitrary
/// `(key, route)` pair. Resets on process restart, which is acceptable for
/// this service's threat model.
pub struct RateLimiter {
    windows: Mutex<HashMap<(String, String), Window>>,
    limit: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        RateLimiter {
            windows: Mutex::new(HashMap::new()),
            limit,
            window,
        }
    }

    /// Record a request for `(key, route)` and return `true` if it is
    /// allowed under the current window, `false` if the limit was exceeded.
    pub fn check(&self, key: &str, route: &str) -> bool {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();
        let entry = windows
            .entry((key.to_string(), route.to_string()))
            .or_insert_with(|| Window {
                count: 0,
                window_start: now,
            });

        if now.duration_since(entry.window_start) >= self.window {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= self.limit
    }
}

/// Middleware that enforces the per-(client_ip, route) rate limit. Must run
/// after [`client_ip_middleware`] so that the `ClientIp` extension is
/// available.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = req
        .extensions()
        .get::<ClientIp>()
        .map(|c| c.0.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let route = req.uri().path().to_string();

    if !state.rate_limiter.check(&ip, &route) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate limit exceeded"})),
        )
            .into_response();
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn trusts_forwarded_header_when_peer_is_loopback() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        let peer: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        assert_eq!(client_ip(&headers, Some(peer)), "1.2.3.4");
    }

    #[test]
    fn falls_back_to_peer_when_not_loopback() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        let peer: SocketAddr = "9.9.9.9:9999".parse().unwrap();
        assert_eq!(client_ip(&headers, Some(peer)), "9.9.9.9");
    }

    #[test]
    fn no_peer_defaults_to_trusting_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("5.6.7.8"));
        assert_eq!(client_ip(&headers, None), "5.6.7.8");
    }

    #[test]
    fn rate_limiter_trips_after_limit() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("1.1.1.1", "/api/comments"));
        assert!(limiter.check("1.1.1.1", "/api/comments"));
        assert!(!limiter.check("1.1.1.1", "/api/comments"));
    }

    #[test]
    fn rate_limiter_keys_by_ip_and_route() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("1.1.1.1", "/api/comments"));
        assert!(!limiter.check("1.1.1.1", "/api/comments"));
        // Different route, same ip -> allowed.
        assert!(limiter.check("1.1.1.1", "/api/contact"));
        // Different ip, same route -> allowed.
        assert!(limiter.check("2.2.2.2", "/api/comments"));
    }
}
