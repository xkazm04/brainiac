//! Memory feedback (migrations/0004): the retrieval loop's return channel.
//! Agents and operators record helpful / wrong / outdated verdicts; the
//! summary powers ranking signals and re-verification queues later.

use anyhow::Result;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub const VERDICTS: [&str; 3] = ["helpful", "wrong", "outdated"];

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    memory_id: Uuid,
    user_id: Uuid,
    verdict: &str,
    note: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_feedback (id, org_id, memory_id, user_id, verdict, note)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org_id)
    .bind(memory_id)
    .bind(user_id)
    .bind(verdict)
    .bind(note)
    .execute(conn)
    .await?;
    Ok(())
}

/// Verdict counts for one memory (RLS-scoped like every read).
pub async fn summary(conn: &mut PgConnection, memory_id: Uuid) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT verdict, count(*) AS n FROM memory_feedback
         WHERE memory_id = $1 GROUP BY verdict ORDER BY verdict",
    )
    .bind(memory_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<String, _>("verdict"), r.get::<i64, _>("n")))
        .collect())
}
