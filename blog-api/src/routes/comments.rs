use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;

use crate::models::Comment;
use crate::AppState;

use super::{bad_request, validate_len};

#[derive(Debug, Deserialize)]
pub struct NewComment {
    pub slug: String,
    pub author: String,
    pub body: String,
    #[serde(default)]
    pub hp: String,
}

/// `GET /api/comments?slug=...` — returns only approved comments for the
/// given slug, oldest first.
pub async fn list_comments(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let slug = match params.get("slug") {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("slug query parameter is required"),
    };

    let conn = state.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, slug, author, body, status, created_at
             FROM comments
             WHERE slug = ?1 AND status = 'approved'
             ORDER BY created_at ASC, id ASC",
        )
        .unwrap();
    let comments: Vec<Comment> = stmt
        .query_map([&slug], Comment::from_row)
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    Json(comments).into_response()
}

/// `POST /api/comments` — create a new comment. Honeypot-filled
/// submissions are silently accepted without being persisted. The comment
/// is stored as `pending` (awaiting admin approval) unless the service is
/// configured with `BLOG_API_AUTO_APPROVE`, in which case it is `approved`
/// immediately.
pub async fn create_comment(
    State(state): State<AppState>,
    Json(payload): Json<NewComment>,
) -> Response {
    if !payload.hp.is_empty() {
        // Honeypot tripped: pretend success, drop silently.
        return StatusCode::OK.into_response();
    }

    if payload.slug.trim().is_empty() {
        return bad_request("slug is required");
    }
    // Author is optional (the UI renders an empty author as "anonymous").
    if let Err(e) = validate_len("author", &payload.author, 0, 80) {
        return e;
    }
    if let Err(e) = validate_len("body", &payload.body, 1, 5000) {
        return e;
    }

    let status = if state.auto_approve { "approved" } else { "pending" };

    let conn = state.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO comments (slug, author, body, status) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            payload.slug.trim(),
            payload.author.trim(),
            payload.body.trim(),
            status
        ],
    )
    .unwrap();

    StatusCode::OK.into_response()
}
