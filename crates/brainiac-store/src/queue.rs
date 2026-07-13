//! SKIP-LOCKED job queue (PLAN.md deviation 1: pgmq-shaped semantics —
//! send / read-with-visibility-timeout / complete / dead-letter archive —
//! without the extension dependency).
//!
//! ## Attempt & retry semantics (Direction 1)
//!
//! `attempts` is bumped **on claim** ([`read`]), not on failure — a claim is
//! the unit of delivery, so a crash-redelivered job and a cleanly-failed one
//! both consume the same budget. That is only honest because the ceiling is
//! *enforced at claim time*: a job whose `attempts` already reached
//! [`MAX_ATTEMPTS`] but was never acked (the worker panicked/crashed before
//! [`fail`] ran) is **reaped** by [`read`] into the dead-letter archive rather
//! than redelivered forever. Without this reaping a deterministic crasher would
//! be re-served every visibility window for eternity.
//!
//! ## Archive outcomes
//!
//! - `ok`     — [`complete`]: the job succeeded.
//! - `failed` — [`fail`] after the budget is spent: an *adjudicated* failure,
//!   i.e. the worker ran the job, caught an error, and reported it through
//!   `fail()` until `attempts >= MAX_ATTEMPTS`. The error was observed.
//! - `dead`   — claim-time reaping of **crash-poison**: the job kept crashing
//!   the worker before it could report, so `fail()` was never reached and the
//!   attempt counter climbed purely on redelivery.
//!
//! Both `failed` and `dead` are terminal and both are surfaced by
//! [`dead_letters`] / recoverable by [`requeue_dead`] — the split exists so
//! [`health`] can tell "the job failed and we know why" apart from "the job
//! took the worker down without a word", which are very different operational
//! signals. An intermediate [`fail`] that re-queues is **not** archived.

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

/// Backoff ceiling for [`fail`]: no single retry parks a job longer than this,
/// regardless of attempt count, so a flapping dependency can't stall a job for
/// hours while still letting early retries be gentle.
pub const BACKOFF_CAP_SECS: i64 = 600;

/// Exponential, attempt-scaled backoff: `base` doubles per prior attempt,
/// capped at [`BACKOFF_CAP_SECS`]. `base = 0` disables the wait entirely
/// (tests force immediate redelivery this way). `attempts` is the post-claim
/// count (>= 1), so attempt 1 waits `base`, attempt 2 waits `2*base`, etc.
fn backoff_secs(base: i64, attempts: i32) -> i64 {
    if base <= 0 {
        return 0;
    }
    // Cap the shift well before i64 overflow; MAX_ATTEMPTS keeps it tiny anyway.
    let shift = (attempts.max(1) - 1).min(16) as u32;
    base.saturating_mul(1i64 << shift).min(BACKOFF_CAP_SECS)
}

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
///
/// Claim-time reaping (Direction 1): before claiming, any *ready* job whose
/// `attempts` already reached [`MAX_ATTEMPTS`] is moved to the dead-letter
/// archive as `dead` (crash-poison — it exhausted its budget on redelivery
/// without ever being acked). The claim itself then only considers jobs still
/// under budget, so a deterministic crasher terminates instead of looping
/// forever. Reap + claim run in one transaction so a claimed job can never
/// slip past the ceiling.
///
/// Per-org fair claiming (Direction 2): the claim does NOT take jobs in strict
/// global id order — that let one org's flood of 100 transcripts head-of-line
/// block every other tenant until it drained. Instead ready jobs are ranked
/// *per org* (`ROW_NUMBER() OVER (PARTITION BY payload->>'org_id' ORDER BY id)`)
/// and claimed by `(rank, id)`, so every org's head job is served before any
/// org's second — a round-robin across tenants — while FIFO **within** an org
/// is preserved (rank follows id). Jobs with no `org_id` in their payload share
/// one bucket (`payload->>'org_id'` is NULL and all NULLs collapse into a single
/// partition), FIFO among themselves.
///
/// Locking: `FOR UPDATE SKIP LOCKED` cannot sit in the same query level as the
/// `ROW_NUMBER()` window, so this is a two-step within one statement — an inner
/// ranking subquery picks the fair top-`n` ids (no lock), then a wrapping
/// `SELECT ... FOR UPDATE SKIP LOCKED` re-selects exactly those ids by primary
/// key and takes the row locks, skipping any a concurrent worker already holds.
/// Correctness: two workers may compute overlapping candidate id sets, but the
/// row lock is the arbiter — only one can lock a given row; the other SKIP-LOCKs
/// it. So no job is ever double-claimed. The only cost is that when a candidate
/// is already locked this worker claims fewer than `n` that round (it simply
/// retries next tick) — never a correctness issue.
///
/// Performance: the ranking subquery scans the ready set of the queue each call.
/// At current scale (thousands of ready jobs at most) this is trivial and the
/// `idx_queue_jobs_ready(queue_name, visible_at)` index still serves the
/// `WHERE queue_name = $1 AND visible_at <= now()` predicate that bounds the
/// scan; the per-org ROW_NUMBER sort is over that already-filtered set. No new
/// index is warranted until the ready backlog grows large enough for the sort
/// to show in EXPLAIN — see migration notes.
pub async fn read(pool: &PgPool, queue: &str, n: i64, visibility_secs: i64) -> Result<Vec<Job>> {
    let mut tx = pool.begin().await?;

    // Reap crash-poison: ready jobs at/over the attempt ceiling that were never
    // acked. `fail()` handles the ceiling for jobs the worker actually reported
    // on; this catches the ones that took the worker down before it could.
    sqlx::query(
        "WITH reaped AS (
             DELETE FROM queue.jobs
             WHERE queue_name = $1 AND visible_at <= now() AND attempts >= $2
             RETURNING id, queue_name, payload, attempts, enqueued_at
         )
         INSERT INTO queue.archive (id, queue_name, payload, attempts, enqueued_at, outcome)
         SELECT id, queue_name, payload, attempts, enqueued_at, 'dead' FROM reaped
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(queue)
    .bind(MAX_ATTEMPTS)
    .execute(&mut *tx)
    .await?;

    let rows = sqlx::query(
        "UPDATE queue.jobs j
         SET visible_at = now() + make_interval(secs => $3::double precision),
             attempts = j.attempts + 1
         WHERE j.id IN (
             -- Lock exactly the fair top-n ids picked by the inner ranking.
             -- FOR UPDATE SKIP LOCKED lives here (a level with no window fn);
             -- it is the arbiter that prevents concurrent double-claims. The
             -- readiness predicate (visible_at/attempts) is RE-STATED here, not
             -- only in the inner ranking: the ranking runs unlocked, so between
             -- ranking and locking another worker may have claimed a candidate
             -- and moved its visible_at into the future. Postgres re-checks this
             -- WHERE against the freshly-locked row version (EvalPlanQual), so
             -- the now-invisible row is dropped instead of being double-claimed.
             SELECT id FROM queue.jobs
             WHERE queue_name = $1 AND visible_at <= now() AND attempts < $4
               AND id IN (
                 SELECT id FROM (
                     SELECT id,
                            ROW_NUMBER() OVER (
                                PARTITION BY payload->>'org_id' ORDER BY id
                            ) AS org_rank
                     FROM queue.jobs
                     WHERE queue_name = $1 AND visible_at <= now() AND attempts < $4
                 ) ranked
                 ORDER BY org_rank, id
                 LIMIT $2
             )
             ORDER BY id
             FOR UPDATE SKIP LOCKED
         )
         RETURNING j.id, j.queue_name, j.payload, j.attempts",
    )
    .bind(queue)
    .bind(n)
    .bind(visibility_secs as f64)
    .bind(MAX_ATTEMPTS)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
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

/// Report an *adjudicated* failure: re-queue with exponential backoff, or
/// archive as `failed` once the attempt budget is spent. `base_backoff_secs`
/// is the first-retry delay; it doubles per attempt up to [`BACKOFF_CAP_SECS`]
/// (pass 0 for immediate redelivery). Returns true when the job will retry.
///
/// The terminal outcome here is `failed` (not `dead`): the worker ran the job,
/// observed the error, and exhausted its retries — distinct from crash-poison,
/// which [`read`] reaps as `dead`. See the module docs.
pub async fn fail(pool: &PgPool, job: &Job, base_backoff_secs: i64) -> Result<bool> {
    if job.attempts >= MAX_ATTEMPTS {
        archive(pool, job, "failed").await?;
        return Ok(false);
    }
    let delay = backoff_secs(base_backoff_secs, job.attempts);
    sqlx::query(
        "UPDATE queue.jobs
         SET visible_at = now() + make_interval(secs => $2::double precision)
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(delay as f64)
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
    /// Adjudicated failures: the worker ran the job, reported errors through
    /// `fail()`, and spent the retry budget (`outcome = 'failed'`).
    pub archived_failed: i64,
    /// Crash-poison reaped at claim time: the job exhausted its budget on
    /// redelivery without ever being acked (`outcome = 'dead'`).
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

/// Terminally-archived jobs (both `failed` and `dead`), most recent first —
/// the operator recovery surface, previously reachable only via manual SQL on
/// queue.archive. Both outcomes are dead letters an operator may inspect and
/// requeue; the `failed`/`dead` distinction is preserved in [`health`].
pub async fn dead_letters(pool: &PgPool, queue: &str, limit: i64) -> Result<Vec<DeadLetter>> {
    let rows = sqlx::query(
        "SELECT id, payload, attempts, enqueued_at, archived_at
         FROM queue.archive
         WHERE queue_name = $1 AND outcome IN ('failed', 'dead')
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
         FROM queue.archive WHERE id = $1 AND outcome IN ('failed', 'dead')
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
