use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

use crate::ratelimit::ClientIp;
use crate::AppState;

use super::{bad_request, validate_len};

/// Per (ip, slug, kind) daily cap: at most this many reactions of the same
/// kind, on the same post, from the same client IP, per calendar day.
const DAILY_CAP_PER_KIND: i64 = 1;

#[derive(Debug, Deserialize)]
pub struct NewReaction {
    pub slug: String,
    pub kind: String,
    pub client_id: String,
}

/// `GET /api/reactions?slug=...` — returns counts per kind, e.g.
/// `{"like": 3, "clap": 1}`.
pub async fn list_reactions(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let slug = match params.get("slug") {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("slug query parameter is required"),
    };

    let conn = state.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT kind, COUNT(*) FROM reactions WHERE slug = ?1 GROUP BY kind")
        .unwrap();
    let rows = stmt
        .query_map([&slug], |row| {
            let kind: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((kind, count))
        })
        .unwrap();

    let mut counts: Map<String, Value> = Map::new();
    for row in rows.filter_map(Result::ok) {
        counts.insert(row.0, json!(row.1));
    }

    Json(Value::Object(counts)).into_response()
}

/// `POST /api/reactions` — record a reaction, subject to a per-(ip, slug,
/// kind) daily cap.
pub async fn create_reaction(
    State(state): State<AppState>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Json(payload): Json<NewReaction>,
) -> Response {
    if payload.slug.trim().is_empty() {
        return bad_request("slug is required");
    }
    if let Err(e) = validate_len("kind", &payload.kind, 1, 40) {
        return e;
    }
    if let Err(e) = validate_len("client_id", &payload.client_id, 1, 120) {
        return e;
    }

    let conn = state.conn.lock().unwrap();

    let today_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reactions
             WHERE ip = ?1 AND slug = ?2 AND kind = ?3
               AND date(created_at) = date('now')",
            rusqlite::params![ip, payload.slug.trim(), payload.kind.trim()],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if today_count >= DAILY_CAP_PER_KIND {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "daily reaction limit reached for this post"})),
        )
            .into_response();
    }

    conn.execute(
        "INSERT INTO reactions (slug, kind, client_id, ip) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            payload.slug.trim(),
            payload.kind.trim(),
            payload.client_id.trim(),
            ip
        ],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT kind, COUNT(*) FROM reactions WHERE slug = ?1 GROUP BY kind")
        .unwrap();
    let rows = stmt
        .query_map([payload.slug.trim()], |row| {
            let kind: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((kind, count))
        })
        .unwrap();
    let mut counts: Map<String, Value> = Map::new();
    for row in rows.filter_map(Result::ok) {
        counts.insert(row.0, json!(row.1));
    }

    (StatusCode::OK, Json(Value::Object(counts))).into_response()
}
