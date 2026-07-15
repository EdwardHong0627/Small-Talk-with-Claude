use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::models::{Comment, ContactMessage};
use crate::{AppError, AppState};

/// `GET /api/admin/comments/pending` — list all pending comments across all
/// slugs, oldest first.
pub async fn pending_comments(State(state): State<AppState>) -> Result<Response, AppError> {
    let conn = state.conn();
    let mut stmt = conn.prepare(
        "SELECT id, slug, author, body, status, created_at
         FROM comments
         WHERE status = 'pending'
         ORDER BY created_at ASC, id ASC",
    )?;
    let comments: Vec<Comment> = stmt
        .query_map([], Comment::from_row)?
        .filter_map(Result::ok)
        .collect();

    Ok(Json(comments).into_response())
}

/// `POST /api/admin/comments/:id/approve` — mark a comment approved.
pub async fn approve_comment(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let conn = state.conn();
    let updated = conn.execute(
        "UPDATE comments SET status = 'approved' WHERE id = ?1",
        [id],
    )?;

    Ok(if updated == 0 {
        StatusCode::NOT_FOUND.into_response()
    } else {
        StatusCode::OK.into_response()
    })
}

/// `DELETE /api/admin/comments/:id` — delete a comment (pending or
/// approved).
pub async fn delete_comment(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let conn = state.conn();
    let deleted = conn.execute("DELETE FROM comments WHERE id = ?1", [id])?;

    Ok(if deleted == 0 {
        StatusCode::NOT_FOUND.into_response()
    } else {
        StatusCode::OK.into_response()
    })
}

/// `GET /api/admin/contact` — list all contact messages, newest first.
pub async fn list_contact(State(state): State<AppState>) -> Result<Response, AppError> {
    let conn = state.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, email, message, created_at
         FROM contact_messages
         ORDER BY created_at DESC, id DESC",
    )?;
    let messages: Vec<ContactMessage> = stmt
        .query_map([], ContactMessage::from_row)?
        .filter_map(Result::ok)
        .collect();

    Ok(Json(messages).into_response())
}
