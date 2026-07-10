use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::AppState;

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
) -> Response {
    if !payload.hp.is_empty() {
        return StatusCode::OK.into_response();
    }

    if let Err(e) = validate_len("name", &payload.name, 1, 80) {
        return e;
    }
    if let Err(e) = validate_len("email", &payload.email, 3, 254) {
        return e;
    }
    if !payload.email.contains('@') {
        return bad_request("email must be a valid email address");
    }
    if let Err(e) = validate_len("message", &payload.message, 1, 5000) {
        return e;
    }

    let conn = state.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO contact_messages (name, email, message) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            payload.name.trim(),
            payload.email.trim(),
            payload.message.trim()
        ],
    )
    .unwrap();

    StatusCode::OK.into_response()
}
