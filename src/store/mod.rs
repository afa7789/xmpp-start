pub mod message_repo;
pub mod roster_repo;

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

/// Thin wrapper around a `SqlitePool` with migrations applied on connect.
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    /// Open (or create) the SQLite database at `path` and run pending migrations.
    ///
    /// Pass `":memory:"` for an in-process test database.
    pub async fn connect(path: &str) -> Result<Self> {
        let url = if path == ":memory:" {
            "sqlite::memory:".to_owned()
        } else {
            format!("sqlite://{}?mode=rwc", path)
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::message_repo::{self, Message};

    #[tokio::test]
    async fn test_insert_and_query_message() {
        let db = Database::connect(":memory:")
            .await
            .expect("failed to open in-memory database");

        // The messages table has a FK on conversations(jid), so we need a
        // conversation row first.
        sqlx::query(
            "INSERT INTO conversations (jid, last_read_id, archived, last_activity) \
             VALUES (?, NULL, 0, NULL)",
        )
        .bind("alice@example.com")
        .execute(&db.pool)
        .await
        .expect("failed to insert conversation");

        let msg = Message {
            id: "msg-001".into(),
            conversation_jid: "alice@example.com".into(),
            from_jid: "bob@example.com".into(),
            body: Some("Hello, Alice!".into()),
            timestamp: 1_700_000_000_000,
            stanza_id: Some("s-001".into()),
            origin_id: Some("o-001".into()),
            state: "received".into(),
            edited_body: None,
            retracted: 0,
        };

        message_repo::insert(&db.pool, &msg)
            .await
            .expect("insert failed");

        // Query back by conversation.
        let results = message_repo::find_by_conversation(&db.pool, "alice@example.com", 10)
            .await
            .expect("find_by_conversation failed");

        assert_eq!(results.len(), 1);
        let got = &results[0];
        assert_eq!(got.id, "msg-001");
        assert_eq!(got.from_jid, "bob@example.com");
        assert_eq!(got.body.as_deref(), Some("Hello, Alice!"));
        assert_eq!(got.timestamp, 1_700_000_000_000);
        assert_eq!(got.origin_id.as_deref(), Some("o-001"));
        assert_eq!(got.state, "received");
        assert_eq!(got.retracted, 0);

        // Query by origin_id.
        let found = message_repo::find_by_origin_id(&db.pool, "o-001")
            .await
            .expect("find_by_origin_id failed")
            .expect("should find message by origin_id");
        assert_eq!(found.id, "msg-001");
    }
}
