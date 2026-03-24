#![allow(dead_code)]
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

pub async fn find_by_origin_id(pool: &SqlitePool, origin_id: &str) -> Result<Option<Message>> {
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

/// Return up to `limit` messages in a conversation whose timestamp is strictly
/// before `before_ts`, ordered newest-first. Used for MAM backward pagination.
pub async fn find_before(
    pool: &SqlitePool,
    conversation_jid: &str,
    before_ts: i64,
    limit: i64,
) -> Result<Vec<Message>> {
    let rows = sqlx::query(
        r#"
        SELECT id, conversation_jid, from_jid, body, timestamp,
               stanza_id, origin_id, state, edited_body, retracted
        FROM messages
        WHERE conversation_jid = ? AND timestamp < ?
        ORDER BY timestamp DESC
        LIMIT ?
        "#,
    )
    .bind(conversation_jid)
    .bind(before_ts)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_message).collect())
}

/// Count non-retracted messages that arrived after the message identified by
/// `last_read_id`. Returns 0 when `last_read_id` does not exist in the table.
pub async fn count_unread(
    pool: &SqlitePool,
    conversation_jid: &str,
    last_read_id: &str,
) -> Result<i64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt FROM messages m
        JOIN messages r ON r.id = ?
        WHERE m.conversation_jid = ?
          AND m.timestamp > r.timestamp
          AND m.retracted = 0
        "#,
    )
    .bind(last_read_id)
    .bind(conversation_jid)
    .fetch_one(pool)
    .await?;

    Ok(row.get::<i64, _>("cnt"))
}

/// Mark a message as retracted (soft-delete). The row is kept so that
/// thread continuity is preserved in the UI.
pub async fn mark_retracted(pool: &SqlitePool, message_id: &str) -> Result<()> {
    sqlx::query("UPDATE messages SET retracted = 1 WHERE id = ?")
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Overwrite the body of a message via its origin_id (XEP-0308 correction).
pub async fn update_body(pool: &SqlitePool, origin_id: &str, new_body: &str) -> Result<()> {
    sqlx::query("UPDATE messages SET edited_body = ? WHERE origin_id = ?")
        .bind(new_body)
        .bind(origin_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// M6: Delete all messages from the database (used by "Clear chat history" in settings).
pub async fn clear_all(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM messages")
        .execute(pool)
        .await?;
    Ok(())
}
