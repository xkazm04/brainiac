//! SKIP-LOCKED job queue (PLAN.md deviation 1: pgmq-shaped semantics —
//! send / read-with-visibility-timeout / complete / dead-letter archive —
//! without the extension dependency).

use anyhow::Result;
use serde_json::Value;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub queue_name: String,
    pub payload: Value,
    pub attempts: i32,
}

pub const MAX_ATTEMPTS: i32 = 5;

pub async fn send(pool: &PgPool, queue: &str, payload: &Value) -> Result<i64> {
    let row =
        sqlx::query("INSERT INTO queue.jobs (queue_name, payload) VALUES ($1, $2) RETURNING id")
            .bind(queue)
            .bind(payload)
            .fetch_one(pool)
            .await?;
    Ok(row.get::<i64, _>("id"))
}

/// Claim up to `n` ready jobs; each becomes invisible for `visibility_secs`.
/// Crash-safe: an unacknowledged job reappears after the timeout with its
/// attempt counter bumped.
pub async fn read(pool: &PgPool, queue: &str, n: i64, visibility_secs: i64) -> Result<Vec<Job>> {
    let rows = sqlx::query(
        "UPDATE queue.jobs j
         SET visible_at = now() + make_interval(secs => $3::double precision),
             attempts = j.attempts + 1
         WHERE j.id IN (
             SELECT id FROM queue.jobs
             WHERE queue_name = $1 AND visible_at <= now()
             ORDER BY id
             LIMIT $2
             FOR UPDATE SKIP LOCKED
         )
         RETURNING j.id, j.queue_name, j.payload, j.attempts",
    )
    .bind(queue)
    .bind(n)
    .bind(visibility_secs as f64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Job {
            id: r.get("id"),
            queue_name: r.get("queue_name"),
            payload: r.get("payload"),
            attempts: r.get("attempts"),
        })
        .collect())
}

/// Acknowledge success: move to archive with outcome `ok`.
pub async fn complete(pool: &PgPool, job: &Job) -> Result<()> {
    archive(pool, job, "ok").await
}

/// Report failure: re-queue with backoff, or dead-letter after MAX_ATTEMPTS.
/// Returns true when the job will retry.
pub async fn fail(pool: &PgPool, job: &Job, backoff_secs: i64) -> Result<bool> {
    if job.attempts >= MAX_ATTEMPTS {
        archive(pool, job, "dead").await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE queue.jobs
         SET visible_at = now() + make_interval(secs => $2::double precision)
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(backoff_secs as f64)
    .execute(pool)
    .await?;
    Ok(true)
}

async fn archive(pool: &PgPool, job: &Job, outcome: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO queue.archive (id, queue_name, payload, attempts, enqueued_at, outcome)
         SELECT id, queue_name, payload, attempts, enqueued_at, $2
         FROM queue.jobs WHERE id = $1
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(job.id)
    .bind(outcome)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM queue.jobs WHERE id = $1")
        .bind(job.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Depth of a queue (ready + in-flight) — observability + test assertions.
pub async fn depth(pool: &PgPool, queue: &str) -> Result<i64> {
    let row = sqlx::query("SELECT count(*) AS n FROM queue.jobs WHERE queue_name = $1")
        .bind(queue)
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("n"))
}
