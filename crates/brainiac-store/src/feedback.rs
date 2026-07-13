//! Memory feedback (migrations/0004, 0005): the retrieval loop's return
//! channel and its triage side.
//!
//! Agents and operators record helpful / wrong / outdated verdicts on the
//! memories they were served. A `helpful` verdict asserts nothing to fix; a
//! `wrong` or `outdated` verdict is a **claim against the corpus** that stays
//! OPEN until a maintainer answers it — by re-verifying the memory,
//! deprecating it, or dismissing the report. [`flagged`] is that queue.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use uuid::Uuid;

pub const VERDICTS: [&str; 3] = ["helpful", "wrong", "outdated"];
/// Verdicts that assert something is wrong with the memory, so they open a
/// claim a maintainer must close.
pub const NEGATIVE_VERDICTS: [&str; 2] = ["wrong", "outdated"];
pub const RESOLUTIONS: [&str; 3] = ["reverified", "deprecated", "dismissed"];

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

/// Trust signal attached to a served memory: what readers reported back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trust {
    pub helpful: i64,
    pub wrong: i64,
    pub outdated: i64,
    /// Unresolved wrong/outdated claims — the "someone disputes this" flag.
    pub open_claims: i64,
}

impl Trust {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn disputed(&self) -> bool {
        self.open_claims > 0
    }
}

/// Batch trust lookup for a result set — one query for N memories, so
/// attaching trust to search results never becomes an N+1.
pub async fn trust_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, Trust>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT memory_id,
                count(*) FILTER (WHERE verdict = 'helpful')  AS helpful,
                count(*) FILTER (WHERE verdict = 'wrong')    AS wrong,
                count(*) FILTER (WHERE verdict = 'outdated') AS outdated,
                count(*) FILTER (WHERE resolved_at IS NULL
                                   AND verdict IN ('wrong', 'outdated')) AS open_claims
         FROM memory_feedback
         WHERE memory_id = ANY($1)
         GROUP BY memory_id",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("memory_id"),
                Trust {
                    helpful: r.get("helpful"),
                    wrong: r.get("wrong"),
                    outdated: r.get("outdated"),
                    open_claims: r.get("open_claims"),
                },
            )
        })
        .collect())
}

/// A memory with unresolved claims against it — one row per memory, not per
/// verdict, so the queue reads as "N memories need an answer".
#[derive(Debug, Clone)]
pub struct FlaggedMemory {
    pub memory_id: Uuid,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub team_id: Option<Uuid>,
    pub valid_to: Option<DateTime<Utc>>,
    pub wrong: i64,
    pub outdated: i64,
    /// Reporter notes on the open claims (most recent first, capped).
    pub notes: Vec<String>,
    /// Age of the OLDEST open claim — how long the dispute has stood.
    pub oldest_claim_secs: i64,
}

/// The triage queue: memories carrying unresolved wrong/outdated claims,
/// most-disputed first, then oldest. RLS-scoped — a maintainer only ever
/// sees claims against memories they can read.
pub async fn flagged(conn: &mut PgConnection, limit: i64) -> Result<Vec<FlaggedMemory>> {
    let rows = sqlx::query(
        "SELECT f.memory_id, m.content, m.kind, m.status::text AS status, m.team_id, m.valid_to,
                count(*) FILTER (WHERE f.verdict = 'wrong')    AS wrong,
                count(*) FILTER (WHERE f.verdict = 'outdated') AS outdated,
                COALESCE(
                    array_agg(f.note ORDER BY f.created_at DESC)
                        FILTER (WHERE f.note IS NOT NULL),
                    '{}'
                ) AS notes,
                EXTRACT(EPOCH FROM now() - min(f.created_at))::bigint AS oldest_claim_secs
         FROM memory_feedback f
         JOIN memories m ON m.id = f.memory_id
         WHERE f.resolved_at IS NULL
           AND f.verdict IN ('wrong', 'outdated')
         GROUP BY f.memory_id, m.content, m.kind, m.status, m.team_id, m.valid_to
         ORDER BY (count(*) FILTER (WHERE f.verdict = 'wrong')) DESC,
                  count(*) DESC,
                  min(f.created_at) ASC
         LIMIT $1",
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| FlaggedMemory {
            memory_id: r.get("memory_id"),
            content: r.get("content"),
            kind: r.get("kind"),
            status: r.get("status"),
            team_id: r.get("team_id"),
            valid_to: r.get("valid_to"),
            wrong: r.get("wrong"),
            outdated: r.get("outdated"),
            notes: r
                .get::<Vec<String>, _>("notes")
                .into_iter()
                .take(5)
                .collect(),
            oldest_claim_secs: r.get("oldest_claim_secs"),
        })
        .collect())
}

/// Count of memories with open claims — the console badge / analytics tile.
pub async fn flagged_count(conn: &mut PgConnection) -> Result<i64> {
    let row = sqlx::query(
        "SELECT count(DISTINCT f.memory_id) AS n
         FROM memory_feedback f
         JOIN memories m ON m.id = f.memory_id
         WHERE f.resolved_at IS NULL AND f.verdict IN ('wrong', 'outdated')",
    )
    .fetch_one(conn)
    .await?;
    Ok(row.get("n"))
}

/// Close every open claim against a memory with the maintainer's answer.
/// Returns how many claims were closed (0 = nothing was open).
pub async fn resolve_claims(
    conn: &mut PgConnection,
    memory_id: Uuid,
    resolver: Uuid,
    resolution: &str,
) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE memory_feedback
         SET resolution = $3, resolved_by = $2, resolved_at = now()
         WHERE memory_id = $1
           AND resolved_at IS NULL
           AND verdict IN ('wrong', 'outdated')",
    )
    .bind(memory_id)
    .bind(resolver)
    .bind(resolution)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}
