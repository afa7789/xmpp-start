#![allow(dead_code)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub jid: String,
    pub last_read_id: Option<String>,
    pub archived: i64,
    pub last_activity: Option<i64>,
    /// K3: 1 = notifications muted for this contact, 0 = notifications enabled.
    pub muted: i64,
}

fn row_to_conversation(row: &sqlx::sqlite::SqliteRow) -> Conversation {
    Conversation {
        jid: row.get("jid"),
        last_read_id: row.get("last_read_id"),
        archived: row.get("archived"),
        last_activity: row.get("last_activity"),
        muted: row.try_get("muted").unwrap_or(0),
    }
}

/// Insert the conversation if it does not already exist. A no-op when the JID
/// is already present so callers can call this freely on every incoming message.
pub async fn upsert(pool: &SqlitePool, jid: &str) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO conversations (jid, last_read_id, archived, last_activity) \
         VALUES (?, NULL, 0, NULL)",
    )
    .bind(jid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return all conversations ordered by last_activity descending (NULLs last).
pub async fn get_all(pool: &SqlitePool) -> Result<Vec<Conversation>> {
    let rows = sqlx::query(
        "SELECT jid, last_read_id, archived, last_activity, \
                COALESCE(muted, 0) AS muted \
         FROM conversations \
         ORDER BY last_activity DESC NULLS LAST",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_conversation).collect())
}

/// Persist the ID of the most-recently-read message for a conversation.
pub async fn mark_read(pool: &SqlitePool, jid: &str, message_id: &str) -> Result<()> {
    sqlx::query("UPDATE conversations SET last_read_id = ? WHERE jid = ?")
        .bind(message_id)
        .bind(jid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Archive or un-archive a conversation.
pub async fn set_archived(pool: &SqlitePool, jid: &str, archived: bool) -> Result<()> {
    sqlx::query("UPDATE conversations SET archived = ? WHERE jid = ?")
        .bind(archived as i64)
        .bind(jid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bump the last_activity timestamp (Unix milliseconds).
pub async fn update_last_activity(pool: &SqlitePool, jid: &str, ts: i64) -> Result<()> {
    sqlx::query("UPDATE conversations SET last_activity = ? WHERE jid = ?")
        .bind(ts)
        .bind(jid)
        .execute(pool)
        .await?;
    Ok(())
}

/// K3: Set or clear the muted flag for a conversation.
pub async fn set_muted(pool: &SqlitePool, jid: &str, muted: bool) -> Result<()> {
    sqlx::query("UPDATE conversations SET muted = ? WHERE jid = ?")
        .bind(muted as i64)
        .bind(jid)
        .execute(pool)
        .await?;
    Ok(())
}

/// K3: Return the muted JIDs as a set (for fast lookup in the notification path).
pub async fn get_muted_jids(pool: &SqlitePool) -> Result<std::collections::HashSet<String>> {
    let rows = sqlx::query("SELECT jid FROM conversations WHERE muted = 1")
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("jid")).collect())
}

/// M6: Delete all conversations from the database (used by "Clear chat history" in settings).
pub async fn clear_all(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM conversations")
        .execute(pool)
        .await?;
    Ok(())
}
