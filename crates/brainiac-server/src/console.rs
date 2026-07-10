//! Console REST surface (ARCHITECTURE.md §5.2/§5.3) — the endpoints the
//! Next.js governance console consumes:
//!
//! - review actions: approve/reject promotions, resolve contradictions
//! - `GET /v1/graph` — entity graph with canonical hubs + evidence pointers
//! - `GET /v1/analytics` — governance health counters
//!
//! Governance rule (v0 slice of §2.5): promotion approve/reject and
//! contradiction supersede require the caller to be a **maintainer of the
//! owning team** (`team_members.role = 'maintainer'`). Everything here runs
//! under the caller's RLS transaction — a reviewer can only ever act on
//! memories they can read.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainiac_core::{MemoryStatus, Principal};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::http::{internal, principal_of, AppState};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/reviews/promotions/{id}/approve", post(approve))
        .route("/v1/reviews/promotions/{id}/reject", post(reject))
        .route("/v1/reviews/contradictions", get(list_contradictions))
        .route(
            "/v1/reviews/contradictions/{id}/resolve",
            post(resolve_contradiction),
        )
        .route("/v1/graph", get(graph))
        .route("/v1/analytics", get(analytics))
}

type HttpError = (StatusCode, String);

async fn is_maintainer(
    conn: &mut PgConnection,
    principal: &Principal,
    team_id: Uuid,
) -> Result<bool, HttpError> {
    let row = sqlx::query(
        "SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'maintainer'",
    )
    .bind(team_id)
    .bind(principal.user_id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?;
    Ok(row.is_some())
}

// ── promotions ──────────────────────────────────────────────────────────

struct PendingPromotion {
    memory_id: Uuid,
    to_status: String,
    team_id: Uuid,
}

/// Fetch a promotion that is still actionable, joined against the memory
/// under the caller's RLS (invisible memory ⇒ 404, not 403 — no oracle).
async fn actionable_promotion(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<PendingPromotion, HttpError> {
    let row = sqlx::query(
        "SELECT p.memory_id, p.to_status::text AS to_status, m.team_id
         FROM promotions p
         JOIN memories m ON m.id = p.memory_id
         WHERE p.id = $1 AND p.policy_decision = 'needs_review' AND p.reviewed_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?
    .ok_or((
        StatusCode::NOT_FOUND,
        "promotion not found or not pending".into(),
    ))?;
    Ok(PendingPromotion {
        memory_id: row.get("memory_id"),
        to_status: row.get("to_status"),
        team_id: row.get("team_id"),
    })
}

async fn review_promotion(
    state: &AppState,
    headers: &HeaderMap,
    id: Uuid,
    approve: bool,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(state, headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let pending = actionable_promotion(&mut tx, id).await?;
    if !is_maintainer(&mut tx, &principal, pending.team_id).await? {
        return Err((
            StatusCode::FORBIDDEN,
            "only a maintainer of the owning team can review promotions".into(),
        ));
    }
    let (decision, new_status) = if approve {
        let to = MemoryStatus::parse(&pending.to_status).ok_or_else(|| {
            internal(format!(
                "promotion carries bad to_status {}",
                pending.to_status
            ))
        })?;
        ("approved", to)
    } else {
        ("denied", MemoryStatus::Rejected)
    };
    sqlx::query(
        "UPDATE promotions SET policy_decision = $2, reviewer_id = $3, reviewed_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(decision)
    .bind(principal.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    brainiac_store::governance::set_memory_status(&mut tx, pending.memory_id, new_status)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({
        "promotion_id": id,
        "memory_id": pending.memory_id,
        "decision": decision,
        "memory_status": new_status.as_str(),
    })))
}

async fn approve(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    review_promotion(&state, &headers, id, true).await
}

async fn reject(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    review_promotion(&state, &headers, id, false).await
}

// ── contradictions ──────────────────────────────────────────────────────

async fn list_contradictions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    // LEFT JOIN: RLS-invisible memories render as null content rather than
    // hiding the contradiction row (the row itself is org-scoped).
    let rows = sqlx::query(
        "SELECT c.id, c.memory_a, c.memory_b, c.detected_by, c.resolution_note,
                ma.content AS content_a, mb.content AS content_b
         FROM contradictions c
         LEFT JOIN memories ma ON ma.id = c.memory_a
         LEFT JOIN memories mb ON mb.id = c.memory_b
         WHERE c.status = 'open'
         ORDER BY c.id
         LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "memory_a": {"id": r.get::<Uuid, _>("memory_a"), "content": r.get::<Option<String>, _>("content_a")},
                "memory_b": {"id": r.get::<Uuid, _>("memory_b"), "content": r.get::<Option<String>, _>("content_b")},
                "detected_by": r.get::<String, _>("detected_by"),
                "suggested_resolution": r.get::<Option<String>, _>("resolution_note"),
            })
        })
        .collect();
    Ok(Json(json!({ "contradictions": out })))
}

#[derive(Deserialize)]
struct ResolveBody {
    /// supersede | coexist | dismiss
    resolution: String,
    /// Required for supersede: the memory that wins.
    winner_memory_id: Option<Uuid>,
    note: Option<String>,
}

async fn resolve_contradiction(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ResolveBody>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let row = sqlx::query(
        "SELECT memory_a, memory_b FROM contradictions WHERE id = $1 AND status = 'open'",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?
    .ok_or((
        StatusCode::NOT_FOUND,
        "contradiction not found or not open".into(),
    ))?;
    let (a, b): (Uuid, Uuid) = (row.get("memory_a"), row.get("memory_b"));

    let status = match body.resolution.as_str() {
        "supersede" => {
            let winner = body.winner_memory_id.ok_or((
                StatusCode::BAD_REQUEST,
                "supersede requires winner_memory_id".into(),
            ))?;
            if winner != a && winner != b {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "winner_memory_id must be one of the contradiction's memories".into(),
                ));
            }
            let loser = if winner == a { b } else { a };
            // Supersession mutates the corpus — gate on the losing memory's
            // owning-team maintainer, under the caller's RLS view.
            let loser_row = sqlx::query("SELECT team_id FROM memories WHERE id = $1")
                .bind(loser)
                .fetch_optional(&mut *tx)
                .await
                .map_err(internal)?
                .ok_or((
                    StatusCode::NOT_FOUND,
                    "losing memory is not visible to you".into(),
                ))?;
            if !is_maintainer(&mut tx, &principal, loser_row.get("team_id")).await? {
                return Err((
                    StatusCode::FORBIDDEN,
                    "supersede requires a maintainer of the losing memory's team".into(),
                ));
            }
            sqlx::query(
                "UPDATE memories
                 SET valid_to = now(), superseded_by = $2,
                     status = 'deprecated'::memory_status, updated_at = now()
                 WHERE id = $1",
            )
            .bind(loser)
            .bind(winner)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
            "resolved_supersede"
        }
        "coexist" => "resolved_coexist",
        "dismiss" => "dismissed",
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown resolution `{other}` (supersede|coexist|dismiss)"),
            ))
        }
    };

    sqlx::query(
        "UPDATE contradictions
         SET status = $2, resolution_note = COALESCE($3, resolution_note),
             resolved_by = $4, resolved_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(body.note.as_deref())
    .bind(principal.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({ "contradiction_id": id, "status": status })))
}

// ── graph ───────────────────────────────────────────────────────────────

async fn graph(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let canonicals =
        sqlx::query("SELECT id, name, kind FROM canonical_entities ORDER BY name LIMIT 2000")
            .fetch_all(&mut *tx)
            .await
            .map_err(internal)?;
    let entities = sqlx::query(
        "SELECT e.id, e.name, e.kind, e.team_id, l.canonical_id
         FROM entities e
         LEFT JOIN entity_links l ON l.entity_id = e.id
         ORDER BY e.name
         LIMIT 2000",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    // Evidence content resolves under the caller's RLS; a hidden memory
    // leaves the edge visible (it's org metadata) with content null.
    let edges = sqlx::query(
        "SELECT ed.src_entity, ed.dst_entity, ed.relation, ed.memory_id, m.content AS evidence
         FROM edges ed
         LEFT JOIN memories m ON m.id = ed.memory_id
         LIMIT 5000",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    Ok(Json(json!({
        "canonicals": canonicals.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
        })).collect::<Vec<_>>(),
        "entities": entities.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "team_id": r.get::<Uuid, _>("team_id"),
            "canonical_id": r.get::<Option<Uuid>, _>("canonical_id"),
        })).collect::<Vec<_>>(),
        "edges": edges.iter().map(|r| json!({
            "src": r.get::<Uuid, _>("src_entity"),
            "dst": r.get::<Uuid, _>("dst_entity"),
            "relation": r.get::<String, _>("relation"),
            "memory_id": r.get::<Option<Uuid>, _>("memory_id"),
            "evidence": r.get::<Option<String>, _>("evidence"),
        })).collect::<Vec<_>>(),
    })))
}

// ── analytics ───────────────────────────────────────────────────────────

async fn analytics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers)?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    // Counts are the CALLER's view (RLS) — a member sees their slice of the
    // org, which is exactly what the console should show them.
    let by_status = sqlx::query(
        "SELECT status::text AS status, count(*) AS n FROM memories GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let review = sqlx::query(
        "SELECT count(*) AS pending,
                COALESCE(EXTRACT(EPOCH FROM now() - min(created_at)), 0)::bigint AS oldest_secs
         FROM promotions
         WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let contradictions_open: i64 =
        sqlx::query("SELECT count(*) AS n FROM contradictions WHERE status = 'open'")
            .fetch_one(&mut *tx)
            .await
            .map_err(internal)?
            .get("n");
    let entities: i64 = sqlx::query("SELECT count(*) AS n FROM entities")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");
    let canonicals: i64 = sqlx::query("SELECT count(*) AS n FROM canonical_entities")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");
    drop(tx);
    let queue_depth =
        brainiac_store::queue::depth(state.store.pool(), brainiac_pipeline::worker::INGEST_QUEUE)
            .await
            .map_err(internal)?;

    Ok(Json(json!({
        "memories_by_status": by_status.iter().map(|r| json!({
            "status": r.get::<String, _>("status"),
            "count": r.get::<i64, _>("n"),
        })).collect::<Vec<_>>(),
        "reviews": {
            "pending_promotions": review.get::<i64, _>("pending"),
            "oldest_pending_secs": review.get::<i64, _>("oldest_secs"),
            "open_contradictions": contradictions_open,
        },
        "graph": { "entities": entities, "canonicals": canonicals },
        "queue": { "ingest_depth": queue_depth },
        "embedding_model": state.embedder.model_name(),
    })))
}
