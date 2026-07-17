//! Projects + the repo whitelist (migrations/0034_projects_onboarding.sql).
//!
//! Same discipline as tokens.rs: these tables carry no RLS because they are
//! consulted by the machinery that PRODUCES principals (onboarding approval,
//! token minting), so every function takes an explicit `org_id` and scopes
//! its SQL to it.

use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct RepoRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub remote: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Create a project. Returns false when the org already has one by that name
/// (the UNIQUE(org_id, name) constraint, surfaced as an outcome not an error).
pub async fn create(pool: &PgPool, id: Uuid, org_id: Uuid, name: &str) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO projects (id, org_id, name) VALUES ($1, $2, $3)
         ON CONFLICT (org_id, name) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<ProjectRow>> {
    let rows = sqlx::query(
        "SELECT id, name, created_at FROM projects WHERE org_id = $1 ORDER BY created_at",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| ProjectRow {
            id: r.get("id"),
            name: r.get("name"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// Every whitelisted repo in the org, for assembling the projects view.
pub async fn list_repos(pool: &PgPool, org_id: Uuid) -> Result<Vec<RepoRow>> {
    let rows = sqlx::query(
        "SELECT id, project_id, remote, created_at
         FROM project_repos WHERE org_id = $1 ORDER BY created_at",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| RepoRow {
            id: r.get("id"),
            project_id: r.get("project_id"),
            remote: r.get("remote"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// Does this project exist in this org? Minting and repo management both need
/// the answer before writing anything that references the project.
pub async fn belongs(pool: &PgPool, org_id: Uuid, project_id: Uuid) -> Result<bool> {
    let row = sqlx::query("SELECT 1 AS one FROM projects WHERE id = $1 AND org_id = $2")
        .bind(project_id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// Whitelist a (normalized) remote under a project. The guarded INSERT…SELECT
/// refuses to attach a repo to another org's project; ON CONFLICT refuses a
/// remote already claimed in this org. Both come back as `false` — the caller
/// distinguishes with [`belongs`] when it needs a precise status code.
pub async fn add_repo(
    pool: &PgPool,
    id: Uuid,
    org_id: Uuid,
    project_id: Uuid,
    remote: &str,
) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO project_repos (id, org_id, project_id, remote)
         SELECT $1, $2, $3, $4
         WHERE EXISTS (SELECT 1 FROM projects WHERE id = $3 AND org_id = $2)
         ON CONFLICT (org_id, remote) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(project_id)
    .bind(remote)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Remove a whitelisted repo. Returns false when it doesn't exist (or belongs
/// to another org — indistinguishable on purpose, same as token revoke).
pub async fn remove_repo(pool: &PgPool, org_id: Uuid, repo_id: Uuid) -> Result<bool> {
    let res = sqlx::query("DELETE FROM project_repos WHERE id = $1 AND org_id = $2")
        .bind(repo_id)
        .bind(org_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// The whitelist lookup onboarding approval rides on: which project (if any)
/// claims this normalized remote in this org?
pub async fn find_by_remote(
    pool: &PgPool,
    org_id: Uuid,
    remote: &str,
) -> Result<Option<(Uuid, String)>> {
    let row = sqlx::query(
        "SELECT p.id, p.name FROM project_repos pr
         JOIN projects p ON p.id = pr.project_id
         WHERE pr.org_id = $1 AND pr.remote = $2",
    )
    .bind(org_id)
    .bind(remote)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("name"))))
}
