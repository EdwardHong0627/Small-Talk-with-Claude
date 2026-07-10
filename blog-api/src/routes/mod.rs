pub mod admin;
pub mod comments;
pub mod contact;
pub mod reactions;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{middleware, Json, Router};
use serde_json::json;

use crate::{auth, ratelimit, AppState};

/// Build a `400 Bad Request` JSON error response.
pub(crate) fn bad_request(msg: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": msg.into()}))).into_response()
}

/// Validate that a trimmed string's length is within `[min, max]`
/// (inclusive) characters.
pub(crate) fn validate_len(
    field: &str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<(), Response> {
    let len = value.trim().chars().count();
    if len < min || len > max {
        return Err(bad_request(format!(
            "{field} must be between {min} and {max} characters"
        )));
    }
    Ok(())
}

/// Assemble the full application [`Router`].
pub fn build_router(state: AppState) -> Router {
    let admin_routes = Router::new()
        .route("/api/admin/comments/pending", get(admin::pending_comments))
        .route("/api/admin/comments/:id/approve", post(admin::approve_comment))
        .route("/api/admin/comments/:id", delete(admin::delete_comment))
        .route("/api/admin/contact", get(admin::list_contact))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth_middleware,
        ));

    let public_routes = Router::new()
        .route(
            "/api/comments",
            get(comments::list_comments).post(comments::create_comment),
        )
        .route(
            "/api/reactions",
            get(reactions::list_reactions).post(reactions::create_reaction),
        )
        .route("/api/contact", post(contact::create_contact));

    Router::new()
        .merge(public_routes)
        .merge(admin_routes)
        // Layers apply outer-to-inner in reverse registration order, i.e.
        // the LAST `.layer()` call runs FIRST for an incoming request. We
        // need `client_ip_middleware` to run before `rate_limit_middleware`
        // (which reads the `ClientIp` extension it sets), so it is added
        // last.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::rate_limit_middleware,
        ))
        .layer(middleware::from_fn(ratelimit::client_ip_middleware))
        .with_state(state)
}
