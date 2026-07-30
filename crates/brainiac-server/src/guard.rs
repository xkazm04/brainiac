//! The write-path guard — knowledge that arrives whether or not the agent asked.
//!
//! Everything else in this crate answers questions. `tool_definitions()` reads,
//! honestly, as a plea: *"Reach for this MID-TASK, not just at the start"*
//! (mcp.rs), *"Organizational knowledge does not announce itself — if you never
//! ask, you build as if the org knows nothing"*. That second sentence is the
//! product's ceiling stated in its own source: every governance investment
//! behind it — the candidate floor, contested withholding, provenance,
//! canonical-only bundles — only pays out on the calls an agent elects to make.
//! A real session performs tens of edits and, generously, one to three searches.
//!
//! This module inverts that. An agent hook calls `POST /v1/guard/pre-edit` on
//! the way *into* every Edit/Write, and the org's pitfalls, decisions and
//! mandatory standards for the touched project arrive unbidden. The same
//! computation is exposed as the `edit_preflight` MCP tool for clients with no
//! hook system, so the REST/MCP parity invariant this crate holds is preserved.
//!
//! Three rules govern it, and they are what keep it installed past day two:
//!
//! 1. **It never blocks.** The response is advisory. LIBRARY-PLAN Never #3 is
//!    relaxed only as far as its own condition allows — "report, then warn,
//!    never silently block at first".
//! 2. **It fails open.** A guard that stalls an edit gets uninstalled. The CLI
//!    exits 0 with no context on any error, timeout or unreachable server.
//! 3. **It is not demand.** Preflights are deliberately NOT written to the
//!    retrieval ledger (0043): they are automatic, one per edit, and mixing them
//!    into the denominator would drown the answered-rate for questions an agent
//!    actually asked. What the guard delivers is measured separately or not at
//!    all — never by corrupting the demand metric.

use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use brainiac_core::{Enforcement, MemoryKind, MemoryStatus, StandardLifecycle};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, AppState, HttpError};

/// Cap on the diff excerpt a hook may send. The guard is a retrieval query, not
/// an ingest path — a whole diff would be both a slow embed and an accidental
/// content channel into a read endpoint.
const MAX_EXCERPT_CHARS: usize = 4_000;
/// Cap on how many changed paths one edit may name.
const MAX_PATHS: usize = 64;
/// How many memories the preflight surfaces. A hook's output is read by a model
/// mid-task with a budget; twenty pitfalls is noise that trains it to skip.
const PREFLIGHT_K: usize = 6;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/guard/pre-edit", post(pre_edit))
}

#[derive(Deserialize, ToSchema)]
pub struct PreEditBody {
    /// The checkout's git remote, in any spelling — normalized server-side, the
    /// same way onboarding normalizes it.
    pub remote: String,
    /// Paths the agent is about to change, repo-relative. The first one resolves
    /// the project in a monorepo.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// The tool about to run (`Edit`, `Write`, `MultiEdit`) — carried for
    /// telemetry and future per-tool policy; it does not change the answer.
    #[serde(default)]
    pub tool: Option<String>,
    /// A bounded excerpt of what is about to change. Optional: without it the
    /// query is built from the paths alone, which still resolves the project and
    /// its standards.
    #[serde(default)]
    pub diff_excerpt: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PreEditNote {
    /// `pitfall` | `decision` | `standard`.
    pub kind: String,
    /// The one-sentence claim, verbatim — never re-typed by a model.
    pub statement: String,
    /// The memory this came from, so the agent can cite or fetch provenance.
    /// `null` for a standard, which carries `slug` instead.
    pub memory_id: Option<Uuid>,
    /// The standard's deep-linkable slug; `null` for a memory.
    pub slug: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PreEditResponse {
    /// Always `allow` today. The field exists so a future `guard_mode` can warn
    /// or block without a shape change — and so a client written now already
    /// reads the decision instead of assuming it.
    pub decision: String,
    /// The project the remote+path resolved to; `null` when the repo is not
    /// registered, which is not an error — the org's cross-cutting standards
    /// still apply and are returned.
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub notes: Vec<PreEditNote>,
}

#[utoipa::path(
    post,
    path = "/v1/guard/pre-edit",
    tag = "guard",
    request_body = PreEditBody,
    description = "What the org already knows about the code an agent is about to change: \
                   canonical pitfalls and decisions anchored to the touched project, plus its \
                   mandatory standards. Advisory only — the decision is always `allow`. Called \
                   by `brainiac hook pre-edit` from an agent's PreToolUse hook, or as the \
                   `edit_preflight` MCP tool. Deliberately not recorded as retrieval demand.",
    responses(
        (status = 200, description = "Advisory notes for the pending edit", body = PreEditResponse),
        (status = 400, description = "Unusable remote"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
    )
)]
pub async fn pre_edit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PreEditBody>,
) -> Result<Json<PreEditResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "read").await?;
    let out = preflight(
        &state.store,
        state.embedder.as_ref(),
        state.embedding_version,
        &ctx.principal,
        body,
    )
    .await
    .map_err(|e| match e {
        PreflightError::BadRemote => HttpError::from((
            axum::http::StatusCode::BAD_REQUEST,
            "unusable remote".to_string(),
        )),
        PreflightError::Internal(e) => internal(e),
    })?;
    Ok(Json(out))
}

/// Why a preflight could not be computed. Two cases only, because each surface
/// maps them differently: REST answers 400/500, MCP answers a tool error.
#[derive(Debug)]
pub enum PreflightError {
    BadRemote,
    Internal(anyhow::Error),
}

/// The computation behind both surfaces (REST here, `edit_preflight` in mcp.rs).
/// Kept in one function so the two can never drift — the same discipline
/// `tool_scope` applies to authorization.
pub async fn preflight(
    store: &brainiac_store::Store,
    embedder: &dyn brainiac_core::embed::Embedder,
    embedding_version: i32,
    principal: &brainiac_core::Principal,
    body: PreEditBody,
) -> Result<PreEditResponse, PreflightError> {
    let Some(remote) = crate::projects::normalize_remote(&body.remote) else {
        return Err(PreflightError::BadRemote);
    };

    // The project comes from remote + the first changed path: in a monorepo the
    // path is what distinguishes `apps/web` from `apps/api`, and `find_by_remote`
    // owns the segment-aware longest-prefix match (never re-implement it here).
    let probe_path = body
        .changed_paths
        .first()
        .map(|p| crate::projects::normalize_path_prefix(p))
        .unwrap_or_default();
    let matched = brainiac_store::projects::find_by_remote(
        store.pool(),
        principal.org_id,
        &remote,
        &probe_path,
    )
    .await
    .map_err(PreflightError::Internal)?;

    // The query: what the edit is about. Paths carry the identifiers that
    // resolve to canonical entities (`payments-api/src/refund.rs` → refund), and
    // the excerpt adds the symbols actually being touched.
    let mut query = body
        .changed_paths
        .iter()
        .take(MAX_PATHS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(excerpt) = &body.diff_excerpt {
        query.push(' ');
        query.push_str(
            excerpt
                .chars()
                .take(MAX_EXCERPT_CHARS)
                .collect::<String>()
                .as_str(),
        );
    }
    let query = query.trim().to_string();

    let mut notes = Vec::new();
    let mut tx = store
        .scoped_tx(principal)
        .await
        .map_err(PreflightError::Internal)?;

    // Pitfalls and decisions only, canonical only. A `fact` is not worth
    // interrupting an edit for; a pitfall is the entire point ("we shipped that
    // in March and reverted it"), and a decision is what an agent is most likely
    // to unknowingly contradict. The floor is Canonical because unbidden context
    // must be org-certified — pushing a candidate at someone who did not ask is
    // how a guard earns distrust.
    if !query.is_empty() {
        let hits = brainiac_store::retrieval::search(
            &mut tx,
            store.pool(),
            embedder,
            embedding_version,
            &brainiac_store::retrieval::RetrievalRequest {
                query,
                k: PREFLIGHT_K,
                as_of: None,
                filters: brainiac_store::retrieval::RetrievalFilters {
                    kinds: vec![MemoryKind::Pitfall, MemoryKind::Decision],
                    min_status: Some(MemoryStatus::Canonical),
                    project_id: matched.as_ref().map(|(id, _)| *id),
                    ..Default::default()
                },
            },
        )
        .await
        .map_err(PreflightError::Internal)?;
        for h in hits {
            notes.push(PreEditNote {
                kind: h.memory.kind.as_str().to_string(),
                statement: h.memory.content,
                memory_id: Some(h.memory.id),
                slug: None,
            });
        }
    }

    // Mandatory standards are unconditional: they apply to the edit whether or
    // not anything semantically matched, which is exactly what "mandatory" means.
    // Recommended ones are deliberately left out — a hook that reports every
    // preference is a hook that gets muted.
    let standards =
        brainiac_store::library::list_standards(&mut tx, None, Some(StandardLifecycle::Adopted))
            .await
            .map_err(PreflightError::Internal)?;
    for s in standards
        .into_iter()
        .filter(|s| s.enforcement == Enforcement::Mandatory)
    {
        notes.push(PreEditNote {
            kind: "standard".to_string(),
            statement: s.statement,
            memory_id: None,
            slug: Some(s.slug),
        });
    }
    tx.commit()
        .await
        .map_err(|e| PreflightError::Internal(e.into()))?;

    Ok(PreEditResponse {
        decision: "allow".to_string(),
        project_id: matched.as_ref().map(|(id, _)| *id),
        project_name: matched.map(|(_, name)| name),
        notes,
    })
}

/// Render the preflight as the text an agent reads mid-task.
///
/// Shared by the CLI hook and the MCP tool so the wording is identical on both
/// paths. Empty string when there is nothing to say — the caller then emits no
/// context at all, rather than "no relevant knowledge found", which trains a
/// model to ignore the channel.
pub fn render_notes(r: &PreEditResponse) -> String {
    if r.notes.is_empty() {
        return String::new();
    }
    let mut out = String::from("Organizational knowledge about the code you are changing");
    if let Some(p) = &r.project_name {
        out.push_str(&format!(" (project: {p})"));
    }
    out.push_str(":\n");
    for n in &r.notes {
        match (&n.memory_id, &n.slug) {
            (Some(id), _) => out.push_str(&format!(
                "- {}: {} (memory:{id})\n",
                n.kind.to_uppercase(),
                n.statement
            )),
            (None, Some(slug)) => {
                out.push_str(&format!("- MANDATORY STANDARD {slug}: {}\n", n.statement))
            }
            _ => {}
        }
    }
    out.push_str(
        "These are canonical — the org certified them. Cite the ids if you rely on them; \
         if you are about to contradict one, say so explicitly rather than silently.\n",
    );
    out
}
