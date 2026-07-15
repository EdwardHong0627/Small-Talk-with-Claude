use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::{AppError, AppState};

use super::{bad_request, validate_len};

#[derive(Debug, Deserialize)]
pub struct NewContactMessage {
    pub name: String,
    pub email: String,
    pub message: String,
    #[serde(default)]
    pub hp: String,
}

/// `POST /api/contact` — store a contact message. Not emailed anywhere;
/// admins read it via `GET /api/admin/contact`. Honeypot-filled submissions
/// are silently accepted without being persisted.
pub async fn create_contact(
    State(state): State<AppState>,
    Json(payload): Json<NewContactMessage>,
) -> Result<Response, AppError> {
    if !payload.hp.is_empty() {
        return Ok(StatusCode::OK.into_response());
    }

    // Name is optional, matching the "name (optional)" label in the UI.
    if let Err(e) = validate_len("name", &payload.name, 0, 80) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_len("email", &payload.email, 3, 254) {
        return Ok(bad_request(e));
    }
    if !payload.email.contains('@') {
        return Ok(bad_request("email must be a valid email address"));
    }
    if let Err(e) = validate_len("message", &payload.message, 1, 5000) {
        return Ok(bad_request(e));
    }

    let conn = state.conn();
    conn.execute(
        "INSERT INTO contact_messages (name, email, message) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            payload.name.trim(),
            payload.email.trim(),
            payload.message.trim()
        ],
    )?;

    Ok(StatusCode::OK.into_response())
}
