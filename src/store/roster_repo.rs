use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterContact {
    pub jid: String,
    pub name: Option<String>,
    pub subscription: String,
    /// Stored as a JSON array string, e.g. `["Friends","Work"]`.
    pub groups: Option<String>,
}

fn row_to_contact(row: &sqlx::sqlite::SqliteRow) -> RosterContact {
    RosterContact {
        jid: row.get("jid"),
        name: row.get("name"),
        subscription: row.get("subscription"),
        groups: row.get("groups"),
    }
}

pub async fn upsert(pool: &SqlitePool, contact: &RosterContact) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO roster (jid, name, subscription, groups)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(jid) DO UPDATE SET
            name         = excluded.name,
            subscription = excluded.subscription,
            groups       = excluded.groups
        "#,
    )
    .bind(&contact.jid)
    .bind(&contact.name)
    .bind(&contact.subscription)
    .bind(&contact.groups)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_all(pool: &SqlitePool) -> Result<Vec<RosterContact>> {
    let rows = sqlx::query(
        r#"
        SELECT jid, name, subscription, groups
        FROM roster
        ORDER BY jid ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_contact).collect())
}
