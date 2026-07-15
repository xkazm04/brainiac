//! Managed API tokens (migrations/0003_api_tokens.sql).
//!
//! Resolution runs on the RAW POOL, not a scoped transaction — it happens
//! before a principal exists (it is what produces the principal). The table
//! carries no RLS for that reason; every management function here takes an
//! explicit `org_id` and scopes its SQL to it.

use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ResolvedToken {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TokenRow {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Look up a live token by secret hash and touch `last_used_at`.
pub async fn resolve(pool: &PgPool, token_hash: &[u8]) -> Result<Option<ResolvedToken>> {
    let row = sqlx::query(
        "UPDATE api_tokens SET last_used_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL
         RETURNING org_id, user_id, scopes",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ResolvedToken {
        org_id: r.get("org_id"),
        user_id: r.get("user_id"),
        scopes: r.get("scopes"),
    }))
}

/// Team memberships for a token's user, resolved at auth time so membership
/// changes take effect immediately (unlike the static env stub).
pub async fn team_ids_of(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> Result<Vec<Uuid>> {
    // Scope this lookup to the token's own org BEFORE querying.
    //
    // `team_members` is RLS'd on `current_setting('app.org_id')::uuid`, but this
    // runs during auth resolution — BEFORE a principal exists, because it is what
    // builds one — so no `scoped_tx` has set that GUC. Postgres does not treat an
    // unset custom parameter as NULL: `current_setting` ERRORS with
    // "unrecognized configuration parameter", which surfaced as a 500 on EVERY
    // request made with a managed `brk_` token. Env bootstrap tokens resolve
    // earlier and never reach here, which is why the whole managed-token surface
    // could be broken without a single test noticing.
    //
    // Setting the GUC from `org_id` is not a widening: that value comes from the
    // token's own row, which is the authority on which org the token belongs to.
    // We are scoping the membership read to exactly the tenant the caller already
    // proved they are.
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(org_id.to_string())
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query(
        "SELECT tm.team_id FROM team_members tm
         JOIN teams t ON t.id = tm.team_id
         WHERE tm.user_id = $1 AND t.org_id = $2",
    )
    .bind(user_id)
    .bind(org_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.iter().map(|r| r.get("team_id")).collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &PgPool,
    id: Uuid,
    org_id: Uuid,
    user_id: Uuid,
    name: &str,
    prefix: &str,
    token_hash: &[u8],
    scopes: &[String],
    created_by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO api_tokens (id, org_id, user_id, name, prefix, token_hash, scopes, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(org_id)
    .bind(user_id)
    .bind(name)
    .bind(prefix)
    .bind(token_hash)
    .bind(scopes)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<TokenRow>> {
    let rows = sqlx::query(
        "SELECT id, name, prefix, scopes, created_at, last_used_at, revoked_at
         FROM api_tokens WHERE org_id = $1 ORDER BY created_at DESC",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| TokenRow {
            id: r.get("id"),
            name: r.get("name"),
            prefix: r.get("prefix"),
            scopes: r.get("scopes"),
            created_at: r.get("created_at"),
            last_used_at: r.get("last_used_at"),
            revoked_at: r.get("revoked_at"),
        })
        .collect())
}

/// Revoke a token in the caller's org. Returns false when it doesn't exist
/// (or belongs to another org — indistinguishable on purpose).
pub async fn revoke(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE api_tokens SET revoked_at = now()
         WHERE id = $1 AND org_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
