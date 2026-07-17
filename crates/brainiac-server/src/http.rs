//! REST surface v0 (ARCHITECTURE.md §5.2, minimal slice):
//! - GET  /health
//! - POST /v1/memories/search   — hybrid retrieval under the caller's RLS
//! - POST /v1/memories          — memory_add: source + pipeline enqueue (202)
//! - GET  /v1/reviews/promotions — pending review queue
//!
//! Every handler resolves the bearer token to a principal FIRST; there is no
//! anonymous data path.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainiac_core::embed::Embedder;
use brainiac_core::rerank::Reranker;
use brainiac_core::Principal;
use brainiac_store::Store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::TokenMap;

pub struct AppState {
    pub store: Store,
    pub embedder: Arc<dyn Embedder>,
    /// Optional stage-5 reranker (ARCHITECTURE.md §4). `None` ⇒ retrieval takes
    /// the byte-identical pre-stage-5 path.
    pub reranker: Option<Arc<dyn Reranker>>,
    pub embedding_version: i32,
    pub tokens: TokenMap,
    /// RLS-bypassing owner pool. Two callers, and both are cases where a
    /// tenant-scoped transaction structurally cannot do the work:
    ///
    /// - Org-level analytics (Knowledge Health) at their TRUE org totals — a
    ///   leadership metric must not depend on which team the viewer belongs to.
    /// - Self-serve provisioning (`crate::provision`) — a person signing up has no
    ///   org yet, so there is no RLS scope to create one under.
    ///
    /// Never route request-scoped reads/writes of tenant content through it; those
    /// stay on `store.scoped_tx`.
    pub admin_pool: sqlx::PgPool,
}

pub async fn router(
    store: Store,
    embedder: Arc<dyn Embedder>,
    reranker: Option<Arc<dyn Reranker>>,
) -> Result<Router> {
    let tokens = TokenMap::from_env()?;
    if tokens.is_empty() {
        tracing::warn!("BRAINIAC_TOKENS is empty — every request will be 401");
    }
    let embedding_version = {
        let principal = brainiac_pipeline::pipeline_principal(Uuid::nil());
        let mut tx = store.scoped_tx(&principal).await?;
        // Serve path: require the version be fully backfilled (is_active). An
        // interrupted reembed leaves its target version inactive, so this refuses
        // to serve a half-embedded corpus instead of silently under-answering.
        let v = brainiac_store::memories::serving_embedding_version(
            &mut tx,
            embedder.model_name(),
            embedder.dim() as i32,
        )
        .await?;
        tx.commit().await?;
        v
    };
    // Owner (RLS-bypassing) pool for org-true analytics only. Built from the same
    // DATABASE_URL the runtime pool connects to; `admin_pool` differs only in that
    // it does NOT `SET ROLE brainiac_app`, so it sees every tenant's rows.
    let admin_pool = brainiac_store::admin_pool(
        &std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
    )
    .await?;
    let state = Arc::new(AppState {
        store,
        embedder,
        reranker,
        embedding_version,
        tokens,
        admin_pool,
    });
    Ok(Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(crate::openapi::openapi_json))
        .route("/v1/memories/search", post(search))
        .route("/v1/memories", post(memory_add))
        // Bulk import carries its own larger body limit (a full page of items
        // exceeds the single-statement global cap). The per-route layer sits
        // inside the global one, so it wins for this route only.
        .route(
            "/v1/memories/bulk",
            post(memory_add_bulk).layer(DefaultBodyLimit::max(BULK_MAX_BODY_BYTES)),
        )
        .route("/v1/memories/{id}/feedback", post(memory_feedback))
        .route("/v1/memories/{id}/provenance", get(memory_provenance))
        .route("/v1/sources/{id}", get(source_status))
        .route("/v1/reviews/promotions", get(pending_promotions))
        .route("/v1/tokens", get(list_tokens).post(create_token))
        .route("/v1/tokens/{id}/revoke", post(revoke_token))
        .route("/v1/queue/health", get(queue_health))
        .route("/v1/queue/dead-letters", get(queue_dead_letters))
        .route("/v1/queue/dead-letters/{id}/requeue", post(queue_requeue))
        .merge(crate::console::routes())
        .merge(crate::library::routes())
        .merge(crate::onboard::routes())
        .merge(crate::projects::routes())
        .merge(crate::provision::routes())
        // Explicit request-body cap. The largest free-text field REST accepts
        // is `memory_add` content (MAX_CONTENT_CHARS = 8000 chars ≈ 32 KiB of
        // UTF-8); 1 MiB leaves generous headroom for JSON framing and every
        // other body while still bounding what an unauthenticated peer can make
        // the server buffer (axum's silent ~2 MiB default is replaced by this).
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state))
}

/// Request-body ceiling for the whole REST router (see the `DefaultBodyLimit`
/// layer above).
const MAX_BODY_BYTES: usize = 1024 * 1024;

// ── input caps ──────────────────────────────────────────────────────────
// Mirrored from the MCP surface (mcp.rs `MAX_CONTENT_CHARS` / `MAX_QUERY_CHARS`)
// so REST and MCP reject oversized free text identically — a runaway caller can
// never hand either surface an unbounded blob to embed, store, or scan. Kept in
// sync by cross-reference; if the MCP consts move, move these too.
/// `memory_add` content — one self-contained statement, generously sized.
const MAX_CONTENT_CHARS: usize = 8_000;
/// `memory_search` query.
const MAX_QUERY_CHARS: usize = 2_000;
/// `memory_feedback` note — a short human explanation (mcp.rs MAX_NOTE_CHARS).
const MAX_NOTE_CHARS: usize = 2_000;
/// Bounded excerpt of a source's raw text on the provenance endpoint — a
/// citation handle, never the whole transcript (mcp.rs SOURCE_EXCERPT_CHARS).
const SOURCE_EXCERPT_CHARS: usize = 500;
/// `Idempotency-Key` header — an opaque client-chosen token (typically a UUID
/// or short hash). Bounded so a caller can't stash an unbounded blob in the
/// index. Scope is the org; lifetime is the source's lifetime.
const MAX_IDEMPOTENCY_KEY_CHARS: usize = 200;
/// Per-request item ceiling on `POST /v1/memories/bulk` — an org import is a
/// page, not the whole corpus. Each item's content still obeys
/// `MAX_CONTENT_CHARS`.
const MAX_BULK_ITEMS: usize = 100;
/// Body ceiling for the `/v1/memories/bulk` route specifically. The global
/// `MAX_BODY_BYTES` (1 MiB) is sized for a single statement; a full bulk page
/// (100 × 8000 chars, worst-case 4-byte UTF-8, plus JSON framing) needs more,
/// so the route carries its own larger `DefaultBodyLimit`.
const BULK_MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Enforce a documented character cap on a free-text field; oversized input is
/// a clear 400, never silent truncation or unbounded work.
fn within_cap(value: &str, cap: usize, field: &str) -> Result<(), HttpError> {
    let n = value.chars().count();
    if n > cap {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("`{field}` is too large ({n} chars); the limit is {cap}"),
        )
            .into());
    }
    Ok(())
}

/// Resolve the bearer token and require `scope` (env tokens pass all
/// scopes; `brk_…` API tokens carry what they were minted with).
pub(crate) async fn auth_of(
    state: &AppState,
    headers: &HeaderMap,
    scope: &str,
) -> Result<crate::auth::AuthContext, HttpError> {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;
    let ctx = crate::auth::resolve_bearer(&state.tokens, &state.store, bearer)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::UNAUTHORIZED, "unknown token".into()))?;
    if !ctx.allows(scope) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("token lacks the `{scope}` scope"),
        )
            .into());
    }
    Ok(ctx)
}

/// Read-scope principal — the default for query endpoints.
pub(crate) async fn principal_of(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, HttpError> {
    Ok(auth_of(state, headers, "read").await?.principal)
}

/// Liveness probe body — `{"status":"ok"}` and nothing else.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct HealthResponse {
    status: String,
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    description = "Liveness probe; unauthenticated.",
    responses((status = 200, description = "Server is up", body = HealthResponse))
)]
pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct SearchBody {
    query: String,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    as_of: Option<DateTime<Utc>>,
    /// Memory kinds to keep (fact|decision|pattern|pitfall|howto); omit for all.
    #[serde(default)]
    kinds: Vec<String>,
    /// Trust floor: raw|candidate|canonical (candidate ⇒ candidate+canonical).
    #[serde(default)]
    min_status: Option<String>,
    /// Restrict to one team's memories.
    #[serde(default)]
    team_id: Option<Uuid>,
    /// Minimum extractor confidence 0..1.
    #[serde(default)]
    min_confidence: Option<f32>,
    /// Project lens (PROJECT-PLAN PR1): keeps this project's memories PLUS
    /// org-shared ones (project_id null) — "my project + the org's
    /// conventions". Omit for the unfiltered org view.
    #[serde(default)]
    project_id: Option<Uuid>,
}

impl SearchBody {
    fn filters(&self) -> Result<brainiac_store::retrieval::RetrievalFilters, HttpError> {
        // Parse each wire string into the typed kind at the edge; the filter
        // downstream is typed, so an invalid kind can never reach the SQL.
        let mut kinds = Vec::with_capacity(self.kinds.len());
        for k in &self.kinds {
            let parsed = brainiac_core::MemoryKind::parse(k).ok_or((
                StatusCode::BAD_REQUEST,
                format!("unknown kind `{k}` (fact|decision|pattern|pitfall|howto)"),
            ))?;
            kinds.push(parsed);
        }
        let min_status = match self.min_status.as_deref() {
            None => None,
            Some(s) => Some(brainiac_core::MemoryStatus::parse(s).ok_or((
                StatusCode::BAD_REQUEST,
                format!("unknown min_status `{s}` (raw|candidate|canonical)"),
            ))?),
        };
        if let Some(c) = self.min_confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "min_confidence must be within 0..=1".into(),
                )
                    .into());
            }
        }
        Ok(brainiac_store::retrieval::RetrievalFilters {
            kinds,
            min_status,
            team_id: self.team_id,
            min_confidence: self.min_confidence,
            // Not validated against the org on purpose: this is a lens, not
            // attribution — an unknown id simply matches only org-shared rows,
            // and RLS already walls off other orgs.
            project_id: self.project_id,
        })
    }
}

fn default_k() -> usize {
    10
}

/// A canonical entity anchoring a hit (id + name).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct AnchorRef {
    id: Uuid,
    name: String,
}

/// What previous readers reported about a hit (mirrors the MCP search
/// `feedback` block, mcp.rs:544). Present only when the memory carries any
/// feedback at all — reuses [`brainiac_store::feedback::trust_for`].
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct HitFeedback {
    helpful: i64,
    wrong: i64,
    outdated: i64,
    /// True while an unresolved wrong/outdated claim stands — treat as unconfirmed.
    disputed: bool,
}

/// An OPEN contradiction touching a hit (mirrors the MCP search `contradicts`
/// entries, mcp.rs:557): the contradiction row and the memory it conflicts
/// with. Reuses [`brainiac_store::governance::open_contradictions_for`].
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct HitContradiction {
    contradiction_id: Uuid,
    counterpart_id: Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SearchHit {
    id: Uuid,
    content: String,
    kind: String,
    status: String,
    score: f64,
    via_graph: bool,
    provenance_id: Option<Uuid>,
    /// Canonical entities anchoring this hit; for via_graph hits, the bridge it
    /// surfaced through. Empty when the memory has no canonical-linked entities.
    anchors: Vec<AnchorRef>,
    /// Reader-reported trust signal — present only when this memory carries any
    /// feedback (omitted entirely otherwise, to keep payloads lean).
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback: Option<HitFeedback>,
    /// Open contradictions this memory is part of — omitted when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contradictions: Vec<HitContradiction>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[utoipa::path(
    post,
    path = "/v1/memories/search",
    tag = "memories",
    description = "Hybrid (vector + lexical + graph) retrieval under the caller's RLS scope.",
    request_body = SearchBody,
    responses(
        (status = 200, description = "Ranked hits (k capped at 50)", body = SearchResponse),
        (status = 400, description = "Unknown kind/min_status, or min_confidence outside 0..=1"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
    )
)]
pub(crate) async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SearchBody>,
) -> Result<Json<SearchResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    within_cap(&body.query, MAX_QUERY_CHARS, "query")?;
    let filters = body.filters()?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let hits = brainiac_store::retrieval::search_reranked(
        &mut tx,
        state.store.pool(),
        state.embedder.as_ref(),
        state.reranker.as_deref(),
        state.embedding_version,
        &brainiac_store::retrieval::RetrievalRequest {
            query: body.query,
            k: body.k.min(50),
            as_of: body.as_of,
            filters,
        },
    )
    .await
    .map_err(internal)?;
    // Trust + open-contradiction signals for the whole result set — the same
    // parity the MCP surface attaches (mcp.rs:523-529). Two batched, RLS-scoped
    // queries (never an N+1); the contradiction join is no-existence-oracle
    // safe (invisible counterparts drop out).
    let ids: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
    let trust = brainiac_store::feedback::trust_for(&mut tx, &ids)
        .await
        .map_err(internal)?;
    let mut contradictions = brainiac_store::governance::open_contradictions_for(&mut tx, &ids)
        .await
        .map_err(internal)?;
    let out: Vec<SearchHit> = hits
        .into_iter()
        .map(|h| {
            let id = h.memory.id;
            let feedback = trust
                .get(&id)
                .filter(|t| !t.is_empty())
                .map(|t| HitFeedback {
                    helpful: t.helpful,
                    wrong: t.wrong,
                    outdated: t.outdated,
                    disputed: t.disputed(),
                });
            let contradictions = contradictions
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|f| HitContradiction {
                    contradiction_id: f.contradiction_id,
                    counterpart_id: f.counterpart_id,
                })
                .collect();
            SearchHit {
                id,
                content: h.memory.content,
                kind: h.memory.kind.as_str().to_string(),
                status: h.memory.status.as_str().to_string(),
                score: h.score,
                via_graph: h.via_graph,
                provenance_id: h.memory.provenance_id,
                anchors: h
                    .anchors
                    .into_iter()
                    .map(|a| AnchorRef {
                        id: a.id,
                        name: a.name,
                    })
                    .collect(),
                feedback,
                contradictions,
            }
        })
        .collect();
    Ok(Json(SearchResponse { hits: out }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct MemoryAddBody {
    content: String,
    #[serde(default)]
    team_id: Option<Uuid>,
    /// Optional kind hint (fact|decision|pattern|pitfall|howto). For a `manual`
    /// add this is authoritative — the F-3 verbatim path stores the statement
    /// under exactly this kind (default `fact`) with no model guessing.
    #[serde(default)]
    kind: Option<String>,
    /// Optional entity names this concerns (services/repos/techs/features) —
    /// carried through to resolution so the memory anchors them.
    #[serde(default)]
    entities: Vec<String>,
    /// PROJECT-PLAN PR0: attribute this write to a project explicitly (must
    /// exist in the caller's org). Omitted ⇒ the key's own project scope, or
    /// org-shared for org-wide keys. An org-wide key (CI, imports) uses this
    /// to attribute correctly.
    #[serde(default)]
    project_id: Option<Uuid>,
}

/// Per-add entity-hint ceiling — a note anchors a handful of things, never a
/// bulk list (mirrors mcp.rs `MAX_ENTITY_HINTS`).
const MAX_ENTITY_HINTS: usize = 16;
/// Per-entity-name cap (mirrors mcp.rs `MAX_NAME_CHARS`).
const MAX_ENTITY_NAME_CHARS: usize = 200;

/// Validate + encode a `memory_add` body into the `manual` source text the
/// extractor stores. Content is validated (non-empty, capped) exactly as
/// before; the optional kind/entities are folded in via the ONE owner of the
/// wire format (`brainiac_pipeline::manual`), so REST, MCP, and the F-3 decode
/// path cannot drift. An unknown kind or an oversized/empty hint is a 400.
fn encode_add_source(body: &MemoryAddBody) -> Result<String, HttpError> {
    let content = body.content.trim();
    if content.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "content must not be empty".to_string(),
        )
            .into());
    }
    within_cap(content, MAX_CONTENT_CHARS, "content")?;

    let kind = match body
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(s) => Some(
            brainiac_core::MemoryKind::parse(s).ok_or_else(|| -> HttpError {
                (
                    StatusCode::BAD_REQUEST,
                    format!("unknown memory kind `{s}` (fact|decision|pattern|pitfall|howto)"),
                )
                    .into()
            })?,
        ),
    };

    if body.entities.len() > MAX_ENTITY_HINTS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "too many entities ({}); the limit is {MAX_ENTITY_HINTS}",
                body.entities.len()
            ),
        )
            .into());
    }
    let mut entities = Vec::with_capacity(body.entities.len());
    for e in &body.entities {
        let name = e.trim();
        if name.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "entity names must not be empty".to_string(),
            )
                .into());
        }
        within_cap(name, MAX_ENTITY_NAME_CHARS, "entities")?;
        entities.push(name.to_string());
    }

    Ok(brainiac_pipeline::manual::encode_manual_source(
        content, kind, &entities,
    ))
}

/// The 202 receipt: the source row that was written and the queue job that
/// will extract memories from it. Poll `GET /v1/sources/{id}` with `source_id`.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct MemoryAcceptedResponse {
    source_id: Uuid,
    job_id: i64,
}

/// Read + validate the optional `Idempotency-Key` header. Absent or blank ⇒
/// `None` (the non-idempotent path). Present ⇒ trimmed, length-capped, and any
/// violation is a clear 400.
fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, HttpError> {
    let Some(raw) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let key = raw
        .to_str()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Idempotency-Key must be valid text".to_string(),
            )
        })?
        .trim();
    if key.is_empty() {
        return Ok(None);
    }
    within_cap(key, MAX_IDEMPOTENCY_KEY_CHARS, "Idempotency-Key")?;
    Ok(Some(key.to_string()))
}

/// Resolve the project a write is attributed to (PROJECT-PLAN PR0): an
/// explicit `body.project_id` wins (validated against the caller's org — a
/// typo'd or foreign UUID must not mint an unresolvable stamp), else the
/// key's own project scope, else org-shared (None).
async fn effective_project(
    state: &AppState,
    ctx: &crate::auth::AuthContext,
    requested: Option<Uuid>,
) -> Result<Option<Uuid>, HttpError> {
    let Some(project_id) = requested else {
        return Ok(ctx.project_id);
    };
    let ok = brainiac_store::projects::belongs(state.store.pool(), ctx.principal.org_id, project_id)
        .await
        .map_err(internal)?;
    if !ok {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("project {project_id} does not exist in this org"),
        )
            .into());
    }
    Ok(Some(project_id))
}

/// Validate one item of content, write its source, and enqueue extraction —
/// the shared, non-idempotent ingest path behind single-add (no key) and every
/// bulk item. An empty or oversized body is a 400 before any DB work.
/// `project_id` is the already-resolved attribution (see [`effective_project`]).
async fn ingest_source(
    state: &AppState,
    principal: &Principal,
    body: &MemoryAddBody,
    project_id: Option<Uuid>,
) -> Result<MemoryAcceptedResponse, HttpError> {
    let raw_text = encode_add_source(body)?;
    let team_id = body.team_id.or_else(|| principal.team_ids.first().copied());
    let source_id = Uuid::new_v4();
    let mut tx = state.store.scoped_tx(principal).await.map_err(internal)?;
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        principal.org_id,
        team_id,
        "manual",
        &raw_text,
        Some(principal.user_id),
        project_id,
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let job_id =
        brainiac_pipeline::worker::enqueue_source(&state.store, principal.org_id, source_id)
            .await
            .map_err(internal)?;
    Ok(MemoryAcceptedResponse { source_id, job_id })
}

#[utoipa::path(
    post,
    path = "/v1/memories",
    tag = "memories",
    description = "Ingest raw content as a source and enqueue the extraction pipeline (async). Pass an `Idempotency-Key` header to make retries safe: the same key (scoped to your org, for the source's lifetime) replays the ORIGINAL receipt instead of minting a duplicate source.",
    request_body = MemoryAddBody,
    params(("Idempotency-Key" = Option<String>, Header, description = "Opaque retry token (≤200 chars). Same key + org ⇒ the original source_id/job_id, no duplicate source.")),
    responses(
        (status = 202, description = "Source stored and job enqueued (or the original receipt replayed for a repeated Idempotency-Key)", body = MemoryAcceptedResponse),
        (status = 400, description = "Empty content, or an oversized content / Idempotency-Key"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `write` scope"),
    )
)]
pub(crate) async fn memory_add(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MemoryAddBody>,
) -> Result<(StatusCode, Json<MemoryAcceptedResponse>), HttpError> {
    let ctx = auth_of(&state, &headers, "write").await?;
    let project_id = effective_project(&state, &ctx, body.project_id).await?;
    let principal = ctx.principal;
    let Some(key) = idempotency_key(&headers)? else {
        // No key: the plain async ingest.
        let receipt = ingest_source(&state, &principal, &body, project_id).await?;
        return Ok((StatusCode::ACCEPTED, Json(receipt)));
    };

    // Keyed: validate + encode the body up front so even a first use of a bad
    // body is a clean 400 (never a stored, un-processable source).
    let raw_text = encode_add_source(&body)?;
    let team_id = body.team_id.or_else(|| principal.team_ids.first().copied());
    let source_id = Uuid::new_v4();
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let inserted = brainiac_store::governance::insert_source_idempotent(
        &mut tx,
        source_id,
        principal.org_id,
        team_id,
        "manual",
        &raw_text,
        Some(principal.user_id),
        &key,
        project_id,
    )
    .await
    .map_err(internal)?;
    match inserted {
        // Fresh key: this call wrote the source — enqueue and return the receipt.
        Some(id) => {
            tx.commit().await.map_err(internal)?;
            let job_id =
                brainiac_pipeline::worker::enqueue_source(&state.store, principal.org_id, id)
                    .await
                    .map_err(internal)?;
            Ok((
                StatusCode::ACCEPTED,
                Json(MemoryAcceptedResponse {
                    source_id: id,
                    job_id,
                }),
            ))
        }
        // Repeated key: an earlier call already claimed it. Replay that
        // source's ORIGINAL receipt — no second source, no second pipeline run.
        None => {
            let existing = brainiac_store::governance::keyed_source_id(&mut tx, &key)
                .await
                .map_err(internal)?
                .ok_or_else(|| internal("idempotency conflict without a visible source"))?;
            drop(tx);
            // The original job, live or archived. `None` only if that first
            // call's enqueue never landed (failed or a sub-ms race) — recover
            // by enqueuing now, which still points at the one existing source.
            let job_id =
                match brainiac_store::queue::job_id_for_source(state.store.pool(), existing)
                    .await
                    .map_err(internal)?
                {
                    Some(job_id) => job_id,
                    None => brainiac_pipeline::worker::enqueue_source(
                        &state.store,
                        principal.org_id,
                        existing,
                    )
                    .await
                    .map_err(internal)?,
                };
            Ok((
                StatusCode::ACCEPTED,
                Json(MemoryAcceptedResponse {
                    source_id: existing,
                    job_id,
                }),
            ))
        }
    }
}

// ── bulk ingest ──────────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct BulkAddBody {
    /// Up to `MAX_BULK_ITEMS` (100) items, each the same shape as a single add.
    items: Vec<MemoryAddBody>,
}

/// One item's outcome, positionally aligned with the request `items`. Either a
/// success (`source_id` + `job_id`) or a per-item error (`error` + `code`) —
/// a bad item never sinks the rest of the batch.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct BulkItemResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    source_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct BulkAcceptedResponse {
    /// Per-item results, in request order.
    results: Vec<BulkItemResult>,
}

#[utoipa::path(
    post,
    path = "/v1/memories/bulk",
    tag = "memories",
    description = "Ingest up to 100 items in one request (org imports). Each item is validated and enqueued independently: the response carries a per-item result in request order, so one bad item ({error, code}) never sinks the others ({source_id, job_id}). This route accepts a larger request body than single add.",
    request_body = BulkAddBody,
    responses(
        (status = 202, description = "Batch accepted; see per-item results (mix of receipts and errors)", body = BulkAcceptedResponse),
        (status = 400, description = "Empty batch or more than 100 items"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `write` scope"),
    )
)]
pub(crate) async fn memory_add_bulk(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BulkAddBody>,
) -> Result<(StatusCode, Json<BulkAcceptedResponse>), HttpError> {
    let ctx = auth_of(&state, &headers, "write").await?;
    let principal = ctx.principal.clone();
    if body.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "items must not be empty".to_string(),
        )
            .into());
    }
    if body.items.len() > MAX_BULK_ITEMS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "too many items ({}); the limit is {MAX_BULK_ITEMS}",
                body.items.len()
            ),
        )
            .into());
    }
    let mut results = Vec::with_capacity(body.items.len());
    for item in &body.items {
        // Per-item attribution: an invalid explicit project is that ITEM's
        // error (like an empty content), never the batch's.
        let attributed = match effective_project(&state, &ctx, item.project_id).await {
            Ok(p) => p,
            Err(e) if e.status == StatusCode::INTERNAL_SERVER_ERROR => return Err(e),
            Err(e) => {
                results.push(BulkItemResult {
                    source_id: None,
                    job_id: None,
                    error: Some(e.message),
                    code: Some(e.code.to_string()),
                });
                continue;
            }
        };
        match ingest_source(&state, &principal, item, attributed).await {
            Ok(r) => results.push(BulkItemResult {
                source_id: Some(r.source_id),
                job_id: Some(r.job_id),
                error: None,
                code: None,
            }),
            // A systemic fault (500) sinks the request; a per-item business
            // error (empty/oversized content) is reported inline and the batch
            // carries on.
            Err(e) if e.status == StatusCode::INTERNAL_SERVER_ERROR => return Err(e),
            Err(e) => results.push(BulkItemResult {
                source_id: None,
                job_id: None,
                error: Some(e.message),
                code: Some(e.code.to_string()),
            }),
        }
    }
    Ok((StatusCode::ACCEPTED, Json(BulkAcceptedResponse { results })))
}

// ── memory feedback (REST mirror of MCP memory_feedback) ─────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct FeedbackBody {
    /// `helpful` (alias `useful`) | `wrong` | `outdated` (alias `stale`).
    verdict: String,
    /// Optional: what happened (especially for wrong/outdated).
    #[serde(default)]
    note: Option<String>,
}

/// One verdict tally for a memory.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct FeedbackVerdictCount {
    verdict: String,
    count: i64,
}

/// The receipt: the verdict as stored (after synonym canonicalization) plus
/// the memory's running feedback totals.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct FeedbackRecordedResponse {
    memory_id: Uuid,
    verdict: String,
    feedback_totals: Vec<FeedbackVerdictCount>,
}

/// Canonicalize the documented feedback vocabulary onto the STORED verdicts —
/// identical to the MCP surface (mcp.rs `canonical_verdict`): the doc terms
/// `useful`/`stale` are accepted while the corpus keeps `helpful`/`outdated`.
fn canonical_verdict(v: &str) -> &str {
    match v {
        "useful" => "helpful",
        "stale" => "outdated",
        other => other,
    }
}

/// POST /v1/memories/{id}/feedback — report how a served memory held up.
/// Mirrors MCP `memory_feedback` exactly: synonyms are canonicalized, an
/// invisible memory is a plain 404 (no existence oracle), and the same store
/// calls run under the caller's RLS.
#[utoipa::path(
    post,
    path = "/v1/memories/{id}/feedback",
    tag = "memories",
    description = "Report how a retrieved memory held up: helpful (alias useful), wrong, or outdated (alias stale). Verdicts drive ranking and re-verification.",
    params(("id" = Uuid, Path, description = "Memory id you were served")),
    request_body = FeedbackBody,
    responses(
        (status = 200, description = "Feedback recorded", body = FeedbackRecordedResponse),
        (status = 400, description = "Unknown verdict, or an oversized note"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `write` scope"),
        (status = 404, description = "Memory not found (or invisible under RLS — no oracle)"),
    )
)]
pub(crate) async fn memory_feedback(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<FeedbackRecordedResponse>, HttpError> {
    let principal = auth_of(&state, &headers, "write").await?.principal;
    let verdict = canonical_verdict(body.verdict.trim());
    if !brainiac_store::feedback::VERDICTS.contains(&verdict) {
        return Err((
            StatusCode::BAD_REQUEST,
            "verdict must be one of helpful|wrong|outdated (aliases: useful→helpful, stale→outdated)"
                .to_string(),
        )
            .into());
    }
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(note) = note {
        within_cap(note, MAX_NOTE_CHARS, "note")?;
    }

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    // Visibility gate under the caller's RLS: feedback on a memory you can't
    // read is refused as not-found (an FK check alone would bypass RLS and leak
    // existence). Mirrors mcp.rs memory_feedback.
    let visible = sqlx::query("SELECT 1 FROM memories WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?;
    if visible.is_none() {
        return Err((StatusCode::NOT_FOUND, "memory not found".to_string()).into());
    }
    brainiac_store::feedback::insert(
        &mut tx,
        Uuid::new_v4(),
        principal.org_id,
        id,
        principal.user_id,
        verdict,
        note,
    )
    .await
    .map_err(internal)?;
    let summary = brainiac_store::feedback::summary(&mut tx, id)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(FeedbackRecordedResponse {
        memory_id: id,
        verdict: verdict.to_string(),
        feedback_totals: summary
            .into_iter()
            .map(|(verdict, count)| FeedbackVerdictCount { verdict, count })
            .collect(),
    }))
}

// ── memory provenance (REST mirror of MCP memory_provenance) ─────────────

/// The originating source, with a bounded excerpt of its raw text. `null` when
/// the memory carries no source; `excerpt` is `null` when the source has no
/// text.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ProvenanceSource {
    kind: String,
    /// Bounded to SOURCE_EXCERPT_CHARS (500) chars — a citation handle, never
    /// the whole transcript.
    excerpt: Option<String>,
}

/// A memory's evidence chain for citation. Mirrors MCP `memory_provenance`
/// exactly: actor/model/time, the originating source (bounded excerpt), and the
/// canonical entities it anchors.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct MemoryProvenanceResponse {
    memory_id: Uuid,
    actor_kind: Option<String>,
    actor_ref: Option<String>,
    model_ref: Option<String>,
    created_at: Option<DateTime<Utc>>,
    source: Option<ProvenanceSource>,
    entity_anchors: Vec<AnchorRef>,
}

/// GET /v1/memories/{id}/provenance — trace a memory's evidence chain. Read
/// scope. Invisible-under-RLS resolves to 404, the same as a nonexistent id
/// (no existence oracle) — mirrors MCP `memory_provenance`.
#[utoipa::path(
    get,
    path = "/v1/memories/{id}/provenance",
    tag = "memories",
    description = "Trace a memory's evidence chain for citation: who/what recorded it, the model, when, the originating source with a short excerpt, and the canonical entities it anchors.",
    params(("id" = Uuid, Path, description = "Memory id you were served")),
    responses(
        (status = 200, description = "Provenance chain", body = MemoryProvenanceResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
        (status = 404, description = "Memory not found (or invisible under RLS — no oracle)"),
    )
)]
pub(crate) async fn memory_provenance(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<MemoryProvenanceResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    // RLS gate: a memory invisible to the caller resolves to None — the SAME
    // "not found" as a nonexistent id, so this endpoint is no existence oracle.
    let Some(view) = brainiac_store::governance::provenance_for_memory(&mut tx, id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "memory not found".to_string()).into());
    };
    // Canonical entities anchoring the memory — the batched helper (single id).
    let anchors = brainiac_store::entities::canonical_anchors_for(&mut tx, &[id])
        .await
        .map_err(internal)?;
    let entity_anchors = anchors
        .get(&id)
        .map(|a| {
            a.iter()
                .map(|e| AnchorRef {
                    id: e.id,
                    name: e.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    // Bound the source excerpt to the documented cap (char-boundary safe).
    let source = view.source_kind.as_ref().map(|kind| {
        let excerpt = view.source_text.as_deref().map(|text| {
            let trimmed = text.trim();
            let excerpt: String = trimmed.chars().take(SOURCE_EXCERPT_CHARS).collect();
            if trimmed.chars().count() > SOURCE_EXCERPT_CHARS {
                format!("{excerpt}…")
            } else {
                excerpt
            }
        });
        ProvenanceSource {
            kind: kind.clone(),
            excerpt,
        }
    });
    Ok(Json(MemoryProvenanceResponse {
        memory_id: id,
        actor_kind: view.actor_kind,
        actor_ref: view.actor_ref,
        model_ref: view.model_ref,
        created_at: view.created_at,
        source,
        entity_anchors,
    }))
}

/// Memory body of a pending promotion. `None` on the parent ⇒ serialized as
/// `null` (RLS-invisible memory) — never omitted.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct PromotionMemory {
    content: String,
    kind: Option<String>,
    status: Option<String>,
    confidence: Option<f32>,
    team: Option<String>,
    /// Project display name (PROJECT-PLAN PR2); null = org-shared. A reviewer
    /// deciding whether a claim generalizes needs to know which application it
    /// came from.
    project: Option<String>,
}

/// Provenance of a pending promotion; `null` alongside an invisible memory.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct PromotionProvenance {
    actor_kind: String,
    actor_id: String,
    model_ref: Option<String>,
    source_kind: Option<String>,
    source_ref: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct PendingPromotion {
    id: Uuid,
    memory_id: Uuid,
    from_status: String,
    to_status: String,
    policy_rule: Option<String>,
    age_secs: i64,
    memory: Option<PromotionMemory>,
    provenance: Option<PromotionProvenance>,
}

/// Paging for the promotion review queue. Defaults preserve the pre-paging
/// behaviour (first 100, oldest first); `offset` + the response `total` make a
/// backlog beyond the first page reachable and countable.
#[derive(Deserialize, utoipa::IntoParams)]
pub(crate) struct PromotionsQuery {
    /// Page size (default 100, clamped 1..200).
    limit: Option<i64>,
    /// Page offset (default 0).
    offset: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct PromotionQueueResponse {
    /// Total promotions awaiting review — the full backlog, independent of the
    /// page window, so a caller knows how far `offset` can reach.
    total: i64,
    promotions: Vec<PendingPromotion>,
}

#[utoipa::path(
    get,
    path = "/v1/reviews/promotions",
    tag = "reviews",
    description = "Promotions awaiting human review (oldest first). Paged: `total` reports the full backlog, `offset` reaches beyond the first page.",
    params(PromotionsQuery),
    responses(
        (status = 200, description = "Pending review queue page", body = PromotionQueueResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
    )
)]
pub(crate) async fn pending_promotions(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<PromotionsQuery>,
    headers: HeaderMap,
) -> Result<Json<PromotionQueueResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = q.limit.unwrap_or(100).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    use sqlx::Row;
    // LEFT JOINs: an RLS-invisible memory keeps the promotion row visible
    // (it's org metadata) but renders content/provenance as null — same
    // no-oracle stance as the contradictions listing.
    let rows = sqlx::query(
        "SELECT p.id, p.memory_id, p.from_status::text AS from_status,
                p.to_status::text AS to_status, p.policy_rule, p.created_at,
                EXTRACT(EPOCH FROM now() - p.created_at)::bigint AS age_secs,
                m.content, m.kind, m.status::text AS memory_status, m.confidence,
                t.name AS team, pj.name AS project,
                pv.actor_kind, pv.actor_id, pv.model_ref,
                s.kind AS source_kind, s.external_ref AS source_ref
         FROM promotions p
         LEFT JOIN memories m ON m.id = p.memory_id
         LEFT JOIN teams t ON t.id = m.team_id
         LEFT JOIN projects pj ON pj.id = m.project_id
         LEFT JOIN provenance pv ON pv.id = m.provenance_id
         LEFT JOIN sources s ON s.id = pv.source_id
         WHERE p.policy_decision = 'needs_review' AND p.reviewed_at IS NULL
         ORDER BY p.created_at ASC, p.id
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    // Full backlog independent of the page window — the count this endpoint
    // exists to expose (the old hard LIMIT 100 hid anything past the first page).
    let total: i64 = sqlx::query(
        "SELECT count(*) AS n FROM promotions p
         WHERE p.policy_decision = 'needs_review' AND p.reviewed_at IS NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?
    .get("n");
    let out: Vec<PendingPromotion> = rows
        .iter()
        .map(|r| {
            let provenance =
                r.get::<Option<String>, _>("actor_kind")
                    .map(|actor_kind| PromotionProvenance {
                        actor_kind,
                        actor_id: r.get::<String, _>("actor_id"),
                        model_ref: r.get::<Option<String>, _>("model_ref"),
                        source_kind: r.get::<Option<String>, _>("source_kind"),
                        source_ref: r.get::<Option<String>, _>("source_ref"),
                    });
            PendingPromotion {
                id: r.get::<Uuid, _>("id"),
                memory_id: r.get::<Uuid, _>("memory_id"),
                from_status: r.get::<String, _>("from_status"),
                to_status: r.get::<String, _>("to_status"),
                policy_rule: r.get::<Option<String>, _>("policy_rule"),
                age_secs: r.get::<i64, _>("age_secs"),
                memory: r
                    .get::<Option<String>, _>("content")
                    .map(|content| PromotionMemory {
                        content,
                        kind: r.get::<Option<String>, _>("kind"),
                        status: r.get::<Option<String>, _>("memory_status"),
                        confidence: r.get::<Option<f32>, _>("confidence"),
                        team: r.get::<Option<String>, _>("team"),
                        project: r.get::<Option<String>, _>("project"),
                    }),
                provenance,
            }
        })
        .collect();
    Ok(Json(PromotionQueueResponse {
        total,
        promotions: out,
    }))
}

// ── ingestion status ────────────────────────────────────────────────────

/// The source row itself, as the caller's RLS scope sees it.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SourceInfo {
    kind: String,
    external_ref: Option<String>,
    created_at: DateTime<Utc>,
}

/// Queue state of the ingest job. `null` on the parent once the job has aged
/// out of both `queue.jobs` and `queue.archive`.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SourceJob {
    /// `queued` (still in `queue.jobs`) or `archived`.
    state: String,
    attempts: i32,
    /// Archive outcome (`ok` / failure reason); always null while queued.
    outcome: Option<String>,
}

/// What the pipeline produced from this source (nested, not flattened).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SourceResults {
    memories: i64,
    promoted: i64,
    pending_review: i64,
    /// The ids of the memories this source produced — what closes the loop on
    /// an async `memory_add` (F-1/F-2). Poll this endpoint until `status` is
    /// `processed`; then these ids are real memories you can cite (e.g. as a
    /// standard's `evidence_memory_id`) or feed back on. `memory_add` returns a
    /// SOURCE id, not a memory id, and extraction is asynchronous — so before
    /// this, an agent had no way to learn what its contribution became, or
    /// whether it landed at all. Empty while queued, or if extraction produced
    /// nothing (a real outcome worth seeing, not a hang).
    memory_ids: Vec<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct SourceStatusResponse {
    source_id: Uuid,
    /// One-word rollup: queued|retrying|processed|failed|unknown.
    status: String,
    source: SourceInfo,
    job: Option<SourceJob>,
    results: SourceResults,
}

/// GET /v1/sources/{id} — what happened to an async memory_add. Closes the
/// loop on the 202: the source row (RLS-scoped), the queue job state, and
/// what the pipeline produced from it.
#[utoipa::path(
    get,
    path = "/v1/sources/{id}",
    tag = "memories",
    description = "Ingestion status of a source: queue job state plus what the pipeline produced.",
    params(("id" = Uuid, Path, description = "Source id returned by POST /v1/memories")),
    responses(
        (status = 200, description = "Ingestion status", body = SourceStatusResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
        (status = 404, description = "Source not found (or invisible under RLS — no oracle)"),
    )
)]
pub(crate) async fn source_status(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<SourceStatusResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    use sqlx::Row;
    // RLS makes an unknown-or-foreign source a plain 404 (no oracle).
    let source = sqlx::query("SELECT kind, external_ref, created_at FROM sources WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "source not found".into()))?;
    // Pipeline output: memories whose provenance points at this source.
    let produced = sqlx::query(
        "SELECT count(*) AS memories,
                count(*) FILTER (WHERE m.status = 'candidate' OR m.status = 'canonical') AS promoted,
                count(pr.id) FILTER (WHERE pr.policy_decision = 'needs_review' AND pr.reviewed_at IS NULL) AS pending_review
         FROM memories m
         LEFT JOIN promotions pr ON pr.memory_id = m.id
         JOIN provenance pv ON pv.id = m.provenance_id
         WHERE pv.source_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    // The ids themselves (not just the count) — the handle that lets an agent
    // cite what it just added. Fetched under the same RLS tx as the count.
    let memory_ids: Vec<Uuid> = sqlx::query(
        "SELECT m.id FROM memories m
         JOIN provenance pv ON pv.id = m.provenance_id
         WHERE pv.source_id = $1
         ORDER BY m.created_at",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?
    .iter()
    .map(|r| r.get::<Uuid, _>("id"))
    .collect();
    drop(tx);
    // Queue state lives outside RLS (the queue schema is org-blind); the
    // source's org membership was already proven by the RLS read above.
    let job = sqlx::query(
        "SELECT 'queued' AS state, attempts, enqueued_at, NULL::text AS outcome
         FROM queue.jobs WHERE payload->>'source_id' = $1
         UNION ALL
         SELECT 'archived' AS state, attempts, enqueued_at, outcome
         FROM queue.archive WHERE payload->>'source_id' = $1
         ORDER BY enqueued_at DESC LIMIT 1",
    )
    .bind(id.to_string())
    .fetch_optional(state.store.pool())
    .await
    .map_err(internal)?;
    let memories: i64 = produced.get("memories");
    let job = job.as_ref().map(|j| SourceJob {
        state: j.get::<String, _>("state"),
        attempts: j.get::<i32, _>("attempts"),
        outcome: j.get::<Option<String>, _>("outcome"),
    });
    // One-word rollup the caller can poll on.
    let status = match (&job, memories) {
        (Some(j), _) if j.state == "queued" && j.attempts == 0 => "queued",
        (Some(j), _) if j.state == "queued" => "retrying",
        (Some(j), _) if j.state == "archived" && j.outcome.as_deref() == Some("ok") => "processed",
        (Some(_), _) => "failed",
        (None, 0) => "unknown", // job vanished without output (pre-status enqueue)
        (None, _) => "processed",
    };
    Ok(Json(SourceStatusResponse {
        source_id: id,
        status: status.to_string(),
        source: SourceInfo {
            kind: source.get::<String, _>("kind"),
            external_ref: source.get::<Option<String>, _>("external_ref"),
            created_at: source.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        },
        job,
        results: SourceResults {
            memories,
            promoted: produced.get::<i64, _>("promoted"),
            pending_review: produced.get::<i64, _>("pending_review"),
            memory_ids,
        },
    }))
}

// ── managed API tokens ──────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct CreateTokenBody {
    name: String,
    /// Subset of read|write|admin; defaults to ["read"].
    #[serde(default)]
    scopes: Option<Vec<String>>,
    /// Principal the token acts as; defaults to the caller.
    #[serde(default)]
    user_id: Option<Uuid>,
    /// Scope the key to one project (migration 0034); omit for org-wide.
    #[serde(default)]
    project_id: Option<Uuid>,
}

/// The mint response. `token` is the plaintext secret and is the only place
/// it will ever appear — everything else is retrievable from `GET /v1/tokens`.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct CreatedTokenResponse {
    id: Uuid,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    user_id: Uuid,
    /// The project the key is scoped to; null = org-wide.
    project_id: Option<Uuid>,
    /// Shown exactly once — never retrievable again.
    token: String,
}

/// POST /v1/tokens — mint a token. The secret appears ONCE in this response;
/// only its sha256 is stored. Requires the `admin` scope (env bootstrap
/// tokens qualify), so read/write tokens cannot mint tokens.
#[utoipa::path(
    post,
    path = "/v1/tokens",
    tag = "tokens",
    description = "Mint an API token; the plaintext secret is returned once and never again.",
    request_body = CreateTokenBody,
    responses(
        (status = 201, description = "Token minted", body = CreatedTokenResponse),
        (status = 400, description = "Empty name or scopes outside the mintable set (see auth::SCOPES)"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn create_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateTokenBody>,
) -> Result<(StatusCode, Json<CreatedTokenResponse>), HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "name must not be empty".to_string(),
        )
            .into());
    }
    let scopes = body.scopes.unwrap_or_else(|| vec!["read".into()]);
    if scopes.is_empty()
        || scopes
            .iter()
            .any(|s| !crate::auth::SCOPES.contains(&s.as_str()))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "scopes must be a non-empty subset of {:?}",
                crate::auth::SCOPES
            ),
        )
            .into());
    }
    let user_id = body.user_id.unwrap_or(ctx.principal.user_id);
    // A project-scoped key must point at a project the caller's org actually
    // owns — otherwise a typo'd/foreign UUID would mint a key whose scope
    // nothing can resolve (or worse, referencing another org's project).
    if let Some(project_id) = body.project_id {
        let ok = brainiac_store::projects::belongs(state.store.pool(), ctx.principal.org_id, project_id)
            .await
            .map_err(internal)?;
        if !ok {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("project {project_id} does not exist in this org"),
            )
                .into());
        }
    }
    let (secret, prefix) = crate::auth::mint_secret();
    let id = Uuid::new_v4();
    brainiac_store::tokens::create(
        state.store.pool(),
        id,
        ctx.principal.org_id,
        user_id,
        name,
        &prefix,
        &crate::auth::hash_token(&secret),
        &scopes,
        body.project_id,
        ctx.principal.user_id,
    )
    .await
    .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedTokenResponse {
            id,
            name: name.to_string(),
            prefix,
            scopes,
            user_id,
            project_id: body.project_id,
            // Shown exactly once — never retrievable again.
            token: secret,
        }),
    ))
}

/// One token as the listing exposes it — metadata only, never the secret.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct TokenSummary {
    id: Uuid,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    /// The project the key is scoped to; null = org-wide.
    project_id: Option<Uuid>,
    /// Display name of that project; null = org-wide.
    project_name: Option<String>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct TokenListResponse {
    tokens: Vec<TokenSummary>,
}

/// GET /v1/tokens — list the org's tokens (metadata only, never secrets).
#[utoipa::path(
    get,
    path = "/v1/tokens",
    tag = "tokens",
    description = "List the org's API tokens (metadata only; secrets are never retrievable).",
    responses(
        (status = 200, description = "Tokens of the caller's org", body = TokenListResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn list_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TokenListResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let rows = brainiac_store::tokens::list(state.store.pool(), ctx.principal.org_id)
        .await
        .map_err(internal)?;
    Ok(Json(TokenListResponse {
        tokens: rows
            .iter()
            .map(|t| TokenSummary {
                id: t.id,
                name: t.name.clone(),
                prefix: t.prefix.clone(),
                scopes: t.scopes.clone(),
                project_id: t.project_id,
                project_name: t.project_name.clone(),
                created_at: t.created_at,
                last_used_at: t.last_used_at,
                revoked_at: t.revoked_at,
            })
            .collect(),
    }))
}

/// Confirmation of a revoke; `revoked` is always `true` (a no-op revoke 404s).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct RevokeResponse {
    id: Uuid,
    revoked: bool,
}

/// POST /v1/tokens/{id}/revoke — kill a token immediately.
#[utoipa::path(
    post,
    path = "/v1/tokens/{id}/revoke",
    tag = "tokens",
    description = "Revoke a token immediately; subsequent requests with it are 401.",
    params(("id" = Uuid, Path, description = "Token id (not the secret)")),
    responses(
        (status = 200, description = "Token revoked", body = RevokeResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Token not found or already revoked"),
    )
)]
pub(crate) async fn revoke_token(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<RevokeResponse>, HttpError> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let revoked = brainiac_store::tokens::revoke(state.store.pool(), ctx.principal.org_id, id)
        .await
        .map_err(internal)?;
    if !revoked {
        return Err((
            StatusCode::NOT_FOUND,
            "token not found or already revoked".into(),
        )
            .into());
    }
    Ok(Json(RevokeResponse { id, revoked: true }))
}

// ── queue operations ────────────────────────────────────────────────────
// The queue schema is org-blind (payloads span every org), so all three
// endpoints require the admin scope — this is an operator surface.

#[derive(Deserialize, utoipa::IntoParams)]
pub(crate) struct QueueQuery {
    /// Queue name; defaults to the ingest queue.
    queue: Option<String>,
    /// Page size for the dead-letter listing (default 50, clamped 1..200).
    limit: Option<i64>,
    /// Page offset for the dead-letter listing (default 0).
    offset: Option<i64>,
}

/// One bucket of the retry-attempt histogram over the ready jobs.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct AttemptsBucket {
    attempts: i32,
    count: i64,
}

/// Archive tallies, kept nested under `archived` (not flattened).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ArchivedCounts {
    ok: i64,
    failed: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct QueueHealthResponse {
    queue: String,
    ready: i64,
    in_flight: i64,
    oldest_ready_secs: i64,
    attempts_histogram: Vec<AttemptsBucket>,
    archived: ArchivedCounts,
    dead_letters: i64,
}

#[utoipa::path(
    get,
    path = "/v1/queue/health",
    tag = "queue",
    description = "Operator view of one queue: depth, in-flight, retry histogram, archive tallies.",
    params(QueueQuery),
    responses(
        (status = 200, description = "Queue health snapshot", body = QueueHealthResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn queue_health(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<QueueQuery>,
    headers: HeaderMap,
) -> Result<Json<QueueHealthResponse>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    let queue = q
        .queue
        .unwrap_or_else(|| brainiac_pipeline::worker::INGEST_QUEUE.to_string());
    let h = brainiac_store::queue::health(state.store.pool(), &queue)
        .await
        .map_err(internal)?;
    Ok(Json(QueueHealthResponse {
        queue: h.queue_name,
        ready: h.ready,
        in_flight: h.in_flight,
        oldest_ready_secs: h.oldest_ready_secs,
        attempts_histogram: h
            .attempts_histogram
            .iter()
            .map(|(a, n)| AttemptsBucket {
                attempts: *a,
                count: *n,
            })
            .collect(),
        archived: ArchivedCounts {
            ok: h.archived_ok,
            failed: h.archived_failed,
        },
        dead_letters: h.dead_letters,
    }))
}

/// One dead-lettered job. `payload` is the opaque job JSON, passed through
/// verbatim (its shape belongs to the pipeline, not to this API).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct DeadLetterEntry {
    id: i64,
    #[schema(value_type = Object)]
    payload: serde_json::Value,
    attempts: i32,
    enqueued_at: DateTime<Utc>,
    archived_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct DeadLetterListResponse {
    /// Total dead letters on the queue — the full recovery backlog, independent
    /// of the page window, so an operator knows how far `offset` can reach.
    total: i64,
    dead_letters: Vec<DeadLetterEntry>,
}

#[utoipa::path(
    get,
    path = "/v1/queue/dead-letters",
    tag = "queue",
    description = "Dead-lettered jobs, most recent first. Paged: `total` reports the full recovery backlog, `offset` reaches beyond the first page (default limit 50).",
    params(QueueQuery),
    responses(
        (status = 200, description = "Dead-letter listing page", body = DeadLetterListResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn queue_dead_letters(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<QueueQuery>,
    headers: HeaderMap,
) -> Result<Json<DeadLetterListResponse>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    let queue = q
        .queue
        .unwrap_or_else(|| brainiac_pipeline::worker::INGEST_QUEUE.to_string());
    let rows = brainiac_store::queue::dead_letters(
        state.store.pool(),
        &queue,
        q.limit.unwrap_or(50),
        q.offset.unwrap_or(0),
    )
    .await
    .map_err(internal)?;
    let total = brainiac_store::queue::dead_letters_count(state.store.pool(), &queue)
        .await
        .map_err(internal)?;
    Ok(Json(DeadLetterListResponse {
        total,
        dead_letters: rows
            .iter()
            .map(|d| DeadLetterEntry {
                id: d.id,
                payload: d.payload.clone(),
                attempts: d.attempts,
                enqueued_at: d.enqueued_at,
                archived_at: d.archived_at,
            })
            .collect(),
    }))
}

/// Confirmation of a requeue; `requeued` is always `true` (a miss 404s).
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct RequeueResponse {
    id: i64,
    requeued: bool,
}

#[utoipa::path(
    post,
    path = "/v1/queue/dead-letters/{id}/requeue",
    tag = "queue",
    description = "Move a dead-lettered job back onto its queue for another attempt.",
    params(("id" = i64, Path, description = "Archive row id of the dead-lettered job")),
    responses(
        (status = 200, description = "Job requeued", body = RequeueResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
        (status = 404, description = "Job is not in the dead-letter archive"),
    )
)]
pub(crate) async fn queue_requeue(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    headers: HeaderMap,
) -> Result<Json<RequeueResponse>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    let requeued = brainiac_store::queue::requeue_dead(state.store.pool(), id)
        .await
        .map_err(internal)?;
    if !requeued {
        return Err((
            StatusCode::NOT_FOUND,
            "job is not in the dead-letter archive".into(),
        )
            .into());
    }
    Ok(Json(RequeueResponse { id, requeued: true }))
}

// ── error envelope ──────────────────────────────────────────────────────
// Every REST error is a JSON envelope `{"error": <message>, "code": <slug>}`
// with the right status — the same content type as the success bodies, and the
// same posture as the MCP surface: internal faults are logged in full and the
// client gets a generic message, so raw DB/anyhow strings never leak.

/// The JSON error body. Documented once in the OpenAPI spec and returned by
/// every error path.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorResponse {
    /// Human-readable message. Specific for business errors (400/401/403/404);
    /// a generic `"internal error"` for 5xx faults (detail is logged, not sent).
    pub error: String,
    /// Machine-readable slug:
    /// `bad_request` | `unauthorized` | `forbidden` | `not_found` |
    /// `payload_too_large` | `internal_error`.
    pub code: String,
}

/// A REST error carrying its status, a machine-readable code, and a
/// client-safe message. Replaces the old `(StatusCode, String)` alias; its
/// `IntoResponse` renders the JSON envelope. Construct business errors from a
/// `(StatusCode, String)` tuple (the code is derived from the status) and
/// internal faults via [`internal`].
pub struct HttpError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

/// The machine-readable slug for a status — the canonical reason, lower-snake.
fn code_for(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        _ => "internal_error",
    }
}

impl From<(StatusCode, String)> for HttpError {
    fn from((status, message): (StatusCode, String)) -> Self {
        HttpError {
            code: code_for(status),
            status,
            message,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
                code: self.code.to_string(),
            }),
        )
            .into_response()
    }
}

/// An internal fault: log the detail (as the MCP surface does) and return a
/// generic `500` — no DB/anyhow string ever reaches the client.
pub(crate) fn internal(e: impl std::fmt::Display) -> HttpError {
    tracing::error!(error = %e, "internal error handling REST request");
    HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal_error",
        message: "internal error".into(),
    }
}
