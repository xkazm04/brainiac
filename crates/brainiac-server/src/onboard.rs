//! Developer onboarding pairing — the device-authorization pattern, hosted by
//! Brainiac itself (docs/ONBOARDING-QUICK-ACTION.md grew into this; the
//! brainiac-onboard skill drives it from inside Claude Code).
//!
//! THE FLOW, and where the trust lives:
//!
//! 1. `POST /v1/onboard/start` (UNAUTHENTICATED — acquiring a credential is
//!    the point) takes a normalized git remote + a display label and returns
//!    two codes: a short `user_code` the human matches by eye in the console,
//!    and a long `device_code` the CLI polls with (stored sha256, never
//!    plaintext — a DB read must not yield a pollable credential).
//! 2. An operator approves the request in the console (admin scope). Approval
//!    is where the whitelist bites: the remote must be registered under a
//!    project (crate::projects), and the project is DERIVED from the remote —
//!    the operator confirms, they don't choose, so a key can never land in
//!    the wrong project by mis-click.
//! 3. The CLI's next poll CLAIMS the request — a single-shot UPDATE — and only
//!    then is the key minted: project-scoped, read+write, never admin. The
//!    secret exists in exactly one HTTP response, in transit to the machine
//!    that will hold it. Nothing here stores it; nothing can re-serve it.
//!
//! What this proves (phase 1): possession of an authenticated console session
//! approved this specific remote. What it does NOT prove: that the requester
//! can push to that repo — that is the documented phase-2 upgrade (GitHub App
//! access check at approval time), which changes this module's approve step
//! and nothing else.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, AppState, HttpError};
use crate::projects::{normalize_path_prefix, normalize_remote};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/onboard/start", post(onboard_start))
        .route("/v1/onboard/poll", post(onboard_poll))
        .route("/v1/onboard/skill", get(onboard_skill))
        .route("/v1/onboard/requests", get(requests_list))
        .route("/v1/onboard/requests/{id}/approve", post(request_approve))
        .route("/v1/onboard/requests/{id}/deny", post(request_deny))
}

/// Pairing lifetime. Long enough for "switch to the browser, find the
/// console, click approve"; short enough that an abandoned code dies today.
const PAIRING_TTL_SECS: i64 = 900;
/// What the start response tells the CLI to sleep between polls.
const POLL_INTERVAL_SECS: i64 = 5;
/// Unexpired-pending ceiling. `start` is unauthenticated; this cap is what
/// bounds an anonymous flood (each row dies in 15 minutes, so the steady
/// state an attacker can hold is exactly this many rows).
const MAX_PENDING: i64 = 100;
/// Display label cap — a hostname/username, not prose.
const MAX_LABEL_CHARS: usize = 100;
/// Raw remote cap before normalization.
const MAX_REMOTE_CHARS: usize = 200;

/// The scopes an onboarded device key gets. Same rule as provisioning device
/// keys: never `admin` — a leaked laptop key must not mint more keys.
const ONBOARD_SCOPES: [&str; 2] = ["read", "write"];

/// User-code alphabet: no 0/O, 1/I/L, 5/S, 8/B — the human compares this code
/// across a terminal and a browser, so every glyph must survive both fonts.
const CODE_ALPHABET: &[u8] = b"ACDEFGHJKMNPQRTUVWXY234679";

fn mint_user_code() -> String {
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(8)
        .map(|b| CODE_ALPHABET[(*b as usize) % CODE_ALPHABET.len()] as char)
        .collect()
}

/// Device-code secret: same construction as API-token secrets (auth.rs
/// mint_secret) with its own prefix so a leaked one is recognizable.
fn mint_device_code() -> String {
    format!("obc_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn console_url() -> String {
    std::env::var("BRAINIAC_CONSOLE_URL").unwrap_or_else(|_| "http://127.0.0.1:3100".into())
}

// ── DTOs ────────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct OnboardStartBody {
    /// The checkout's `git remote get-url origin`, any spelling.
    pub remote: String,
    /// Who is asking — hostname/username, shown to the approver.
    #[serde(default)]
    pub label: Option<String>,
    /// The checkout subdir relative to the repo root (e.g. `apps/web`), for
    /// monorepos split across projects by path_prefix. Omit (or `""`) for a
    /// whole-repo checkout — the default, back-compat with every
    /// pre-monorepo caller.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OnboardStartResponse {
    /// Short code the approver matches by eye in the console.
    pub user_code: String,
    /// Long secret the CLI polls with. Appears only here.
    pub device_code: String,
    /// The normalized remote the request was recorded under.
    pub remote: String,
    /// Where a human approves this — the console's projects module.
    pub verification_url: String,
    pub expires_in_secs: i64,
    pub poll_interval_secs: i64,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct OnboardPollBody {
    pub device_code: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OnboardPollResponse {
    /// pending | approved | denied | expired | claimed
    pub status: String,
    /// The minted key — present exactly once, on the poll that claims an
    /// approved request. Never retrievable again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OnboardRequestView {
    pub id: Uuid,
    pub user_code: String,
    pub remote: String,
    pub label: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// The project this remote would land in (whitelist match in the caller's
    /// org), or null — meaning approval will 409 until the repo is registered.
    pub project_name: Option<String>,
    pub project_id: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OnboardRequestsResponse {
    pub requests: Vec<OnboardRequestView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OnboardDecisionResponse {
    pub id: Uuid,
    /// approved | denied
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

// ── handlers ────────────────────────────────────────────────────────────

/// POST /v1/onboard/start — open a pairing request (unauthenticated).
#[utoipa::path(
    post,
    path = "/v1/onboard/start",
    tag = "onboard",
    description = "Open an onboarding pairing request for a git remote. Unauthenticated — \
                   this is how a developer with no credentials acquires one. Returns a short \
                   user_code (matched by eye in the console) and a long device_code to poll with.",
    request_body = OnboardStartBody,
    responses(
        (status = 201, description = "Pairing opened", body = OnboardStartResponse),
        (status = 400, description = "Remote is not a recognizable git remote URL"),
        (status = 429, description = "Too many pending pairing requests; try again shortly"),
    )
)]
pub(crate) async fn onboard_start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OnboardStartBody>,
) -> Result<(StatusCode, Json<OnboardStartResponse>), HttpError> {
    if body.remote.chars().count() > MAX_REMOTE_CHARS {
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
    let label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unnamed device");
    let label: String = label.chars().take(MAX_LABEL_CHARS).collect();
    let path = normalize_path_prefix(body.path.as_deref().unwrap_or(""));

    let pool = state.store.pool();
    // Hygiene first, gauge second: a flood yesterday must not brick today.
    let _ = brainiac_store::onboard::prune_expired(pool).await;
    let pending = brainiac_store::onboard::pending_count(pool)
        .await
        .map_err(internal)?;
    if pending >= MAX_PENDING {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "too many pending onboarding requests; try again in a few minutes".to_string(),
        )
            .into());
    }

    let user_code = mint_user_code();
    let device_code = mint_device_code();
    brainiac_store::onboard::start(
        pool,
        Uuid::new_v4(),
        &user_code,
        &crate::auth::hash_token(&device_code),
        &remote,
        &label,
        &path,
        PAIRING_TTL_SECS,
    )
    .await
    .map_err(internal)?;

    tracing::info!(remote = %remote, label = %label, "onboarding pairing opened");
    Ok((
        StatusCode::CREATED,
        Json(OnboardStartResponse {
            user_code,
            device_code,
            remote,
            verification_url: format!("{}/console?m=projects", console_url()),
            expires_in_secs: PAIRING_TTL_SECS,
            poll_interval_secs: POLL_INTERVAL_SECS,
        }),
    ))
}

/// POST /v1/onboard/poll — the CLI's wait loop; the approving poll mints.
#[utoipa::path(
    post,
    path = "/v1/onboard/poll",
    tag = "onboard",
    description = "Poll a pairing request by device_code. While pending returns \
                   {status: \"pending\"}; the first poll after approval claims the request \
                   and returns the minted project-scoped key EXACTLY ONCE.",
    request_body = OnboardPollBody,
    responses(
        (status = 200, description = "Current pairing state (token present exactly once, on claim)", body = OnboardPollResponse),
        (status = 404, description = "Unknown device code"),
    )
)]
pub(crate) async fn onboard_poll(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OnboardPollBody>,
) -> Result<Json<OnboardPollResponse>, HttpError> {
    let pool = state.store.pool();
    let hash = crate::auth::hash_token(&body.device_code);
    let Some(req) = brainiac_store::onboard::get_by_device_hash(pool, &hash)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "unknown device code".to_string()).into());
    };

    let expired = req.expires_at < chrono::Utc::now();
    let bare = |status: &str| {
        Json(OnboardPollResponse {
            status: status.to_string(),
            token: None,
            project_id: None,
            project_name: None,
            org_id: None,
        })
    };
    match (req.status.as_str(), expired) {
        ("pending", false) => return Ok(bare("pending")),
        ("pending", true) | ("approved", true) => return Ok(bare("expired")),
        ("denied", _) => return Ok(bare("denied")),
        ("claimed", _) => return Ok(bare("claimed")),
        ("approved", false) => {}
        (other, _) => {
            return Err(internal(anyhow::anyhow!("impossible pairing status {other:?}")).into())
        }
    }

    // Claim is a single-shot UPDATE: two racing polls mint at most one key.
    let Some(claimed) = brainiac_store::onboard::claim(pool, &hash)
        .await
        .map_err(internal)?
    else {
        // Lost the race (or expired between read and claim): report the truth.
        return Ok(bare("claimed"));
    };
    let (org_id, project_id, approved_by) =
        match (claimed.org_id, claimed.project_id, claimed.approved_by) {
            (Some(o), Some(p), Some(u)) => (o, p, u),
            _ => {
                return Err(internal(anyhow::anyhow!(
                    "approved pairing {} is missing org/project/approver",
                    claimed.id
                ))
                .into())
            }
        };
    let project_name = brainiac_store::projects::list(pool, org_id)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|p| p.id == project_id)
        .map(|p| p.name);

    let (secret, prefix) = crate::auth::mint_secret();
    let repo_name = claimed.remote.rsplit('/').next().unwrap_or("repo");
    brainiac_store::tokens::create(
        pool,
        Uuid::new_v4(),
        org_id,
        // The key acts as the approving operator — the human who signed.
        approved_by,
        &format!("onboard · {} · {}", claimed.label, repo_name),
        &prefix,
        &crate::auth::hash_token(&secret),
        &ONBOARD_SCOPES.map(str::to_string),
        Some(project_id),
        approved_by,
    )
    .await
    .map_err(internal)?;

    tracing::info!(
        remote = %claimed.remote,
        label = %claimed.label,
        org_id = %org_id,
        project_id = %project_id,
        "onboarding pairing claimed — device key minted"
    );
    Ok(Json(OnboardPollResponse {
        status: "approved".to_string(),
        token: Some(secret),
        project_id: Some(project_id),
        project_name,
        org_id: Some(org_id),
    }))
}

/// GET /v1/onboard/requests — the console's approval queue (admin).
#[utoipa::path(
    get,
    path = "/v1/onboard/requests",
    tag = "onboard",
    description = "Pending pairing requests with their whitelist match in the caller's org \
                   (admin). A null project_name means approval will be refused until the \
                   remote is registered under a project.",
    responses(
        (status = 200, description = "Pending requests, oldest first", body = OnboardRequestsResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn requests_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<OnboardRequestsResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let pool = state.store.pool();
    let rows = brainiac_store::onboard::list_pending(pool)
        .await
        .map_err(internal)?;
    let mut requests = Vec::with_capacity(rows.len());
    for r in rows {
        let matched = brainiac_store::projects::find_by_remote(
            pool,
            ctx.principal.org_id,
            &r.remote,
            &r.path,
        )
        .await
        .map_err(internal)?;
        requests.push(OnboardRequestView {
            id: r.id,
            user_code: r.user_code,
            remote: r.remote,
            label: r.label,
            created_at: r.created_at,
            expires_at: r.expires_at,
            project_id: matched.as_ref().map(|(id, _)| *id),
            project_name: matched.map(|(_, name)| name),
        });
    }
    Ok(Json(OnboardRequestsResponse { requests }))
}

/// POST /v1/onboard/requests/{id}/approve — approve into the whitelist match.
#[utoipa::path(
    post,
    path = "/v1/onboard/requests/{id}/approve",
    tag = "onboard",
    description = "Approve a pending pairing (admin). The project is DERIVED from the \
                   whitelist — the remote must already be registered under a project in the \
                   caller's org, or this refuses with 409.",
    params(("id" = Uuid, Path, description = "Pairing request id")),
    responses(
        (status = 200, description = "Approved; the CLI's next poll mints the key", body = OnboardDecisionResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Request not found, expired, or already decided"),
        (status = 409, description = "Remote not whitelisted under any project in this org"),
    )
)]
pub(crate) async fn request_approve(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<OnboardDecisionResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let pool = state.store.pool();
    let Some(req) = brainiac_store::onboard::get(pool, id)
        .await
        .map_err(internal)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            "pairing request not found".to_string(),
        )
            .into());
    };
    let Some((project_id, project_name)) = brainiac_store::projects::find_by_remote(
        pool,
        ctx.principal.org_id,
        &req.remote,
        &req.path,
    )
    .await
    .map_err(internal)?
    else {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "{} is not whitelisted under any project — register it in the Projects \
                 module first",
                req.remote
            ),
        )
            .into());
    };
    let approved = brainiac_store::onboard::approve(
        pool,
        id,
        ctx.principal.org_id,
        project_id,
        ctx.principal.user_id,
    )
    .await
    .map_err(internal)?;
    if !approved {
        return Err((
            StatusCode::NOT_FOUND,
            "pairing request expired or already decided".to_string(),
        )
            .into());
    }
    tracing::info!(
        pairing = %id,
        remote = %req.remote,
        project_id = %project_id,
        by = %ctx.principal.user_id,
        "onboarding pairing approved"
    );
    Ok(Json(OnboardDecisionResponse {
        id,
        status: "approved".to_string(),
        project_id: Some(project_id),
        project_name: Some(project_name),
    }))
}

/// POST /v1/onboard/requests/{id}/deny — refuse a pending pairing.
#[utoipa::path(
    post,
    path = "/v1/onboard/requests/{id}/deny",
    tag = "onboard",
    description = "Deny a pending pairing request (admin). The polling CLI is told \
                   {status: \"denied\"} and stops.",
    params(("id" = Uuid, Path, description = "Pairing request id")),
    responses(
        (status = 200, description = "Denied", body = OnboardDecisionResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Request not found or already decided"),
    )
)]
pub(crate) async fn request_deny(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<OnboardDecisionResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let denied = brainiac_store::onboard::deny(state.store.pool(), id)
        .await
        .map_err(internal)?;
    if !denied {
        return Err((
            StatusCode::NOT_FOUND,
            "pairing request not found or already decided".to_string(),
        )
            .into());
    }
    tracing::info!(pairing = %id, by = %ctx.principal.user_id, "onboarding pairing denied");
    Ok(Json(OnboardDecisionResponse {
        id,
        status: "denied".to_string(),
        project_id: None,
        project_name: None,
    }))
}

/// GET /v1/onboard/skill — the brainiac-onboard skill, served for the
/// copyable bootstrap command in the Keys module.
#[utoipa::path(
    get,
    path = "/v1/onboard/skill",
    tag = "onboard",
    description = "The brainiac-onboard Claude Code skill (markdown). Open, like /health: \
                   a developer fetches this BEFORE they have a token — the skill is how \
                   they get one. Canonical source: docs/skills/brainiac-onboard/SKILL.md.",
    responses((status = 200, description = "The skill, as markdown")),
)]
pub(crate) async fn onboard_skill() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        include_str!("../../../docs/skills/brainiac-onboard/SKILL.md"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_codes_use_the_unambiguous_alphabet() {
        for _ in 0..50 {
            let code = mint_user_code();
            assert_eq!(code.len(), 8);
            assert!(
                code.bytes().all(|b| CODE_ALPHABET.contains(&b)),
                "code: {code}"
            );
        }
    }

    #[test]
    fn device_codes_are_prefixed_unique_and_hashable() {
        let a = mint_device_code();
        let b = mint_device_code();
        assert!(a.starts_with("obc_"));
        assert_ne!(a, b);
        assert_eq!(crate::auth::hash_token(&a).len(), 32);
    }
}
