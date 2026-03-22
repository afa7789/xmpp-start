pub mod avatar_crop;
pub mod conversation_repo;
pub mod message_repo;
pub mod roster_repo;
pub mod thumbnail;

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
    use crate::store::{
        conversation_repo,
        message_repo::{self, Message},
    };

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

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Insert a conversation row and a message row for use in multiple tests.
    async fn seed_conversation(db: &Database, jid: &str, msg_id: &str, origin_id: &str, ts: i64) {
        conversation_repo::upsert(&db.pool, jid)
            .await
            .expect("upsert conversation failed");

        let msg = Message {
            id: msg_id.into(),
            conversation_jid: jid.into(),
            from_jid: "sender@example.com".into(),
            body: Some("seed body".into()),
            timestamp: ts,
            stanza_id: None,
            origin_id: Some(origin_id.into()),
            state: "received".into(),
            edited_body: None,
            retracted: 0,
        };
        message_repo::insert(&db.pool, &msg)
            .await
            .expect("insert message failed");
    }

    // -----------------------------------------------------------------------
    // conversation_repo tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_conversation_upsert_and_get_all() {
        let db = Database::connect(":memory:")
            .await
            .expect("open in-memory db");

        conversation_repo::upsert(&db.pool, "alice@example.com")
            .await
            .expect("first upsert failed");

        // Second call must be a no-op (INSERT OR IGNORE).
        conversation_repo::upsert(&db.pool, "alice@example.com")
            .await
            .expect("second upsert failed");

        conversation_repo::upsert(&db.pool, "bob@example.com")
            .await
            .expect("bob upsert failed");

        let all = conversation_repo::get_all(&db.pool)
            .await
            .expect("get_all failed");

        assert_eq!(all.len(), 2);
        let jids: Vec<&str> = all.iter().map(|c| c.jid.as_str()).collect();
        assert!(jids.contains(&"alice@example.com"));
        assert!(jids.contains(&"bob@example.com"));

        // Defaults must be set correctly.
        let alice = all.iter().find(|c| c.jid == "alice@example.com").unwrap();
        assert!(alice.last_read_id.is_none());
        assert_eq!(alice.archived, 0);
        assert!(alice.last_activity.is_none());
    }

    #[tokio::test]
    async fn test_mark_conversation_read() {
        let db = Database::connect(":memory:")
            .await
            .expect("open in-memory db");

        seed_conversation(&db, "carol@example.com", "m-1", "o-1", 1_000).await;

        conversation_repo::mark_read(&db.pool, "carol@example.com", "m-1")
            .await
            .expect("mark_read failed");

        let all = conversation_repo::get_all(&db.pool)
            .await
            .expect("get_all failed");
        let carol = all.iter().find(|c| c.jid == "carol@example.com").unwrap();
        assert_eq!(carol.last_read_id.as_deref(), Some("m-1"));

        // set_archived round-trip.
        conversation_repo::set_archived(&db.pool, "carol@example.com", true)
            .await
            .expect("set_archived failed");
        let all2 = conversation_repo::get_all(&db.pool).await.unwrap();
        let carol2 = all2.iter().find(|c| c.jid == "carol@example.com").unwrap();
        assert_eq!(carol2.archived, 1);

        conversation_repo::set_archived(&db.pool, "carol@example.com", false)
            .await
            .expect("unarchive failed");
        let all3 = conversation_repo::get_all(&db.pool).await.unwrap();
        let carol3 = all3.iter().find(|c| c.jid == "carol@example.com").unwrap();
        assert_eq!(carol3.archived, 0);

        // update_last_activity.
        conversation_repo::update_last_activity(&db.pool, "carol@example.com", 9_999_999)
            .await
            .expect("update_last_activity failed");
        let all4 = conversation_repo::get_all(&db.pool).await.unwrap();
        let carol4 = all4.iter().find(|c| c.jid == "carol@example.com").unwrap();
        assert_eq!(carol4.last_activity, Some(9_999_999));
    }

    // -----------------------------------------------------------------------
    // message_repo extension tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_find_before_pagination() {
        let db = Database::connect(":memory:")
            .await
            .expect("open in-memory db");

        let jid = "dave@example.com";
        conversation_repo::upsert(&db.pool, jid).await.unwrap();

        // Insert five messages with distinct timestamps.
        for i in 1..=5_i64 {
            let msg = Message {
                id: format!("pm-{}", i),
                conversation_jid: jid.into(),
                from_jid: "someone@example.com".into(),
                body: Some(format!("msg {}", i)),
                timestamp: i * 1_000,
                stanza_id: None,
                origin_id: Some(format!("po-{}", i)),
                state: "received".into(),
                edited_body: None,
                retracted: 0,
            };
            message_repo::insert(&db.pool, &msg).await.unwrap();
        }

        // Fetch messages before ts=4000, limit 2 — expect pm-3 and pm-2 (newest first).
        let page = message_repo::find_before(&db.pool, jid, 4_000, 2)
            .await
            .expect("find_before failed");

        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id, "pm-3");
        assert_eq!(page[1].id, "pm-2");

        // Fetch before the very first message — expect empty.
        let empty = message_repo::find_before(&db.pool, jid, 1_000, 10)
            .await
            .expect("find_before (empty) failed");
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_mark_retracted() {
        let db = Database::connect(":memory:")
            .await
            .expect("open in-memory db");

        seed_conversation(&db, "eve@example.com", "r-1", "ro-1", 5_000).await;

        // Before retraction retracted == 0.
        let before = message_repo::find_by_origin_id(&db.pool, "ro-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.retracted, 0);

        message_repo::mark_retracted(&db.pool, "r-1")
            .await
            .expect("mark_retracted failed");

        let after = message_repo::find_by_origin_id(&db.pool, "ro-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.retracted, 1);

        // count_unread must not count retracted messages.
        // Insert a second non-retracted message after the retracted one.
        let later = Message {
            id: "r-2".into(),
            conversation_jid: "eve@example.com".into(),
            from_jid: "sender@example.com".into(),
            body: Some("visible".into()),
            timestamp: 6_000,
            stanza_id: None,
            origin_id: Some("ro-2".into()),
            state: "received".into(),
            edited_body: None,
            retracted: 0,
        };
        message_repo::insert(&db.pool, &later).await.unwrap();

        // Unread after "r-1" (the retracted msg) = 1 (only r-2 is visible).
        let unread = message_repo::count_unread(&db.pool, "eve@example.com", "r-1")
            .await
            .expect("count_unread failed");
        assert_eq!(unread, 1);
    }

    #[tokio::test]
    async fn test_update_body_correction() {
        let db = Database::connect(":memory:")
            .await
            .expect("open in-memory db");

        seed_conversation(&db, "frank@example.com", "c-1", "co-1", 7_000).await;

        // edited_body starts NULL.
        let original = message_repo::find_by_origin_id(&db.pool, "co-1")
            .await
            .unwrap()
            .unwrap();
        assert!(original.edited_body.is_none());

        message_repo::update_body(&db.pool, "co-1", "corrected text")
            .await
            .expect("update_body failed");

        let corrected = message_repo::find_by_origin_id(&db.pool, "co-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(corrected.edited_body.as_deref(), Some("corrected text"));
        // Original body must be untouched.
        assert_eq!(corrected.body.as_deref(), Some("seed body"));
    }
}
