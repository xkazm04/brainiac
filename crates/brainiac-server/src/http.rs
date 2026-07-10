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
use serde_json::json;
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
        .route("/v1/memories/search", post(search))
        .route("/v1/memories", post(memory_add))
        .route("/v1/sources/{id}", get(source_status))
        .route("/v1/reviews/promotions", get(pending_promotions))
        .route("/v1/tokens", get(list_tokens).post(create_token))
        .route("/v1/tokens/{id}/revoke", post(revoke_token))
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

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

#[derive(Deserialize)]
struct SearchBody {
    query: String,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    as_of: Option<DateTime<Utc>>,
}

fn default_k() -> usize {
    10
}

#[derive(Serialize)]
struct SearchHit {
    id: Uuid,
    content: String,
    kind: String,
    status: String,
    score: f64,
    via_graph: bool,
    provenance_id: Option<Uuid>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SearchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.embedder.as_ref(),
        state.embedding_version,
        &brainiac_store::retrieval::RetrievalRequest {
            query: body.query,
            k: body.k.min(50),
            as_of: body.as_of,
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
        })
        .collect();
    Ok(Json(json!({ "hits": out })))
}

#[derive(Deserialize)]
struct MemoryAddBody {
    content: String,
    #[serde(default)]
    team_id: Option<Uuid>,
}

async fn memory_add(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MemoryAddBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
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
        Json(json!({ "source_id": source_id, "job_id": job_id })),
    ))
}

async fn pending_promotions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
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
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let provenance = r.get::<Option<String>, _>("actor_kind").map(|actor_kind| {
                json!({
                    "actor_kind": actor_kind,
                    "actor_id": r.get::<String, _>("actor_id"),
                    "model_ref": r.get::<Option<String>, _>("model_ref"),
                    "source_kind": r.get::<Option<String>, _>("source_kind"),
                    "source_ref": r.get::<Option<String>, _>("source_ref"),
                })
            });
            json!({
                "id": r.get::<Uuid, _>("id"),
                "memory_id": r.get::<Uuid, _>("memory_id"),
                "from_status": r.get::<String, _>("from_status"),
                "to_status": r.get::<String, _>("to_status"),
                "policy_rule": r.get::<Option<String>, _>("policy_rule"),
                "age_secs": r.get::<i64, _>("age_secs"),
                "memory": r.get::<Option<String>, _>("content").map(|content| json!({
                    "content": content,
                    "kind": r.get::<Option<String>, _>("kind"),
                    "status": r.get::<Option<String>, _>("memory_status"),
                    "confidence": r.get::<Option<f32>, _>("confidence"),
                    "team": r.get::<Option<String>, _>("team"),
                })),
                "provenance": provenance,
            })
        })
        .collect();
    Ok(Json(json!({ "promotions": out })))
}

// ── ingestion status ────────────────────────────────────────────────────

/// GET /v1/sources/{id} — what happened to an async memory_add. Closes the
/// loop on the 202: the source row (RLS-scoped), the queue job state, and
/// what the pipeline produced from it.
async fn source_status(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
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
    let job_json = job.as_ref().map(|j| {
        json!({
            "state": j.get::<String, _>("state"),
            "attempts": j.get::<i32, _>("attempts"),
            "outcome": j.get::<Option<String>, _>("outcome"),
        })
    });
    // One-word rollup the caller can poll on.
    let status = match (&job_json, memories) {
        (Some(j), _) if j["state"] == "queued" && j["attempts"] == 0 => "queued",
        (Some(j), _) if j["state"] == "queued" => "retrying",
        (Some(j), _) if j["state"] == "archived" && j["outcome"] == "ok" => "processed",
        (Some(_), _) => "failed",
        (None, 0) => "unknown", // job vanished without output (pre-status enqueue)
        (None, _) => "processed",
    };
    Ok(Json(json!({
        "source_id": id,
        "status": status,
        "source": {
            "kind": source.get::<String, _>("kind"),
            "external_ref": source.get::<Option<String>, _>("external_ref"),
            "created_at": source.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        },
        "job": job_json,
        "results": {
            "memories": memories,
            "promoted": produced.get::<i64, _>("promoted"),
            "pending_review": produced.get::<i64, _>("pending_review"),
        },
    })))
}

// ── managed API tokens ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateTokenBody {
    name: String,
    /// Subset of read|write|admin; defaults to ["read"].
    #[serde(default)]
    scopes: Option<Vec<String>>,
    /// Principal the token acts as; defaults to the caller.
    #[serde(default)]
    user_id: Option<Uuid>,
}

/// POST /v1/tokens — mint a token. The secret appears ONCE in this response;
/// only its sha256 is stored. Requires the `admin` scope (env bootstrap
/// tokens qualify), so read/write tokens cannot mint tokens.
async fn create_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateTokenBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
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
        Json(json!({
            "id": id,
            "name": name,
            "prefix": prefix,
            "scopes": scopes,
            "user_id": user_id,
            // Shown exactly once — never retrievable again.
            "token": secret,
        })),
    ))
}

/// GET /v1/tokens — list the org's tokens (metadata only, never secrets).
async fn list_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let ctx = auth_of(&state, &headers, "admin").await?;
    let rows = brainiac_store::tokens::list(state.store.pool(), ctx.principal.org_id)
        .await
        .map_err(internal)?;
    Ok(Json(json!({
        "tokens": rows.iter().map(|t| json!({
            "id": t.id,
            "name": t.name,
            "prefix": t.prefix,
            "scopes": t.scopes,
            "created_at": t.created_at,
            "last_used_at": t.last_used_at,
            "revoked_at": t.revoked_at,
        })).collect::<Vec<_>>(),
    })))
}

/// POST /v1/tokens/{id}/revoke — kill a token immediately.
async fn revoke_token(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
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
    Ok(Json(json!({ "id": id, "revoked": true })))
}

pub(crate) fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
