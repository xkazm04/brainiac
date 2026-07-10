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
use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::Principal;
use brainiac_store::Store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::TokenMap;

pub struct AppState {
    pub store: Store,
    pub embedder: DeterministicEmbedder,
    pub embedding_version: i32,
    pub tokens: TokenMap,
}

pub async fn router(store: Store, embedder: DeterministicEmbedder) -> Result<Router> {
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
        .route("/v1/reviews/promotions", get(pending_promotions))
        .with_state(state))
}

fn principal_of(state: &AppState, headers: &HeaderMap) -> Result<Principal, (StatusCode, String)> {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;
    state
        .tokens
        .resolve(bearer)
        .cloned()
        .ok_or((StatusCode::UNAUTHORIZED, "unknown token".into()))
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
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        &state.embedder,
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
    let principal = principal_of(&state, &headers)?;
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
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT p.id, p.memory_id, p.to_status::text AS to_status, p.policy_rule, p.created_at
         FROM promotions p
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
            json!({
                "id": r.get::<Uuid, _>("id"),
                "memory_id": r.get::<Uuid, _>("memory_id"),
                "to_status": r.get::<String, _>("to_status"),
                "policy_rule": r.get::<Option<String>, _>("policy_rule"),
            })
        })
        .collect();
    Ok(Json(json!({ "promotions": out })))
}

fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
