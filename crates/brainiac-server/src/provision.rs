//! Self-serve onboarding — a verified identity in, a project + a device key out.
//!
//! THE SEAM THIS SITS ON. The console has two ways in, and they are not rivals:
//!
//! - The shared **passcode** gate (`console/src/lib/auth.ts`) is the operator
//!   console: one secret per deployment, one trusted operator, a privileged env
//!   token behind it. It stays exactly as it is.
//! - This is the **free tier**: a person signs in with Google, gets ONE project of
//!   their own, and a key their local device (the MCP agent) uses to reach it.
//!   The paid multi-user company tier grows from here — see migration 0022 for why
//!   the "one project" rule lives on the identity rather than on orgs.
//!
//! WHY THE ADMIN POOL. Creating a new org cannot happen under a tenant's
//! `scoped_tx`: RLS scopes every write to the caller's org, and someone signing up
//! has no org yet. Both the project and its first key are therefore written on the
//! RLS-bypassing owner pool. This is also why `POST /v1/tokens` cannot serve this
//! flow — it mints strictly for `ctx.principal.org_id`, i.e. the CONSOLE's org,
//! never the org that was just created.
//!
//! WHAT THIS ENDPOINT TRUSTS. It requires the `admin` scope, so only the console's
//! bootstrap token can call it, and it takes the identity as data: nothing here can
//! tell a real Google uid from a made-up string. The console verifies the Firebase
//! ID token (server-side, against Google's public keys) BEFORE calling. That is a
//! deliberate delegation, and it is not a widening: the admin token could already
//! do anything this does. Moving verification into the server (JWKS in Rust) is the
//! obvious hardening once this stops being stage 1.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, AppState, HttpError};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/provision", post(provision_project))
}

/// A verified federated identity, plus what to do with it.
#[derive(Deserialize, ToSchema)]
pub(crate) struct ProvisionBody {
    /// Identity provider. Only `google` today.
    pub provider: String,
    /// The provider's stable subject — the Firebase uid. NOT the email: emails get
    /// reassigned and change case, uids do not.
    pub subject: String,
    /// For display and support only; never the identity.
    pub email: String,
    /// What to call the project. Defaults to the email's local part.
    #[serde(default)]
    pub project_name: Option<String>,
    /// Mint a fresh device key. Always true in effect on first creation (a project
    /// with no key cannot be reached); on a returning identity it is opt-in, so a
    /// page refresh does not quietly mint keys.
    #[serde(default)]
    pub issue_key: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProvisionResponse {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub team_id: Uuid,
    /// False when the identity already had a project. Signing in twice is not an
    /// error — it just does not make a second project.
    pub created: bool,
    /// The device key, in the ONLY response that will ever contain it — only its
    /// sha256 is stored. Null when no key was issued this call.
    pub api_key: Option<String>,
    /// The key's display prefix, safe to persist and show in a list.
    pub api_key_prefix: Option<String>,
}

/// The scopes a local device gets: read + write its own project's memories, never
/// `admin` (which mints tokens). A leaked device key must not be able to issue
/// more keys.
const DEVICE_SCOPES: [&str; 2] = ["read", "write"];

/// POST /v1/provision — exchange a verified identity for its project.
///
/// Idempotent by construction: the identity's PK (migration 0022) means a second
/// call returns the same project with `created: false`, and two concurrent first
/// sign-ins collide on the key so exactly one project exists.
#[utoipa::path(
    post,
    path = "/v1/provision",
    tag = "provision",
    description = "Exchange a VERIFIED federated identity for its single project, optionally minting a device key. Requires the `admin` scope; the caller must have already verified the identity with the provider.",
    request_body = ProvisionBody,
    responses(
        (status = 200, description = "Project provisioned or already existed", body = ProvisionResponse),
        (status = 400, description = "Unsupported provider, or empty subject/email"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn provision_project(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ProvisionBody>,
) -> Result<Json<ProvisionResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;

    if body.provider != "google" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported provider {:?} (only `google`)", body.provider),
        )
            .into());
    }
    let subject = body.subject.trim();
    let email = body.email.trim();
    if subject.is_empty() || email.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "`subject` and `email` are required".to_string(),
        )
            .into());
    }
    let project_name = body
        .project_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| email.split('@').next().unwrap_or("workspace").to_string());

    // Owner pool: a brand-new org has no RLS scope to write under.
    let mut tx = state.admin_pool.begin().await.map_err(internal)?;
    let p = brainiac_store::identities::provision(&mut tx, "google", subject, email, &project_name)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    // A project with no key is unreachable from the device it was made for, so the
    // first one is always issued; afterwards it is opt-in.
    let (api_key, api_key_prefix) = if p.created || body.issue_key {
        let (secret, prefix) = crate::auth::mint_secret();
        brainiac_store::tokens::create(
            &state.admin_pool,
            Uuid::new_v4(),
            p.org_id,
            p.user_id,
            "device key",
            &prefix,
            &crate::auth::hash_token(&secret),
            &DEVICE_SCOPES.map(str::to_string),
            // Org-wide: the free-tier project IS the org; project scoping
            // arrives via the onboarding pairing flow (crate::onboard).
            None,
            // Attributed to the person who owns the project, not to the console's
            // bootstrap principal — the audit trail should name the human.
            p.user_id,
        )
        .await
        .map_err(internal)?;
        (Some(secret), Some(prefix))
    } else {
        (None, None)
    };

    tracing::info!(
        org_id = %p.org_id,
        created = p.created,
        issued_key = api_key.is_some(),
        by = %ctx.principal.user_id,
        "provisioned self-serve project"
    );

    Ok(Json(ProvisionResponse {
        org_id: p.org_id,
        user_id: p.user_id,
        team_id: p.team_id,
        created: p.created,
        api_key,
        api_key_prefix,
    }))
}
