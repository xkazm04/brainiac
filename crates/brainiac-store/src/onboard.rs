//! Onboarding pairing requests (migrations/0034_projects_onboarding.sql) —
//! the device-authorization pattern, self-hosted.
//!
//! Lifecycle: `start` (unauthenticated CLI) → `approve`/`deny` (authenticated
//! operator in the console) → `claim` (the CLI's poll, which is what actually
//! mints the key — see the server's onboard module). Rows begin org-less and
//! acquire org/project at approval; the minted secret is never stored here.
//!
//! No RLS, same reason as tokens.rs: this is the machinery that produces a
//! principal, so it runs on the raw pool with explicit scoping in SQL.

use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OnboardRequestRow {
    pub id: Uuid,
    pub user_code: String,
    pub remote: String,
    pub label: String,
    pub status: String,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub approved_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

fn row_of(r: &sqlx::postgres::PgRow) -> OnboardRequestRow {
    OnboardRequestRow {
        id: r.get("id"),
        user_code: r.get("user_code"),
        remote: r.get("remote"),
        label: r.get("label"),
        status: r.get("status"),
        org_id: r.get("org_id"),
        project_id: r.get("project_id"),
        approved_by: r.get("approved_by"),
        created_at: r.get("created_at"),
        expires_at: r.get("expires_at"),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn start(
    pool: &PgPool,
    id: Uuid,
    user_code: &str,
    device_code_hash: &[u8],
    remote: &str,
    label: &str,
    ttl_secs: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO onboard_requests
             (id, user_code, device_code_hash, remote, label, expires_at)
         VALUES ($1, $2, $3, $4, $5, now() + make_interval(secs => $6))",
    )
    .bind(id)
    .bind(user_code)
    .bind(device_code_hash)
    .bind(remote)
    .bind(label)
    .bind(ttl_secs as f64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Live (unexpired) pending requests, deployment-wide. This is the start
/// endpoint's flood gauge — `start` is unauthenticated, so the cap on this
/// count is the only thing bounding table growth from an anonymous peer.
pub async fn pending_count(pool: &PgPool) -> Result<i64> {
    let row = sqlx::query(
        "SELECT count(*) AS n FROM onboard_requests
         WHERE status = 'pending' AND expires_at > now()",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("n"))
}

/// The console's approval queue: pending, unexpired, oldest first.
pub async fn list_pending(pool: &PgPool) -> Result<Vec<OnboardRequestRow>> {
    let rows = sqlx::query(
        "SELECT id, user_code, remote, label, status, org_id, project_id,
                approved_by, created_at, expires_at
         FROM onboard_requests
         WHERE status = 'pending' AND expires_at > now()
         ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_of).collect())
}

/// The poll lookup, by hashed device code. Returns whatever state the row is
/// in (including expired-but-pending); the server maps states to responses.
pub async fn get_by_device_hash(
    pool: &PgPool,
    device_code_hash: &[u8],
) -> Result<Option<OnboardRequestRow>> {
    let row = sqlx::query(
        "SELECT id, user_code, remote, label, status, org_id, project_id,
                approved_by, created_at, expires_at
         FROM onboard_requests WHERE device_code_hash = $1",
    )
    .bind(device_code_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_of))
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<OnboardRequestRow>> {
    let row = sqlx::query(
        "SELECT id, user_code, remote, label, status, org_id, project_id,
                approved_by, created_at, expires_at
         FROM onboard_requests WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_of))
}

/// Approve a pending, unexpired request into an org + project. False when the
/// request is gone, expired, or already decided — approval is single-shot.
pub async fn approve(
    pool: &PgPool,
    id: Uuid,
    org_id: Uuid,
    project_id: Uuid,
    approved_by: Uuid,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE onboard_requests
         SET status = 'approved', org_id = $2, project_id = $3, approved_by = $4
         WHERE id = $1 AND status = 'pending' AND expires_at > now()",
    )
    .bind(id)
    .bind(org_id)
    .bind(project_id)
    .bind(approved_by)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn deny(pool: &PgPool, id: Uuid) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE onboard_requests SET status = 'denied'
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Opportunistic hygiene, called from the unauthenticated start path: rows a
/// day past expiry are dead weight whatever state they reached (pairing codes
/// are single-shot and the minted key lives in api_tokens, not here).
pub async fn prune_expired(pool: &PgPool) -> Result<u64> {
    let res = sqlx::query(
        "DELETE FROM onboard_requests WHERE expires_at < now() - interval '1 day'",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Claim an approved request — the single-shot transition that authorizes
/// minting. The WHERE clause is the whole security argument: only an approved,
/// unexpired row flips, and it flips exactly once, so two racing polls with
/// the same device code mint at most one key.
pub async fn claim(
    pool: &PgPool,
    device_code_hash: &[u8],
) -> Result<Option<OnboardRequestRow>> {
    let row = sqlx::query(
        "UPDATE onboard_requests
         SET status = 'claimed', claimed_at = now()
         WHERE device_code_hash = $1 AND status = 'approved' AND expires_at > now()
         RETURNING id, user_code, remote, label, status, org_id, project_id,
                   approved_by, created_at, expires_at",
    )
    .bind(device_code_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_of))
}
