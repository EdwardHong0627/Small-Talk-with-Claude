//! Database setup and migrations.

use rusqlite::Connection;

/// Ordered list of embedded migrations: `(version, sql)`.
///
/// New migrations should be added here (and as a new file under
/// `migrations/`) with a strictly increasing version number. Migration SQL
/// must be safe to run against a database that may already have the schema
/// applied from a previous run (use `IF NOT EXISTS`, etc.) so that
/// `migrate()` is idempotent.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_init.sql"))];

/// Open a SQLite connection at `path` and enable WAL mode for better
/// concurrent read/write behavior.
pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

/// Open an in-memory SQLite connection. Useful for tests.
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    Connection::open_in_memory()
}

/// Apply any pending migrations. Safe to call on every startup.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
        );",
    )?;

    let current: i64 = conn
        .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let mut applied = current;
    for (version, sql) in MIGRATIONS {
        if *version > current {
            conn.execute_batch(sql)?;
            applied = *version;
        }
    }

    if applied != current {
        conn.execute(
            "INSERT INTO schema_version (id, version) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET version = excluded.version",
            [applied],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 1);

        // Tables should exist and be queryable.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM comments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
