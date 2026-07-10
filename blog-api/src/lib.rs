pub mod auth;
pub mod config;
pub mod db;
pub mod models;
pub mod ratelimit;
pub mod routes;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub use ratelimit::RateLimiter;

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub conn: Arc<Mutex<Connection>>,
    pub admin_token: String,
    pub rate_limiter: Arc<RateLimiter>,
    /// When true, `POST /api/comments` publishes immediately (status
    /// `approved`) instead of holding the comment as `pending`.
    pub auto_approve: bool,
}

/// Build the full axum [`Router`] for the service, wired up with all
/// middleware (client-ip resolution, rate limiting, admin auth).
pub fn build_app(state: AppState) -> axum::Router {
    routes::build_router(state)
}
