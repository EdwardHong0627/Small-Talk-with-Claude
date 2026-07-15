pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod ratelimit;
pub mod routes;

use rusqlite::Connection;
use std::sync::{Arc, Mutex, MutexGuard};

pub use error::AppError;
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

impl AppState {
    /// Lock the shared database connection, recovering from mutex
    /// poisoning (e.g. a prior panic while the lock was held) instead of
    /// propagating a panic to every subsequent caller.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Build the full axum [`Router`] for the service, wired up with all
/// middleware (client-ip resolution, rate limiting, admin auth).
pub fn build_app(state: AppState) -> axum::Router {
    routes::build_router(state)
}
