//! Application error type shared by all route handlers.
//!
//! Handlers return `Result<Response, AppError>` so that fallible database
//! calls can use `?` instead of `.unwrap()`ing (and panicking under load).
//! The error's [`IntoResponse`] impl logs the underlying cause and returns a
//! generic `500` to the client -- internal details are never leaked in the
//! response body.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Opaque application error. Currently the only source is a database
/// failure, but this can grow additional variants without changing the
/// public `IntoResponse` behavior (always a generic `500`).
#[derive(Debug)]
pub enum AppError {
    /// A `rusqlite` call failed (e.g. the connection or a table is gone).
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Db(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::Db(err) => {
                tracing::error!(error = %err, "database error handling request");
            }
        }

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal error"})),
        )
            .into_response()
    }
}
