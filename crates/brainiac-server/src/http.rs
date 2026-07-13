//! REST surface v0 (ARCHITECTURE.md §5.2, minimal slice):
//! - GET  /health
//! - POST /v1/memories/search   — hybrid retrieval under the caller's RLS
//! - POST /v1/memories          — memory_add: source + pipeline enqueue (202)
//! - GET  /v1/reviews/promotions — pending review queue
//!
//! Every handler resolves the bearer token to a principal FIRST; there is no
//! anonymous data path.

use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainiac_core::embed::Embedder;
use brainiac_core::Principal;
use brainiac_store::Store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::TokenMap;

pub struct AppState {
    pub store: Store,
    pub embedder: Arc<dyn Embedder>,
    pub embedding_version: i32,
    pub tokens: TokenMap,
}

pub async fn router(store: Store, embedder: Arc<dyn Embedder>) -> Result<Router> {
    let tokens = TokenMap::from_env()?;
    if tokens.is_empty() {
        tracing::warn!("BRAINIAC_TOKENS is empty — every request will be 401");
    }
    let embedding_version = {
        let principal = brainiac_pipeline::pipeline_principal(Uuid::nil());
        let mut tx = store.scoped_tx(&principal).await?;
        let v = brainiac_store::memories::ensure_embedding_version(
            &mut tx,
            embedder.model_name(),
            embedder.dim() as i32,
        )
        .await?;
        tx.commit().await?;
        v
    };
    let state = Arc::new(AppState {
        store,
        embedder,
        embedding_version,
        tokens,
    });
    Ok(Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(crate::openapi::openapi_json))
        .route("/v1/memories/search", post(search))
        .route("/v1/memories", post(memory_add))
        .route("/v1/sources/{id}", get(source_status))
        .route("/v1/reviews/promotions", get(pending_promotions))
        .route("/v1/tokens", get(list_tokens).post(create_token))
        .route("/v1/tokens/{id}/revoke", post(revoke_token))
        .route("/v1/queue/health", get(queue_health))
        .route("/v1/queue/dead-letters", get(queue_dead_letters))
        .route("/v1/queue/dead-letters/{id}/requeue", post(queue_requeue))
        .merge(crate::console::routes())
        .with_state(state))
}

/// Resolve the bearer token and require `scope` (env tokens pass all
/// scopes; `brk_…` API tokens carry what they were minted with).
pub(crate) async fn auth_of(
    state: &AppState,
    headers: &HeaderMap,
    scope: &str,
) -> Result<crate::auth::AuthContext, (StatusCode, String)> {
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
        ));
    }
    Ok(ctx)
}

/// Read-scope principal — the default for query endpoints.
pub(crate) async fn principal_of(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, (StatusCode, String)> {
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
}

impl SearchBody {
    fn filters(&self) -> Result<brainiac_store::retrieval::RetrievalFilters, (StatusCode, String)> {
        for k in &self.kinds {
            if brainiac_core::MemoryKind::parse(k).is_none() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("unknown kind `{k}` (fact|decision|pattern|pitfall|howto)"),
                ));
            }
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
                ));
            }
        }
        Ok(brainiac_store::retrieval::RetrievalFilters {
            kinds: self.kinds.clone(),
            min_status,
            team_id: self.team_id,
            min_confidence: self.min_confidence,
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
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let principal = principal_of(&state, &headers).await?;
    let filters = body.filters()?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.store.pool(),
        state.embedder.as_ref(),
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
    let out: Vec<SearchHit> = hits
        .into_iter()
        .map(|h| SearchHit {
            id: h.memory.id,
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
        })
        .collect();
    Ok(Json(SearchResponse { hits: out }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct MemoryAddBody {
    content: String,
    #[serde(default)]
    team_id: Option<Uuid>,
}

/// The 202 receipt: the source row that was written and the queue job that
/// will extract memories from it. Poll `GET /v1/sources/{id}` with `source_id`.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct MemoryAcceptedResponse {
    source_id: Uuid,
    job_id: i64,
}

#[utoipa::path(
    post,
    path = "/v1/memories",
    tag = "memories",
    description = "Ingest raw content as a source and enqueue the extraction pipeline (async).",
    request_body = MemoryAddBody,
    responses(
        (status = 202, description = "Source stored and job enqueued", body = MemoryAcceptedResponse),
        (status = 400, description = "Empty content"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `write` scope"),
    )
)]
pub(crate) async fn memory_add(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MemoryAddBody>,
) -> Result<(StatusCode, Json<MemoryAcceptedResponse>), (StatusCode, String)> {
    let principal = auth_of(&state, &headers, "write").await?.principal;
    if body.content.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "content must not be empty".into()));
    }
    let team_id = body.team_id.or_else(|| principal.team_ids.first().copied());
    let source_id = Uuid::new_v4();
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        principal.org_id,
        team_id,
        "manual",
        body.content.trim(),
        Some(principal.user_id),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let job_id =
        brainiac_pipeline::worker::enqueue_source(&state.store, principal.org_id, source_id)
            .await
            .map_err(internal)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(MemoryAcceptedResponse { source_id, job_id }),
    ))
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

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct PromotionQueueResponse {
    promotions: Vec<PendingPromotion>,
}

#[utoipa::path(
    get,
    path = "/v1/reviews/promotions",
    tag = "reviews",
    description = "Promotions awaiting human review (oldest first, max 100).",
    responses(
        (status = 200, description = "Pending review queue", body = PromotionQueueResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `read` scope"),
    )
)]
pub(crate) async fn pending_promotions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<PromotionQueueResponse>, (StatusCode, String)> {
    let principal = principal_of(&state, &headers).await?;
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
                t.name AS team,
                pv.actor_kind, pv.actor_id, pv.model_ref,
                s.kind AS source_kind, s.external_ref AS source_ref
         FROM promotions p
         LEFT JOIN memories m ON m.id = p.memory_id
         LEFT JOIN teams t ON t.id = m.team_id
         LEFT JOIN provenance pv ON pv.id = m.provenance_id
         LEFT JOIN sources s ON s.id = pv.source_id
         WHERE p.policy_decision = 'needs_review' AND p.reviewed_at IS NULL
         ORDER BY p.created_at ASC
         LIMIT 100",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
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
                    }),
                provenance,
            }
        })
        .collect();
    Ok(Json(PromotionQueueResponse { promotions: out }))
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
) -> Result<Json<SourceStatusResponse>, (StatusCode, String)> {
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
        (status = 400, description = "Empty name or scopes outside read|write|admin"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn create_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateTokenBody>,
) -> Result<(StatusCode, Json<CreatedTokenResponse>), (StatusCode, String)> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name must not be empty".into()));
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
        ));
    }
    let user_id = body.user_id.unwrap_or(ctx.principal.user_id);
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
) -> Result<Json<TokenListResponse>, (StatusCode, String)> {
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
) -> Result<Json<RevokeResponse>, (StatusCode, String)> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let revoked = brainiac_store::tokens::revoke(state.store.pool(), ctx.principal.org_id, id)
        .await
        .map_err(internal)?;
    if !revoked {
        return Err((
            StatusCode::NOT_FOUND,
            "token not found or already revoked".into(),
        ));
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
    limit: Option<i64>,
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
) -> Result<Json<QueueHealthResponse>, (StatusCode, String)> {
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
    dead_letters: Vec<DeadLetterEntry>,
}

#[utoipa::path(
    get,
    path = "/v1/queue/dead-letters",
    tag = "queue",
    description = "Dead-lettered jobs, most recent first (default limit 50).",
    params(QueueQuery),
    responses(
        (status = 200, description = "Dead-letter listing", body = DeadLetterListResponse),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `admin` scope"),
    )
)]
pub(crate) async fn queue_dead_letters(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<QueueQuery>,
    headers: HeaderMap,
) -> Result<Json<DeadLetterListResponse>, (StatusCode, String)> {
    auth_of(&state, &headers, "admin").await?;
    let queue = q
        .queue
        .unwrap_or_else(|| brainiac_pipeline::worker::INGEST_QUEUE.to_string());
    let rows =
        brainiac_store::queue::dead_letters(state.store.pool(), &queue, q.limit.unwrap_or(50))
            .await
            .map_err(internal)?;
    Ok(Json(DeadLetterListResponse {
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
) -> Result<Json<RequeueResponse>, (StatusCode, String)> {
    auth_of(&state, &headers, "admin").await?;
    let requeued = brainiac_store::queue::requeue_dead(state.store.pool(), id)
        .await
        .map_err(internal)?;
    if !requeued {
        return Err((
            StatusCode::NOT_FOUND,
            "job is not in the dead-letter archive".into(),
        ));
    }
    Ok(Json(RequeueResponse { id, requeued: true }))
}

pub(crate) fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
