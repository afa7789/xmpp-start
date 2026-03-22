use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_jid: String,
    pub from_jid: String,
    pub body: Option<String>,
    /// Unix milliseconds.
    pub timestamp: i64,
    pub stanza_id: Option<String>,
    pub origin_id: Option<String>,
    pub state: String,
    pub edited_body: Option<String>,
    pub retracted: i64,
}

fn row_to_message(row: &sqlx::sqlite::SqliteRow) -> Message {
    Message {
        id: row.get("id"),
        conversation_jid: row.get("conversation_jid"),
        from_jid: row.get("from_jid"),
        body: row.get("body"),
        timestamp: row.get("timestamp"),
        stanza_id: row.get("stanza_id"),
        origin_id: row.get("origin_id"),
        state: row.get("state"),
        edited_body: row.get("edited_body"),
        retracted: row.get("retracted"),
    }
}

pub async fn insert(pool: &SqlitePool, msg: &Message) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO messages
            (id, conversation_jid, from_jid, body, timestamp,
             stanza_id, origin_id, state, edited_body, retracted)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&msg.id)
    .bind(&msg.conversation_jid)
    .bind(&msg.from_jid)
    .bind(&msg.body)
    .bind(msg.timestamp)
    .bind(&msg.stanza_id)
    .bind(&msg.origin_id)
    .bind(&msg.state)
    .bind(&msg.edited_body)
    .bind(msg.retracted)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_by_conversation(
    pool: &SqlitePool,
    conversation_jid: &str,
    limit: i64,
) -> Result<Vec<Message>> {
    let rows = sqlx::query(
        r#"
        SELECT id, conversation_jid, from_jid, body, timestamp,
               stanza_id, origin_id, state, edited_body, retracted
        FROM messages
        WHERE conversation_jid = ?
        ORDER BY timestamp ASC
        LIMIT ?
        "#,
    )
    .bind(conversation_jid)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_message).collect())
}

pub async fn find_by_origin_id(
    pool: &SqlitePool,
    origin_id: &str,
) -> Result<Option<Message>> {
    let row = sqlx::query(
        r#"
        SELECT id, conversation_jid, from_jid, body, timestamp,
               stanza_id, origin_id, state, edited_body, retracted
        FROM messages
        WHERE origin_id = ?
        "#,
    )
    .bind(origin_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_message))
}
