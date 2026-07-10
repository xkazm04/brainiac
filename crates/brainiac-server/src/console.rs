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

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainiac_core::{MemoryStatus, Principal};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::http::{auth_of, internal, principal_of, AppState};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/reviews/promotions/{id}/approve", post(approve))
        .route("/v1/reviews/promotions/{id}/reject", post(reject))
        .route("/v1/reviews/contradictions", get(list_contradictions))
        .route(
            "/v1/reviews/contradictions/{id}/resolve",
            post(resolve_contradiction),
        )
        .route("/v1/audit", get(audit))
        .route("/v1/graph", get(graph))
        .route("/v1/analytics", get(analytics))
        .route("/v1/analytics/observatory", get(observatory))
        .route("/v1/graph/overview", get(graph_overview))
        .route("/v1/graph/canonical/{id}", get(graph_canonical))
        .route("/v1/memories", get(memories_list))
        .route("/v1/memories/{id}", get(memory_detail))
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
    let principal = auth_of(state, headers, "write").await?.principal;
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

#[derive(Deserialize)]
struct ContradictionsQuery {
    /// open | resolved_supersede | resolved_coexist | dismissed | all (default open)
    status: Option<String>,
    /// Filter by detector (e.g. `embedding_similarity`, `llm`).
    detected_by: Option<String>,
    /// Only rows at least this many hours old (SLA aging view).
    min_age_hours: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_contradictions(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ContradictionsQuery>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let status = q.status.as_deref().unwrap_or("open");
    if !matches!(
        status,
        "open" | "resolved_supersede" | "resolved_coexist" | "dismissed" | "all"
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown status `{status}` (open|resolved_supersede|resolved_coexist|dismissed|all)"),
        ));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    // LEFT JOIN: RLS-invisible memories render as null content rather than
    // hiding the contradiction row (the row itself is org-scoped). Oldest
    // first — the aging queue surfaces SLA breaches at the top.
    let rows = sqlx::query(
        "SELECT c.id, c.memory_a, c.memory_b, c.detected_by, c.status,
                c.resolution_note, c.resolved_by, c.resolved_at, c.created_at,
                EXTRACT(EPOCH FROM now() - c.created_at)::bigint AS age_secs,
                ma.content AS content_a, mb.content AS content_b
         FROM contradictions c
         LEFT JOIN memories ma ON ma.id = c.memory_a
         LEFT JOIN memories mb ON mb.id = c.memory_b
         WHERE ($1 = 'all' OR c.status = $1)
           AND ($2::text IS NULL OR c.detected_by = $2)
           AND ($3::bigint IS NULL OR c.created_at <= now() - make_interval(hours => $3::int))
         ORDER BY c.created_at ASC, c.id
         LIMIT $4 OFFSET $5",
    )
    .bind(status)
    .bind(q.detected_by.as_deref())
    .bind(q.min_age_hours)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    // Status counts ignore the row filters — they power the queue's tabs.
    let counts = sqlx::query("SELECT status, count(*) AS n FROM contradictions GROUP BY 1")
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
                "status": r.get::<String, _>("status"),
                "suggested_resolution": r.get::<Option<String>, _>("resolution_note"),
                "resolved_by": r.get::<Option<Uuid>, _>("resolved_by"),
                "resolved_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "age_secs": r.get::<i64, _>("age_secs"),
            })
        })
        .collect();
    Ok(Json(json!({
        "contradictions": out,
        "counts": counts.iter().map(|r| json!({
            "status": r.get::<String, _>("status"),
            "count": r.get::<i64, _>("n"),
        })).collect::<Vec<_>>(),
    })))
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
    let principal = auth_of(&state, &headers, "write").await?.principal;
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

// ── audit trail ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
}

/// Reverse-chronological feed of governance actions: promotion reviews
/// (human and policy) and contradiction resolutions. Reuses the reviewer /
/// resolved-by columns both tables already carry; rows resolve under the
/// caller's RLS transaction so members see their org slice only.
async fn audit(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = sqlx::query(
        "SELECT * FROM (
            SELECT 'promotion_review' AS kind, p.id, p.memory_id,
                   NULL::uuid AS memory_b,
                   p.policy_decision AS outcome, p.policy_rule AS detail,
                   p.reviewer_id AS actor_id,
                   COALESCE(p.reviewed_at, p.created_at) AS at
            FROM promotions p
            WHERE p.reviewed_at IS NOT NULL OR p.policy_decision = 'auto_approved'
            UNION ALL
            SELECT 'contradiction_resolution' AS kind, c.id, c.memory_a AS memory_id,
                   c.memory_b,
                   c.status AS outcome, c.resolution_note AS detail,
                   c.resolved_by AS actor_id,
                   c.resolved_at AS at
            FROM contradictions c
            WHERE c.resolved_at IS NOT NULL
         ) audit
         ORDER BY at DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "kind": r.get::<String, _>("kind"),
                "id": r.get::<Uuid, _>("id"),
                "memory_id": r.get::<Uuid, _>("memory_id"),
                "memory_b": r.get::<Option<Uuid>, _>("memory_b"),
                "outcome": r.get::<String, _>("outcome"),
                "detail": r.get::<Option<String>, _>("detail"),
                "actor_id": r.get::<Option<Uuid>, _>("actor_id"),
                "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at"),
            })
        })
        .collect();
    Ok(Json(json!({ "events": out })))
}

// ── graph ───────────────────────────────────────────────────────────────

async fn graph(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
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
    let principal = principal_of(&state, &headers).await?;
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

// ── observatory (the dashboard module's richer payload) ─────────────────

async fn observatory(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let by_status = sqlx::query(
        "SELECT status::text AS status, count(*) AS n FROM memories GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Weekly flow: captured = memory rows created; promoted = human/auto
    // approvals. ISO week labels keep the two series joinable client-side.
    let captured = sqlx::query(
        "SELECT to_char(date_trunc('week', created_at), 'IYYY\"-W\"IW') AS week, count(*) AS n
         FROM memories GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let promoted = sqlx::query(
        "SELECT to_char(date_trunc('week', created_at), 'IYYY\"-W\"IW') AS week, count(*) AS n
         FROM promotions
         WHERE policy_decision IN ('auto_approved', 'approved')
         GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let by_kind = sqlx::query(
        "SELECT m.kind, t.name AS team, count(*) AS n
         FROM memories m JOIN teams t ON t.id = m.team_id
         GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Themes: canonical entities ranked by anchored memories + team spread.
    // Counts only — no memory content crosses RLS here.
    let top_entities = sqlx::query(
        "SELECT ce.name, ce.kind,
                count(DISTINCT me.memory_id) AS memories,
                count(DISTINCT e.team_id) AS teams
         FROM canonical_entities ce
         JOIN entity_links l ON l.canonical_id = ce.id
         JOIN entities e ON e.id = l.entity_id
         LEFT JOIN memory_entities me ON me.entity_id = e.id
         GROUP BY ce.id, ce.name, ce.kind
         ORDER BY memories DESC, teams DESC, ce.name
         LIMIT 12",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let review = sqlx::query(
        "SELECT count(*) FILTER (WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL) AS pending,
                COALESCE(EXTRACT(EPOCH FROM now() - min(created_at)
                    FILTER (WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL)), 0)::bigint AS oldest_secs,
                count(*) FILTER (WHERE reviewed_at IS NOT NULL) AS reviewed,
                COALESCE(EXTRACT(EPOCH FROM avg(reviewed_at - created_at)), 0)::bigint AS avg_latency_secs,
                count(*) FILTER (WHERE policy_decision = 'auto_approved') AS auto_promoted
         FROM promotions",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;

    let contradictions =
        sqlx::query("SELECT status, count(*) AS n FROM contradictions GROUP BY 1 ORDER BY 1")
            .fetch_all(&mut *tx)
            .await
            .map_err(internal)?;

    drop(tx);
    let queue_depth =
        brainiac_store::queue::depth(state.store.pool(), brainiac_pipeline::worker::INGEST_QUEUE)
            .await
            .map_err(internal)?;

    Ok(Json(json!({
        "totals": by_status.iter().map(|r| json!({
            "status": r.get::<String, _>("status"),
            "count": r.get::<i64, _>("n"),
        })).collect::<Vec<_>>(),
        "weekly": {
            "captured": captured.iter().map(|r| json!({
                "week": r.get::<String, _>("week"), "count": r.get::<i64, _>("n"),
            })).collect::<Vec<_>>(),
            "promoted": promoted.iter().map(|r| json!({
                "week": r.get::<String, _>("week"), "count": r.get::<i64, _>("n"),
            })).collect::<Vec<_>>(),
        },
        "by_kind": by_kind.iter().map(|r| json!({
            "kind": r.get::<String, _>("kind"),
            "team": r.get::<String, _>("team"),
            "count": r.get::<i64, _>("n"),
        })).collect::<Vec<_>>(),
        "top_entities": top_entities.iter().map(|r| json!({
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "memories": r.get::<i64, _>("memories"),
            "teams": r.get::<i64, _>("teams"),
        })).collect::<Vec<_>>(),
        "review": {
            "pending": review.get::<i64, _>("pending"),
            "oldest_pending_secs": review.get::<i64, _>("oldest_secs"),
            "reviewed": review.get::<i64, _>("reviewed"),
            "avg_latency_secs": review.get::<i64, _>("avg_latency_secs"),
            "auto_promoted": review.get::<i64, _>("auto_promoted"),
        },
        "contradictions": contradictions.iter().map(|r| json!({
            "status": r.get::<String, _>("status"),
            "count": r.get::<i64, _>("n"),
        })).collect::<Vec<_>>(),
        "queue": { "ingest_depth": queue_depth },
        "embedding_model": state.embedder.model_name(),
    })))
}

// ── cortex map (multi-level graph; never ships the whole graph at once) ──

/// L0/L1: team lobes with volumes, top canonical hubs with team spread, and
/// team-pair binding strength (shared canonicals). Bounded by construction.
async fn graph_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let teams = sqlx::query(
        "SELECT t.id, t.name,
                (SELECT count(*) FROM memories m WHERE m.team_id = t.id) AS memories,
                (SELECT count(*) FROM entities e WHERE e.team_id = t.id) AS entities
         FROM teams t ORDER BY t.name",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let canonicals = sqlx::query(
        "SELECT ce.id, ce.name, ce.kind,
                count(DISTINCT me.memory_id) AS memories,
                count(DISTINCT e.team_id) AS team_count,
                array_agg(DISTINCT e.team_id) AS team_ids
         FROM canonical_entities ce
         JOIN entity_links l ON l.canonical_id = ce.id
         JOIN entities e ON e.id = l.entity_id
         LEFT JOIN memory_entities me ON me.entity_id = e.id
         GROUP BY ce.id, ce.name, ce.kind
         ORDER BY memories DESC, team_count DESC, ce.name
         LIMIT 60",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Binding strength between team pairs = canonicals both teams link into.
    let team_links = sqlx::query(
        "SELECT ta.team_id AS a, tb.team_id AS b, count(DISTINCT ta.canonical_id) AS shared
         FROM (SELECT DISTINCT l.canonical_id, e.team_id
               FROM entity_links l JOIN entities e ON e.id = l.entity_id) ta
         JOIN (SELECT DISTINCT l.canonical_id, e.team_id
               FROM entity_links l JOIN entities e ON e.id = l.entity_id) tb
           ON ta.canonical_id = tb.canonical_id AND ta.team_id < tb.team_id
         GROUP BY 1, 2",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    Ok(Json(json!({
        "teams": teams.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "memories": r.get::<i64, _>("memories"),
            "entities": r.get::<i64, _>("entities"),
        })).collect::<Vec<_>>(),
        "canonicals": canonicals.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "memories": r.get::<i64, _>("memories"),
            "teams": r.get::<i64, _>("team_count"),
            "team_ids": r.get::<Vec<Uuid>, _>("team_ids"),
        })).collect::<Vec<_>>(),
        "team_links": team_links.iter().map(|r| json!({
            "a": r.get::<Uuid, _>("a"),
            "b": r.get::<Uuid, _>("b"),
            "shared": r.get::<i64, _>("shared"),
        })).collect::<Vec<_>>(),
    })))
}

/// L2/L3: one canonical entity's neighborhood — surface forms per team,
/// 1-hop evidence edges (content RLS-scoped), neighbor canonicals reachable
/// through those edges, and the anchored memories the caller may read.
async fn graph_canonical(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let canonical =
        sqlx::query("SELECT id, name, kind, summary FROM canonical_entities WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "canonical entity not found".into()))?;

    let surface_forms = sqlx::query(
        "SELECT e.id, e.name, e.kind, e.team_id, t.name AS team, l.confidence, l.method
         FROM entity_links l
         JOIN entities e ON e.id = l.entity_id
         JOIN teams t ON t.id = e.team_id
         WHERE l.canonical_id = $1
         ORDER BY t.name, e.name",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // 1-hop edges touching any surface form; evidence text under caller RLS.
    let edges = sqlx::query(
        "SELECT ed.src_entity, es.name AS src_name, ed.dst_entity, ds.name AS dst_name,
                ed.relation, ed.memory_id, m.content AS evidence
         FROM edges ed
         JOIN entities es ON es.id = ed.src_entity
         JOIN entities ds ON ds.id = ed.dst_entity
         LEFT JOIN memories m ON m.id = ed.memory_id
         WHERE ed.src_entity IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
            OR ed.dst_entity IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
         LIMIT 60",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Neighbor canonicals: the far end of those edges, resolved via links.
    let neighbors = sqlx::query(
        "SELECT ce.id, ce.name, ce.kind, count(*) AS shared_edges
         FROM edges ed
         JOIN entity_links far ON far.entity_id =
              CASE WHEN ed.src_entity IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
                   THEN ed.dst_entity ELSE ed.src_entity END
         JOIN canonical_entities ce ON ce.id = far.canonical_id
         WHERE ce.id <> $1
           AND (ed.src_entity IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
             OR ed.dst_entity IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1))
         GROUP BY ce.id, ce.name, ce.kind
         ORDER BY shared_edges DESC, ce.name
         LIMIT 12",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Anchored memories the CALLER can read (RLS filters the join).
    let memories = sqlx::query(
        "SELECT DISTINCT m.id, m.content, m.kind, m.status::text AS status, t.name AS team
         FROM memory_entities me
         JOIN memories m ON m.id = me.memory_id
         JOIN teams t ON t.id = m.team_id
         WHERE me.entity_id IN (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
         ORDER BY m.kind, m.id
         LIMIT 12",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    Ok(Json(json!({
        "canonical": {
            "id": canonical.get::<Uuid, _>("id"),
            "name": canonical.get::<String, _>("name"),
            "kind": canonical.get::<String, _>("kind"),
            "summary": canonical.get::<Option<String>, _>("summary"),
        },
        "surface_forms": surface_forms.iter().map(|r| json!({
            "entity_id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "team_id": r.get::<Uuid, _>("team_id"),
            "team": r.get::<String, _>("team"),
            "confidence": r.get::<Option<f32>, _>("confidence"),
            "method": r.get::<Option<String>, _>("method"),
        })).collect::<Vec<_>>(),
        "edges": edges.iter().map(|r| json!({
            "src": r.get::<Uuid, _>("src_entity"),
            "src_name": r.get::<String, _>("src_name"),
            "dst": r.get::<Uuid, _>("dst_entity"),
            "dst_name": r.get::<String, _>("dst_name"),
            "relation": r.get::<String, _>("relation"),
            "memory_id": r.get::<Option<Uuid>, _>("memory_id"),
            "evidence": r.get::<Option<String>, _>("evidence"),
        })).collect::<Vec<_>>(),
        "neighbors": neighbors.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "shared_edges": r.get::<i64, _>("shared_edges"),
        })).collect::<Vec<_>>(),
        "memories": memories.iter().map(|r| json!({
            "id": r.get::<Uuid, _>("id"),
            "content": r.get::<String, _>("content"),
            "kind": r.get::<String, _>("kind"),
            "status": r.get::<String, _>("status"),
            "team": r.get::<String, _>("team"),
        })).collect::<Vec<_>>(),
    })))
}

// ── archive (the memory ledger: as-of browsing + full lineage) ───────────

#[derive(Deserialize)]
struct MemoriesListParams {
    kind: Option<String>,
    status: Option<String>,
    team: Option<Uuid>,
    /// RFC3339. When set, returns rows VALID at that instant — including
    /// deprecated ones that were true then. The archive's time travel.
    as_of: Option<String>,
    #[serde(default = "default_list_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_list_limit() -> i64 {
    50
}

fn memory_row_json(r: &sqlx::postgres::PgRow) -> serde_json::Value {
    let ts = |col: &str| {
        r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
            .map(|d| d.to_rfc3339())
    };
    json!({
        "id": r.get::<Uuid, _>("id"),
        "content": r.get::<String, _>("content"),
        "kind": r.get::<String, _>("kind"),
        "status": r.get::<String, _>("status"),
        "visibility": r.get::<String, _>("visibility"),
        "team": r.get::<String, _>("team"),
        "team_id": r.get::<Uuid, _>("team_id"),
        "valid_from": ts("valid_from"),
        "valid_to": ts("valid_to"),
        "superseded_by": r.get::<Option<Uuid>, _>("superseded_by"),
        "created_at": ts("created_at"),
        "confidence": r.get::<Option<f32>, _>("confidence"),
    })
}

async fn memories_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<MemoriesListParams>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let as_of = match &p.as_of {
        None => None,
        Some(s) => Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|_| (StatusCode::BAD_REQUEST, "as_of must be RFC3339".into()))?
                .with_timezone(&chrono::Utc),
        ),
    };
    let limit = p.limit.clamp(1, 200);
    let offset = p.offset.max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    const FILTER: &str = "WHERE ($1::text IS NULL OR m.kind = $1)
           AND ($2::text IS NULL OR m.status = $2::memory_status)
           AND ($3::uuid IS NULL OR m.team_id = $3)
           AND ($4::timestamptz IS NULL OR
                ((m.valid_from IS NULL OR m.valid_from <= $4)
                 AND (m.valid_to IS NULL OR m.valid_to > $4)))";

    let rows = sqlx::query(&format!(
        "SELECT m.id, m.content, m.kind, m.status::text AS status,
                m.visibility::text AS visibility, t.name AS team, m.team_id,
                m.valid_from, m.valid_to, m.superseded_by, m.created_at, m.confidence
         FROM memories m JOIN teams t ON t.id = m.team_id
         {FILTER}
         ORDER BY m.created_at DESC, m.id
         LIMIT $5 OFFSET $6"
    ))
    .bind(&p.kind)
    .bind(&p.status)
    .bind(p.team)
    .bind(as_of)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let total: i64 = sqlx::query(&format!("SELECT count(*) AS n FROM memories m {FILTER}"))
        .bind(&p.kind)
        .bind(&p.status)
        .bind(p.team)
        .bind(as_of)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");

    Ok(Json(json!({
        "total": total,
        "memories": rows.iter().map(memory_row_json).collect::<Vec<_>>(),
    })))
}

async fn memory_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let row = sqlx::query(
        "SELECT m.id, m.content, m.kind, m.status::text AS status,
                m.visibility::text AS visibility, t.name AS team, m.team_id,
                m.valid_from, m.valid_to, m.superseded_by, m.created_at, m.confidence,
                pv.actor_kind, pv.actor_id, pv.model_ref,
                s.kind AS source_kind, s.external_ref AS source_ref
         FROM memories m
         JOIN teams t ON t.id = m.team_id
         LEFT JOIN provenance pv ON pv.id = m.provenance_id
         LEFT JOIN sources s ON s.id = pv.source_id
         WHERE m.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "memory not found".into()))?;

    let entities = sqlx::query(
        "SELECT e.name, e.kind, t.name AS team
         FROM memory_entities me
         JOIN entities e ON e.id = me.entity_id
         JOIN teams t ON t.id = e.team_id
         WHERE me.memory_id = $1
         ORDER BY t.name, e.name",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let promotions = sqlx::query(
        "SELECT from_status::text AS from_status, to_status::text AS to_status,
                policy_decision, policy_rule, reviewer_id, reviewed_at, created_at
         FROM promotions WHERE memory_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // Supersession lineage, both directions. RLS silently drops chain
    // members the caller can't read.
    let successors = sqlx::query(
        "WITH RECURSIVE chain AS (
             SELECT id, content, status, valid_from, valid_to, superseded_by, 0 AS depth
             FROM memories WHERE id = $1
             UNION ALL
             SELECT m.id, m.content, m.status, m.valid_from, m.valid_to, m.superseded_by, c.depth + 1
             FROM memories m JOIN chain c ON m.id = c.superseded_by
             WHERE c.depth < 8
         )
         SELECT id, content, status::text AS status, valid_from, valid_to, depth
         FROM chain WHERE depth > 0 ORDER BY depth",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let predecessors = sqlx::query(
        "WITH RECURSIVE chain AS (
             SELECT id, content, status, valid_from, valid_to, 0 AS depth
             FROM memories WHERE id = $1
             UNION ALL
             SELECT m.id, m.content, m.status, m.valid_from, m.valid_to, c.depth + 1
             FROM memories m JOIN chain c ON m.superseded_by = c.id
             WHERE c.depth < 8
         )
         SELECT id, content, status::text AS status, valid_from, valid_to, depth
         FROM chain WHERE depth > 0 ORDER BY depth DESC",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let ts = |r: &sqlx::postgres::PgRow, col: &str| {
        r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
            .map(|d| d.to_rfc3339())
    };
    let chain_json = |r: &sqlx::postgres::PgRow, dir: i64| {
        json!({
            "id": r.get::<Uuid, _>("id"),
            "content": r.get::<String, _>("content"),
            "status": r.get::<String, _>("status"),
            "valid_from": ts(r, "valid_from"),
            "valid_to": ts(r, "valid_to"),
            "depth": r.get::<i32, _>("depth") as i64 * dir,
        })
    };

    Ok(Json(json!({
        "memory": memory_row_json(&row),
        "provenance": row.get::<Option<String>, _>("actor_kind").map(|actor_kind| json!({
            "actor_kind": actor_kind,
            "actor_id": row.get::<String, _>("actor_id"),
            "model_ref": row.get::<Option<String>, _>("model_ref"),
            "source_kind": row.get::<Option<String>, _>("source_kind"),
            "source_ref": row.get::<Option<String>, _>("source_ref"),
        })),
        "entities": entities.iter().map(|r| json!({
            "name": r.get::<String, _>("name"),
            "kind": r.get::<String, _>("kind"),
            "team": r.get::<String, _>("team"),
        })).collect::<Vec<_>>(),
        "promotions": promotions.iter().map(|r| json!({
            "from_status": r.get::<String, _>("from_status"),
            "to_status": r.get::<String, _>("to_status"),
            "policy_decision": r.get::<String, _>("policy_decision"),
            "policy_rule": r.get::<Option<String>, _>("policy_rule"),
            "reviewed_at": ts(r, "reviewed_at"),
            "created_at": ts(r, "created_at"),
        })).collect::<Vec<_>>(),
        "chain": {
            "predecessors": predecessors.iter().map(|r| chain_json(r, -1)).collect::<Vec<_>>(),
            "successors": successors.iter().map(|r| chain_json(r, 1)).collect::<Vec<_>>(),
        },
    })))
}
