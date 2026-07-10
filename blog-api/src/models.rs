//! Row structs and response DTOs.

use rusqlite::Row;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub id: i64,
    pub slug: String,
    pub author: String,
    pub body: String,
    pub status: String,
    pub created_at: String,
}

impl Comment {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Comment {
            id: row.get(0)?,
            slug: row.get(1)?,
            author: row.get(2)?,
            body: row.get(3)?,
            status: row.get(4)?,
            created_at: row.get(5)?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Reaction {
    pub id: i64,
    pub slug: String,
    pub kind: String,
    pub client_id: String,
    pub ip: String,
    pub created_at: String,
}

impl Reaction {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Reaction {
            id: row.get(0)?,
            slug: row.get(1)?,
            kind: row.get(2)?,
            client_id: row.get(3)?,
            ip: row.get(4)?,
            created_at: row.get(5)?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactMessage {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub message: String,
    pub created_at: String,
}

impl ContactMessage {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(ContactMessage {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            message: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
}
