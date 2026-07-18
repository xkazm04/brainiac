//! The project registry + repo whitelist (migration 0034).
//!
//! A project is the logical unit an API key scopes to — an application or a
//! business domain under the org. A repo row whitelists one normalized git
//! remote under one project; the onboarding pairing flow (crate::onboard)
//! refuses to mint a key for any remote this registry doesn't claim, which is
//! what makes the registry the onboarding allow-list rather than bookkeeping.
//!
//! All endpoints are operator surfaces (admin scope), same as token minting:
//! deciding which repos may join the org's memory IS access control.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, AppState, HttpError};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/projects", get(projects_list).post(project_create))
        .route("/v1/projects/{id}/repos", axum::routing::post(repo_add))
        .route("/v1/projects/{id}/repos/{repo_id}", delete(repo_remove))
}

/// Cap on user-supplied names/remotes — identifiers, not prose.
const MAX_NAME_CHARS: usize = 200;

// ── remote normalization ────────────────────────────────────────────────

/// Normalize a git remote URL to `host/owner/name` (lowercase host+owner,
/// `.git` and trailing slashes stripped). Every spelling of the same repo —
/// https, ssh, scp-style, or already-bare — must collide to one whitelist row,
/// or the whitelist silently stops matching what a developer's checkout says.
pub fn normalize_remote(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // Strip protocol prefixes: https://, http://, ssh://, git://.
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .or_else(|| s.strip_prefix("ssh://"))
        .or_else(|| s.strip_prefix("git://"))
        .unwrap_or(s);
    // scp-style `git@host:owner/name` (optionally `git@host:2222/owner/name`
    // with an SSH port) → `host/owner/name`.
    let s = match (s.strip_prefix("git@"), s.find(':')) {
        (Some(rest), _) => rest.replacen(':', "/", 1),
        (None, _) => s.to_string(),
    };
    // A leftover `user@` (ssh://git@host/…) or embedded credentials.
    let s = s
        .rsplit_once('@')
        .map(|(_, tail)| tail.to_string())
        .unwrap_or(s);
    let s = s.trim_end_matches('/').trim_end_matches(".git");
    let mut parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    // host/owner/name — exactly three segments. Deeper paths (GitLab
    // subgroups) keep everything after the host as the repo path.
    if parts.len() < 3 {
        return None;
    }
    // Both the scp form (`git@host:2222/owner/name`, after the `:` → `/`
    // rewrite above becomes `host/2222/owner/name`) and the `ssh://`
    // form (`ssh://host:2222/owner/name`, port still attached to the host
    // segment) can carry an SSH port. Strip it so it isn't mistaken for
    // the owner path segment.
    if let Some((host, port)) = parts[0].split_once(':') {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            parts[0] = host;
        }
    } else if parts.len() >= 4
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && !parts[1].is_empty()
    {
        parts.remove(1);
    }
    if parts.len() < 3 {
        return None;
    }
    let host = parts[0].to_lowercase();
    // A plausible hostname: non-empty, alphanumeric + hyphens (dots are
    // allowed too, for public hosts like github.com). Self-hosted Git
    // servers commonly live behind a bare internal hostname with no
    // public DNS suffix — see `dot_less_self_hosted_host_is_accepted`.
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return None;
    }
    let owner = parts[1..parts.len() - 1].join("/").to_lowercase();
    let name = parts[parts.len() - 1];
    if name.is_empty() || owner.is_empty() {
        return None;
    }
    Some(format!("{host}/{owner}/{name}"))
}

/// Normalize a repo-relative path prefix for `project_repos.path_prefix` /
/// onboarding's `path`: trim whitespace and strip leading/trailing slashes
/// so `"/apps/web/"`, `"apps/web"`, and `"apps/web/"` all collide to
/// `"apps/web"` — the longest-prefix match in
/// [`brainiac_store::projects::find_by_remote`] otherwise depends on exact
/// string shape. `""` (the default) means "whole repo".
pub fn normalize_path_prefix(raw: &str) -> String {
    raw.trim().trim_matches('/').to_string()
}

// ── DTOs ────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectRepoView {
    pub id: Uuid,
    /// Normalized remote, e.g. `github.com/acme/payments`.
    pub remote: String,
    /// Repo-relative subdirectory this row claims ('' = the whole repo —
    /// see migrations/0039_project_path_prefix.sql).
    pub path_prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub repos: Vec<ProjectRepoView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectsListResponse {
    pub projects: Vec<ProjectView>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateProjectBody {
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct CreatedProjectResponse {
    pub id: Uuid,
    pub name: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct AddRepoBody {
    /// Any spelling of the remote (https, ssh, scp, bare); stored normalized.
    pub remote: String,
    /// Repo-relative subdirectory this row claims (e.g. `apps/web`), for
    /// splitting a monorepo across projects. Omit (or `""`) to claim the
    /// whole repo — the default, back-compat with every pre-monorepo caller.
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AddedRepoResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    /// The normalized form that was stored — what onboarding will match on.
    pub remote: String,
    pub path_prefix: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RemovedRepoResponse {
    pub id: Uuid,
    pub removed: bool,
}

// ── handlers ────────────────────────────────────────────────────────────

/// GET /v1/projects — the org's projects with their whitelisted repos.
#[utoipa::path(
    get,
    path = "/v1/projects",
    tag = "projects",
    description = "The org's projects with their whitelisted repos (admin).",
    responses(
        (status = 200, description = "Projects of the caller's org", body = ProjectsListResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn projects_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ProjectsListResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let pool = state.store.pool();
    let projects = brainiac_store::projects::list(pool, ctx.principal.org_id)
        .await
        .map_err(internal)?;
    let repos = brainiac_store::projects::list_repos(pool, ctx.principal.org_id)
        .await
        .map_err(internal)?;
    Ok(Json(ProjectsListResponse {
        projects: projects
            .into_iter()
            .map(|p| ProjectView {
                id: p.id,
                name: p.name,
                created_at: p.created_at,
                repos: repos
                    .iter()
                    .filter(|r| r.project_id == p.id)
                    .map(|r| ProjectRepoView {
                        id: r.id,
                        remote: r.remote.clone(),
                        path_prefix: r.path_prefix.clone(),
                        created_at: r.created_at,
                    })
                    .collect(),
            })
            .collect(),
    }))
}

/// POST /v1/projects — create a project.
#[utoipa::path(
    post,
    path = "/v1/projects",
    tag = "projects",
    description = "Create a project (admin). Names are unique per org.",
    request_body = CreateProjectBody,
    responses(
        (status = 201, description = "Project created", body = CreatedProjectResponse),
        (status = 400, description = "Empty or oversized name"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 409, description = "A project with this name already exists"),
    )
)]
pub(crate) async fn project_create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateProjectBody>,
) -> Result<(StatusCode, Json<CreatedProjectResponse>), HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("name must be 1..{MAX_NAME_CHARS} characters"),
        )
            .into());
    }
    let id = Uuid::new_v4();
    let created =
        brainiac_store::projects::create(state.store.pool(), id, ctx.principal.org_id, name)
            .await
            .map_err(internal)?;
    if !created {
        return Err((
            StatusCode::CONFLICT,
            format!("a project named {name:?} already exists"),
        )
            .into());
    }
    Ok((
        StatusCode::CREATED,
        Json(CreatedProjectResponse {
            id,
            name: name.to_string(),
        }),
    ))
}

/// POST /v1/projects/{id}/repos — whitelist a remote under a project.
#[utoipa::path(
    post,
    path = "/v1/projects/{id}/repos",
    tag = "projects",
    description = "Whitelist a git remote under a project (admin). The remote is normalized \
                   to `host/owner/name`; within an org a remote maps to exactly one project.",
    params(("id" = Uuid, Path, description = "Project id")),
    request_body = AddRepoBody,
    responses(
        (status = 201, description = "Repo whitelisted", body = AddedRepoResponse),
        (status = 400, description = "Remote is not a recognizable git remote URL"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Project not found in this org"),
        (status = 409, description = "This remote is already whitelisted in the org"),
    )
)]
pub(crate) async fn repo_add(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<AddRepoBody>,
) -> Result<(StatusCode, Json<AddedRepoResponse>), HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    if body.remote.chars().count() > MAX_NAME_CHARS {
        return Err((StatusCode::BAD_REQUEST, "remote is too long".to_string()).into());
    }
    let Some(remote) = normalize_remote(&body.remote) else {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "{:?} is not a recognizable git remote (expected e.g. \
                 https://github.com/owner/name or git@github.com:owner/name.git)",
                body.remote
            ),
        )
            .into());
    };
    let path_prefix = normalize_path_prefix(body.path_prefix.as_deref().unwrap_or(""));
    let pool = state.store.pool();
    let id = Uuid::new_v4();
    let added = brainiac_store::projects::add_repo(
        pool,
        id,
        ctx.principal.org_id,
        project_id,
        &remote,
        &path_prefix,
    )
    .await
    .map_err(internal)?;
    if !added {
        // The guarded insert can refuse for two reasons; tell them apart so the
        // operator fixes the right thing.
        let exists = brainiac_store::projects::belongs(pool, ctx.principal.org_id, project_id)
            .await
            .map_err(internal)?;
        if !exists {
            return Err((StatusCode::NOT_FOUND, "project not found".to_string()).into());
        }
        return Err((
            StatusCode::CONFLICT,
            format!("{remote} (path {path_prefix:?}) is already whitelisted in this org"),
        )
            .into());
    }
    Ok((
        StatusCode::CREATED,
        Json(AddedRepoResponse {
            id,
            project_id,
            remote,
            path_prefix,
        }),
    ))
}

/// DELETE /v1/projects/{id}/repos/{repo_id} — un-whitelist a remote.
#[utoipa::path(
    delete,
    path = "/v1/projects/{id}/repos/{repo_id}",
    tag = "projects",
    description = "Remove a whitelisted remote (admin). Existing keys stay valid; only \
                   future onboarding stops matching.",
    params(
        ("id" = Uuid, Path, description = "Project id (route context only)"),
        ("repo_id" = Uuid, Path, description = "Repo row id"),
    ),
    responses(
        (status = 200, description = "Repo removed", body = RemovedRepoResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Repo not found in this org"),
    )
)]
pub(crate) async fn repo_remove(
    State(state): State<Arc<AppState>>,
    Path((_project_id, repo_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<RemovedRepoResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let removed =
        brainiac_store::projects::remove_repo(state.store.pool(), ctx.principal.org_id, repo_id)
            .await
            .map_err(internal)?;
    if !removed {
        return Err((StatusCode::NOT_FOUND, "repo not found".to_string()).into());
    }
    Ok(Json(RemovedRepoResponse {
        id: repo_id,
        removed: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::normalize_remote;

    #[test]
    fn every_spelling_of_a_remote_collides() {
        let want = Some("github.com/acme/payments".to_string());
        for raw in [
            "https://github.com/acme/payments",
            "https://github.com/acme/payments.git",
            "https://github.com/Acme/payments/",
            "http://github.com/acme/payments",
            "git@github.com:acme/payments.git",
            "git@GitHub.com:Acme/payments",
            "ssh://git@github.com/acme/payments.git",
            "git://github.com/acme/payments",
            "github.com/acme/payments",
        ] {
            assert_eq!(normalize_remote(raw), want, "raw: {raw}");
        }
    }

    #[test]
    fn repo_name_case_is_preserved_but_host_and_owner_fold() {
        assert_eq!(
            normalize_remote("https://GitHub.com/Acme/PayMents"),
            Some("github.com/acme/PayMents".into())
        );
    }

    #[test]
    fn gitlab_subgroups_keep_their_path() {
        assert_eq!(
            normalize_remote("https://gitlab.com/acme/platform/payments.git"),
            Some("gitlab.com/acme/platform/payments".into())
        );
        assert_eq!(
            normalize_remote("git@gitlab.com:acme/platform/payments.git"),
            Some("gitlab.com/acme/platform/payments".into())
        );
    }

    #[test]
    fn garbage_is_refused_not_guessed() {
        for raw in [
            "",
            "   ",
            "payments",
            "acme/payments",
            "https://github.com/acme",
            "file:///C:/repos/payments",
            "https://github.com/acme/pay ments",
        ] {
            assert_eq!(normalize_remote(raw), None, "raw: {raw:?}");
        }
    }

    // Self-hosted enterprise Git servers routinely live behind a bare
    // internal hostname with no public DNS suffix (e.g. `git-internal`,
    // resolved via an internal /etc/hosts or split-horizon DNS). Refusing
    // to onboard those repos just because the host lacks a dot blocked
    // legitimate enterprise use; the existing ≥3-segment / non-empty
    // owner+name guards already reject genuine garbage like bare
    // `payments` or `acme/payments`, so the dot requirement was the only
    // thing standing in the way and is dropped below.
    #[test]
    fn dot_less_self_hosted_host_is_accepted() {
        assert_eq!(
            normalize_remote("git@git-internal:team/repo"),
            Some("git-internal/team/repo".into())
        );
        assert_eq!(
            normalize_remote("https://gitserver/team/repo"),
            Some("gitserver/team/repo".into())
        );
    }

    #[test]
    fn scp_style_ssh_port_is_stripped_not_mistaken_for_owner() {
        assert_eq!(
            normalize_remote("git@host:2222/owner/name"),
            Some("host/owner/name".into())
        );
    }

    #[test]
    fn ssh_url_ssh_port_is_stripped_not_mistaken_for_owner() {
        assert_eq!(
            normalize_remote("ssh://host:2222/owner/name"),
            Some("host/owner/name".into())
        );
    }

    #[test]
    fn embedded_credentials_are_stripped() {
        assert_eq!(
            normalize_remote("https://user:pass@github.com/acme/payments.git"),
            Some("github.com/acme/payments".into())
        );
    }
}
