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

// ── health & dead-letter operations ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct QueueHealth {
    pub queue_name: String,
    /// Jobs claimable right now.
    pub ready: i64,
    /// Jobs inside a visibility window (claimed, not yet acked).
    pub in_flight: i64,
    /// Age of the oldest ready job; 0 when the queue is drained.
    pub oldest_ready_secs: i64,
    /// (attempts, count) over live jobs — a tail at high attempts means
    /// poison input burning retry budget.
    pub attempts_histogram: Vec<(i32, i64)>,
    pub archived_ok: i64,
    pub archived_failed: i64,
    pub dead_letters: i64,
}

pub async fn health(pool: &PgPool, queue: &str) -> Result<QueueHealth> {
    let live = sqlx::query(
        "SELECT count(*) FILTER (WHERE visible_at <= now()) AS ready,
                count(*) FILTER (WHERE visible_at > now()) AS in_flight,
                COALESCE(EXTRACT(EPOCH FROM now() - min(enqueued_at)
                    FILTER (WHERE visible_at <= now())), 0)::bigint AS oldest_ready_secs
         FROM queue.jobs WHERE queue_name = $1",
    )
    .bind(queue)
    .fetch_one(pool)
    .await?;
    let histogram = sqlx::query(
        "SELECT attempts, count(*) AS n FROM queue.jobs
         WHERE queue_name = $1 GROUP BY attempts ORDER BY attempts",
    )
    .bind(queue)
    .fetch_all(pool)
    .await?;
    let archived = sqlx::query(
        "SELECT count(*) FILTER (WHERE outcome = 'ok') AS ok,
                count(*) FILTER (WHERE outcome = 'failed') AS failed,
                count(*) FILTER (WHERE outcome = 'dead') AS dead
         FROM queue.archive WHERE queue_name = $1",
    )
    .bind(queue)
    .fetch_one(pool)
    .await?;
    Ok(QueueHealth {
        queue_name: queue.to_string(),
        ready: live.get("ready"),
        in_flight: live.get("in_flight"),
        oldest_ready_secs: live.get("oldest_ready_secs"),
        attempts_histogram: histogram
            .iter()
            .map(|r| (r.get::<i32, _>("attempts"), r.get::<i64, _>("n")))
            .collect(),
        archived_ok: archived.get("ok"),
        archived_failed: archived.get("failed"),
        dead_letters: archived.get("dead"),
    })
}

#[derive(Debug, Clone)]
pub struct DeadLetter {
    pub id: i64,
    pub payload: Value,
    pub attempts: i32,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub archived_at: chrono::DateTime<chrono::Utc>,
}

/// Dead-lettered jobs, most recent first — previously reachable only via
/// manual SQL on queue.archive.
pub async fn dead_letters(pool: &PgPool, queue: &str, limit: i64) -> Result<Vec<DeadLetter>> {
    let rows = sqlx::query(
        "SELECT id, payload, attempts, enqueued_at, archived_at
         FROM queue.archive
         WHERE queue_name = $1 AND outcome = 'dead'
         ORDER BY archived_at DESC
         LIMIT $2",
    )
    .bind(queue)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DeadLetter {
            id: r.get("id"),
            payload: r.get("payload"),
            attempts: r.get("attempts"),
            enqueued_at: r.get("enqueued_at"),
            archived_at: r.get("archived_at"),
        })
        .collect())
}

/// Move a dead-lettered job back into the live queue with a fresh attempt
/// budget. Returns false when the id isn't a dead letter. Reusing the id is
/// safe: bigserial never re-issues consumed ids, so no collision.
pub async fn requeue_dead(pool: &PgPool, id: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO queue.jobs (id, queue_name, payload, attempts, visible_at, enqueued_at)
         SELECT id, queue_name, payload, 0, now(), now()
         FROM queue.archive WHERE id = $1 AND outcome = 'dead'
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query("DELETE FROM queue.archive WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}
