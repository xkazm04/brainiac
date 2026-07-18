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
    /// Repo-relative subdirectory this row claims ('' = the whole repo).
    /// See migrations/0039_project_path_prefix.sql.
    pub path_prefix: String,
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
        "SELECT id, project_id, remote, path_prefix, created_at
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
            path_prefix: r.get("path_prefix"),
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

/// Flip a project's opt-in RLS isolation (migration 0040). Scoped to the org
/// so one tenant can never toggle another's project. Returns false when no
/// project by that id exists in the org (surfaced as a 404 by the handler),
/// same shape as [`create`] / [`add_repo`].
pub async fn set_isolated(
    pool: &PgPool,
    org_id: Uuid,
    project_id: Uuid,
    isolated: bool,
) -> Result<bool> {
    let res = sqlx::query("UPDATE projects SET isolated = $3 WHERE id = $1 AND org_id = $2")
        .bind(project_id)
        .bind(org_id)
        .bind(isolated)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Whitelist a (normalized) remote — optionally scoped to a repo-relative
/// `path_prefix` ('' = the whole repo) — under a project. The guarded
/// INSERT…SELECT refuses to attach a repo to another org's project;
/// ON CONFLICT refuses a (remote, path_prefix) pair already claimed in this
/// org (a monorepo CAN have several rows on the same remote, one per prefix —
/// see migrations/0039_project_path_prefix.sql). Both come back as `false` —
/// the caller distinguishes with [`belongs`] when it needs a precise status
/// code.
pub async fn add_repo(
    pool: &PgPool,
    id: Uuid,
    org_id: Uuid,
    project_id: Uuid,
    remote: &str,
    path_prefix: &str,
) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO project_repos (id, org_id, project_id, remote, path_prefix)
         SELECT $1, $2, $3, $4, $5
         WHERE EXISTS (SELECT 1 FROM projects WHERE id = $3 AND org_id = $2)
         ON CONFLICT (org_id, remote, path_prefix) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(project_id)
    .bind(remote)
    .bind(path_prefix)
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
/// claims this normalized remote in this org, at this repo-relative `path`
/// (pass `""` for a whole-repo lookup)?
///
/// Resolution is LONGEST-PREFIX match: a row whose `path_prefix` is a prefix
/// of `path` is eligible, and among eligible rows the longest prefix wins —
/// so a monorepo split (`apps/web` vs `apps`) always prefers the more
/// specific project, and the `''` (whole-repo) row is the fallback every
/// pre-monorepo caller still gets when it passes an empty path.
pub async fn find_by_remote(
    pool: &PgPool,
    org_id: Uuid,
    remote: &str,
    path: &str,
) -> Result<Option<(Uuid, String)>> {
    // Match on a SEGMENT boundary, not a bare character prefix: a row's
    // `path_prefix` claims `path` only when it is the whole repo (''), equals
    // `path` exactly, or is followed by a '/' in `path`. Otherwise a prefix
    // `client` would wrongly claim another project's `clientdata/…` path — a
    // cross-project mis-attribution, which is exactly the isolation this arc
    // must not leak. `left(…)` (not `LIKE`) so a real directory name's `_`/`%`
    // is never treated as a wildcard.
    let row = sqlx::query(
        "SELECT p.id, p.name FROM project_repos pr
         JOIN projects p ON p.id = pr.project_id
         WHERE pr.org_id = $1 AND pr.remote = $2
           AND (pr.path_prefix = ''
                OR $3 = pr.path_prefix
                OR left($3, length(pr.path_prefix) + 1) = pr.path_prefix || '/')
         ORDER BY length(pr.path_prefix) DESC
         LIMIT 1",
    )
    .bind(org_id)
    .bind(remote)
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("name"))))
}
