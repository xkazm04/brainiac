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
//!
//! Response typing: every handler returns a named `Serialize + ToSchema`
//! struct so the OpenAPI spec is derived from the code that actually emits
//! the bytes. The structs mirror the previous `json!` payloads EXACTLY —
//! including the two timestamp conventions in play (some columns are
//! stringified with `.to_rfc3339()` and are therefore typed `String`; others
//! pass a `DateTime<Utc>` straight through). Do not "unify" them: the console
//! and the integration tests pin the current shapes.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainiac_core::{MemoryStatus, Principal};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, principal_of, AppState, HttpError};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/reviews/promotions/{id}/approve", post(approve))
        .route("/v1/reviews/promotions/{id}/reject", post(reject))
        .route("/v1/reviews/promotions/bulk", post(bulk_review))
        .route("/v1/reviews/contradictions", get(list_contradictions))
        .route(
            "/v1/reviews/contradictions/{id}/resolve",
            post(resolve_contradiction),
        )
        .route("/v1/audit", get(audit))
        .route("/v1/graph", get(graph))
        .route("/v1/analytics", get(analytics))
        .route("/v1/analytics/observatory", get(observatory))
        .route("/v1/analytics/knowledge-health", get(knowledge_health))
        .route(
            "/v1/analytics/practice-divergence",
            get(practice_divergence),
        )
        .route(
            "/v1/analytics/knowledge-health/snapshot",
            post(knowledge_health_snapshot),
        )
        .route("/v1/graph/overview", get(graph_overview))
        .route("/v1/graph/canonical/{id}", get(graph_canonical))
        .route("/v1/memories", get(memories_list))
        .route("/v1/memories/validity", get(memories_validity))
        .route("/v1/memories/expiring", get(memories_expiring))
        .route("/v1/memories/{id}", get(memory_detail))
        .route("/v1/memories/{id}/reverify", post(memory_reverify))
        .route("/v1/reviews/feedback", get(feedback_queue))
        .route(
            "/v1/reviews/feedback/{id}/resolve",
            post(resolve_feedback_claims),
        )
        .route("/v1/sources", get(sources_list))
        .route("/v1/pipeline/runs", get(pipeline_runs))
        .route("/v1/org/users", get(org_users))
        .route("/v1/tokens/preview", post(token_preview))
        // operator sweeps (admin) — schedule the periodic org-intelligence scans
        .route("/v1/ops/sweeps", get(crate::sweeps::sweeps_list))
        .route(
            "/v1/ops/sweeps/{kind}",
            axum::routing::put(crate::sweeps::sweep_update),
        )
        .route("/v1/ops/sweeps/{kind}/run", post(crate::sweeps::sweep_run))
        // ── the knowledge base (§8): pages are projections over memories ──
        .route("/v1/docs", get(crate::docs::docs_list))
        .route("/v1/docs/{slug}", get(crate::docs::doc_get))
        .route("/v1/docs/{slug}/revisions", get(crate::docs::doc_revisions))
        .route("/v1/docs/{slug}/edit", post(crate::docs::doc_edit))
        .route(
            "/v1/docs/revisions/{id}/approve",
            post(crate::docs::doc_approve),
        )
}

/// `{status, count}` — the shape every status histogram in this module emits
/// (memories-by-status, contradiction tabs).
#[derive(Serialize, ToSchema)]
pub(crate) struct StatusCount {
    pub status: String,
    pub count: i64,
}

pub(crate) async fn is_maintainer(
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

/// Outcome of an approve/reject: the promotion, the memory it moved, and the
/// status that memory now carries.
#[derive(Serialize, ToSchema)]
pub(crate) struct ReviewDecisionResponse {
    pub promotion_id: Uuid,
    pub memory_id: Uuid,
    /// `approved` | `denied`
    pub decision: String,
    /// The memory's status after the decision.
    pub memory_status: String,
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
         WHERE p.id = $1 AND p.policy_decision = 'needs_review' AND p.reviewed_at IS NULL
         FOR UPDATE OF p",
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
) -> Result<Json<ReviewDecisionResponse>, HttpError> {
    let principal = auth_of(state, headers, "write").await?.principal;
    Ok(Json(review_one(state, &principal, id, approve).await?))
}

/// Decide ONE promotion, in its own RLS transaction: the maintainer gate, the
/// `FOR UPDATE` read, the self-guarding transition and the phantom-approval
/// rollback.
///
/// This is a function rather than the body of the single-item handler because
/// the bulk endpoint calls it once per id. That is the whole design of bulk:
/// every gate here runs per item, under that item's own transaction, so a batch
/// is exactly N single reviews and cannot be a cheaper path to the same writes.
/// A batch-shaped query (`WHERE id = ANY($1)`) would have been one round trip
/// and one authorization decision for N teams — which is how a bulk endpoint
/// quietly becomes a way around the gate it is supposed to honour.
async fn review_one(
    state: &AppState,
    principal: &Principal,
    id: Uuid,
    approve: bool,
) -> Result<ReviewDecisionResponse, HttpError> {
    let principal = principal.clone();
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let pending = actionable_promotion(&mut tx, id).await?;
    if !is_maintainer(&mut tx, &principal, pending.team_id).await? {
        return Err((
            StatusCode::FORBIDDEN,
            "only a maintainer of the owning team can review promotions".into(),
        )
            .into());
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
    // Self-guarding transition: re-assert `reviewed_at IS NULL` in the WHERE so a
    // promotion already decided by a concurrent approve/reject (or a double-submit)
    // updates 0 rows. Combined with the `FOR UPDATE OF p` in actionable_promotion,
    // the second request either blocks-then-404s at the read or lands here with
    // rows_affected == 0 — never a last-writer-wins reviewer or a double
    // set_memory_status that leaves the memory in a nondeterministic status.
    let updated = sqlx::query(
        "UPDATE promotions SET policy_decision = $2, reviewer_id = $3, reviewed_at = now()
         WHERE id = $1 AND reviewed_at IS NULL",
    )
    .bind(id)
    .bind(decision)
    .bind(principal.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?
    .rows_affected();
    if updated == 0 {
        return Err((
            StatusCode::CONFLICT,
            "promotion was already reviewed".into(),
        )
            .into());
    }
    // If the memory changed 0 rows (hard-deleted since the promotion was queued,
    // or out of RLS scope) the status never actually moved — reject rather than
    // commit a phantom approval. Returning before commit rolls back the promotion
    // stamp above, keeping the audit trail and the memory's real status in sync.
    let status_changed =
        brainiac_store::governance::set_memory_status(&mut tx, pending.memory_id, new_status)
            .await
            .map_err(internal)?;
    if !status_changed {
        return Err((
            StatusCode::CONFLICT,
            "the memory no longer exists or is out of scope — nothing was approved".into(),
        )
            .into());
    }
    tx.commit().await.map_err(internal)?;
    Ok(ReviewDecisionResponse {
        promotion_id: id,
        memory_id: pending.memory_id,
        decision: decision.to_string(),
        memory_status: new_status.as_str().to_string(),
    })
}

#[utoipa::path(
    post,
    path = "/v1/reviews/promotions/{id}/approve",
    tag = "reviews",
    description = "Approve a pending promotion: moves the memory to the promotion's target status. Requires a maintainer of the owning team.",
    params(("id" = Uuid, Path, description = "Promotion id")),
    responses(
        (status = 200, description = "Promotion approved", body = ReviewDecisionResponse),
        (status = 403, description = "Caller is not a maintainer of the owning team"),
        (status = 404, description = "Promotion not found or no longer pending"),
    )
)]
pub(crate) async fn approve(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ReviewDecisionResponse>, HttpError> {
    review_promotion(&state, &headers, id, true).await
}

#[utoipa::path(
    post,
    path = "/v1/reviews/promotions/{id}/reject",
    tag = "reviews",
    description = "Reject a pending promotion: the memory is marked rejected. Requires a maintainer of the owning team.",
    params(("id" = Uuid, Path, description = "Promotion id")),
    responses(
        (status = 200, description = "Promotion rejected", body = ReviewDecisionResponse),
        (status = 403, description = "Caller is not a maintainer of the owning team"),
        (status = 404, description = "Promotion not found or no longer pending"),
    )
)]
pub(crate) async fn reject(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ReviewDecisionResponse>, HttpError> {
    review_promotion(&state, &headers, id, false).await
}

// ── bulk review ─────────────────────────────────────────────────────────

/// The most promotions one request may decide.
///
/// Not a performance number — a governance one. The queue's whole purpose is
/// that a human looked at each claim, and an endpoint that accepts "all 5000"
/// in one call is an endpoint that will eventually be used that way. 200 is the
/// page the console renders (see the reviews module's PAGE), so the ceiling is
/// "everything you can currently see", which is the largest batch a reviewer can
/// honestly claim to have read.
const BULK_MAX: usize = 200;

#[derive(Deserialize, ToSchema)]
pub(crate) struct BulkReviewRequest {
    /// `approve` | `reject` — applied to every id in the batch.
    pub action: String,
    /// The promotions to decide. Duplicates are collapsed.
    pub ids: Vec<Uuid>,
}

/// What happened to ONE promotion in a batch.
///
/// `status` is the HTTP status this id would have returned as a single request,
/// so a partial failure stays legible per row: 403 (not a maintainer of THAT
/// item's team), 404 (gone, or invisible under RLS), 409 (someone decided it
/// first). A batch does not collapse these into one error, because they are not
/// one error — "3 of 12 were not yours" is a different fact from "the request
/// failed", and only the per-row shape can say it.
#[derive(Serialize, ToSchema)]
pub(crate) struct BulkReviewRow {
    pub promotion_id: Uuid,
    pub ok: bool,
    /// The status this item would have returned on its own (200/403/404/409/500).
    pub status: u16,
    pub memory_id: Option<Uuid>,
    /// The memory's status after the decision — `None` when this row failed.
    pub memory_status: Option<String>,
    /// Why this row failed — `None` when it succeeded.
    pub error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BulkReviewResponse {
    /// Rows that were actually written.
    pub decided: usize,
    /// Rows that were refused or lost a race. The batch still returns 200: the
    /// request succeeded, and `results` says what became of each id.
    pub failed: usize,
    pub results: Vec<BulkReviewRow>,
}

#[utoipa::path(
    post,
    path = "/v1/reviews/promotions/bulk",
    tag = "reviews",
    description = "Approve or reject many promotions in one request. Every item is authorized and decided independently — the maintainer gate and the concurrency guard run per item, and the response reports each id's outcome. Returns 200 even when some rows fail; read `results`.",
    request_body = BulkReviewRequest,
    responses(
        (status = 200, description = "Per-item outcomes (some may have failed)", body = BulkReviewResponse),
        (status = 400, description = "Unknown action, empty batch, or more than 200 ids"),
        (status = 401, description = "Missing or unknown bearer token"),
        (status = 403, description = "Token lacks the `write` scope"),
    )
)]
pub(crate) async fn bulk_review(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<BulkReviewRequest>,
) -> Result<Json<BulkReviewResponse>, HttpError> {
    let approve = match req.action.as_str() {
        "approve" => true,
        "reject" => false,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown action {other:?} — expected approve or reject"),
            )
                .into())
        }
    };
    if req.ids.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no promotions given".into()).into());
    }
    if req.ids.len() > BULK_MAX {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "{} promotions in one batch — the maximum is {BULK_MAX}",
                req.ids.len()
            ),
        )
            .into());
    }
    // `write` scope once — it is a property of the token, not of an item. The
    // per-TEAM maintainer gate is emphatically NOT hoisted here: it runs inside
    // review_one, per item, because the batch may span teams.
    let principal = auth_of(&state, &headers, "write").await?.principal;

    let mut seen = std::collections::HashSet::new();
    let ids: Vec<Uuid> = req.ids.into_iter().filter(|id| seen.insert(*id)).collect();

    // Sequential, deliberately. Each review_one takes `FOR UPDATE` on its row;
    // firing a batch concurrently would have N transactions racing for
    // overlapping locks against a queue another operator may also be draining.
    // A batch of 200 is not a latency problem worth a deadlock.
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        match review_one(&state, &principal, id, approve).await {
            Ok(d) => results.push(BulkReviewRow {
                promotion_id: id,
                ok: true,
                status: StatusCode::OK.as_u16(),
                memory_id: Some(d.memory_id),
                memory_status: Some(d.memory_status),
                error: None,
            }),
            // One item's refusal is that item's business. Keep going: the
            // alternative is that a single 403 in a batch of 50 discards 49
            // legitimate decisions and tells the operator nothing about which.
            Err(e) => results.push(BulkReviewRow {
                promotion_id: id,
                ok: false,
                status: e.status.as_u16(),
                memory_id: None,
                memory_status: None,
                error: Some(e.message),
            }),
        }
    }
    let decided = results.iter().filter(|r| r.ok).count();
    Ok(Json(BulkReviewResponse {
        decided,
        failed: results.len() - decided,
        results,
    }))
}

// ── contradictions ──────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct ContradictionsQuery {
    /// open | resolved_supersede | resolved_coexist | dismissed | all (default open)
    status: Option<String>,
    /// Filter by detector (e.g. `embedding_similarity`, `llm`).
    detected_by: Option<String>,
    /// Only rows at least this many hours old (SLA aging view).
    min_age_hours: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// One side of a contradiction. `content` is null when the memory is not
/// visible to the caller under RLS (the row itself is org-scoped metadata).
#[derive(Serialize, ToSchema)]
pub(crate) struct ContradictionMemoryRef {
    pub id: Uuid,
    pub content: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ContradictionRow {
    pub id: Uuid,
    pub memory_a: ContradictionMemoryRef,
    pub memory_b: ContradictionMemoryRef,
    pub detected_by: String,
    pub status: String,
    /// `contradictions.resolution_note` — the detector's suggestion, or the
    /// resolver's note once answered.
    pub suggested_resolution: Option<String>,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub age_secs: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ContradictionQueueResponse {
    pub contradictions: Vec<ContradictionRow>,
    /// Rows matching the CURRENT filters, ignoring the page window — the real
    /// backlog behind this page. Without it a client can only report the length
    /// of a truncated array, which silently reads as the whole queue the moment
    /// the backlog passes `limit`.
    pub total: i64,
    /// Status histogram over ALL contradictions — unaffected by the filters.
    pub counts: Vec<StatusCount>,
}

#[utoipa::path(
    get,
    path = "/v1/reviews/contradictions",
    tag = "reviews",
    description = "The contradiction queue: oldest first (SLA aging), plus a status histogram that powers the queue's tabs.",
    params(
        ("status" = Option<String>, Query, description = "open | resolved_supersede | resolved_coexist | dismissed | all (default open)"),
        ("detected_by" = Option<String>, Query, description = "Filter by detector (e.g. embedding_similarity, llm)"),
        ("min_age_hours" = Option<i64>, Query, description = "Only rows at least this many hours old"),
        ("limit" = Option<i64>, Query, description = "Page size (default 50, clamped 1..200)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
    ),
    responses(
        (status = 200, description = "Contradiction queue page", body = ContradictionQueueResponse),
        (status = 400, description = "Unknown status filter"),
    )
)]
pub(crate) async fn list_contradictions(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ContradictionsQuery>,
    headers: HeaderMap,
) -> Result<Json<ContradictionQueueResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let status = q.status.as_deref().unwrap_or("open");
    if !matches!(
        status,
        "open" | "resolved_supersede" | "resolved_coexist" | "dismissed" | "all"
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown status `{status}` (open|resolved_supersede|resolved_coexist|dismissed|all)"),
        )
            .into());
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
    // The filtered backlog: the same predicate as the page query, without the
    // window. Counted over `contradictions` alone — the LEFT JOINs above only
    // decorate rows with content and never filter, so they cannot change it.
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contradictions c
         WHERE ($1 = 'all' OR c.status = $1)
           AND ($2::text IS NULL OR c.detected_by = $2)
           AND ($3::bigint IS NULL OR c.created_at <= now() - make_interval(hours => $3::int))",
    )
    .bind(status)
    .bind(q.detected_by.as_deref())
    .bind(q.min_age_hours)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    // Status counts ignore the row filters — they power the queue's tabs.
    let counts = sqlx::query("SELECT status, count(*) AS n FROM contradictions GROUP BY 1")
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?;
    let out: Vec<ContradictionRow> = rows
        .iter()
        .map(|r| ContradictionRow {
            id: r.get("id"),
            memory_a: ContradictionMemoryRef {
                id: r.get("memory_a"),
                content: r.get("content_a"),
            },
            memory_b: ContradictionMemoryRef {
                id: r.get("memory_b"),
                content: r.get("content_b"),
            },
            detected_by: r.get("detected_by"),
            status: r.get("status"),
            suggested_resolution: r.get("resolution_note"),
            resolved_by: r.get("resolved_by"),
            resolved_at: r.get("resolved_at"),
            created_at: r.get("created_at"),
            age_secs: r.get("age_secs"),
        })
        .collect();
    Ok(Json(ContradictionQueueResponse {
        contradictions: out,
        total,
        counts: counts
            .iter()
            .map(|r| StatusCount {
                status: r.get("status"),
                count: r.get("n"),
            })
            .collect(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ResolveBody {
    /// supersede | coexist | dismiss
    resolution: String,
    /// Required for supersede: the memory that wins.
    winner_memory_id: Option<Uuid>,
    note: Option<String>,
}

/// The contradiction's new terminal status (`resolved_supersede` |
/// `resolved_coexist` | `dismissed`).
#[derive(Serialize, ToSchema)]
pub(crate) struct ResolveContradictionResponse {
    pub contradiction_id: Uuid,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/v1/reviews/contradictions/{id}/resolve",
    tag = "reviews",
    description = "Resolve an open contradiction by supersede (deprecates the loser), coexist, or dismiss. Supersede requires a maintainer of the losing memory's team.",
    params(("id" = Uuid, Path, description = "Contradiction id")),
    request_body = ResolveBody,
    responses(
        (status = 200, description = "Contradiction resolved", body = ResolveContradictionResponse),
        (status = 400, description = "Unknown resolution, or supersede without a valid winner_memory_id"),
        (status = 403, description = "Supersede requires a maintainer of the losing memory's team"),
        (status = 404, description = "Contradiction not found / not open, or the losing memory is not visible"),
        (status = 409, description = "Lost a concurrent resolve, or the supersession could not be applied — nothing was resolved"),
    )
)]
pub(crate) async fn resolve_contradiction(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ResolveBody>,
) -> Result<Json<ResolveContradictionResponse>, HttpError> {
    let principal = auth_of(&state, &headers, "write").await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    // `FOR UPDATE` is the serialization point, exactly as in
    // actionable_promotion: two maintainers resolving the same dispute both
    // used to pass this read and both write, so the ledger recorded only the
    // last writer while the first one's supersession side-effects had already
    // landed. The loser of the race now blocks here, and — because READ
    // COMMITTED re-evaluates the WHERE against the committed row once the lock
    // is released — sees `status <> 'open'` and 404s before it can act.
    let row = sqlx::query(
        "SELECT memory_a, memory_b FROM contradictions WHERE id = $1 AND status = 'open'
         FOR UPDATE",
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
                )
                    .into());
            }
            let loser = if winner == a { b } else { a };
            // Supersession mutates the corpus — gate on the losing memory's
            // owning-team maintainer, under the caller's RLS view.
            let loser_row =
                sqlx::query("SELECT team_id, superseded_by FROM memories WHERE id = $1")
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
                )
                    .into());
            }
            // Store-owned primitive: deprecates the loser, closes valid_to,
            // sets superseded_by, AND records the transition in the
            // promotions audit log — the inline SQL this replaces skipped
            // the audit row.
            //
            // `apply_supersession` is idempotent and reports `false` whenever it
            // applied nothing — but `false` conflates two very different worlds,
            // and the caller must not treat them alike:
            //
            //   * the loser already points at THIS winner — the outcome the
            //     reviewer is asking for already holds. Nothing to apply, and
            //     nothing wrong: record `resolved_supersede` and close the
            //     dispute. Refusing here would strand such a dispute open
            //     forever, un-resolvable by the one verdict that fits it.
            //   * the loser points at a DIFFERENT winner (or is out of scope) —
            //     the request contradicts the corpus. Refuse.
            //
            // Deciding this from the loser's own `superseded_by` (read above,
            // under the same row lock) keeps the distinction here rather than
            // widening the store's bool into a status enum.
            let already: Option<Uuid> = loser_row.get("superseded_by");
            if already != Some(winner) {
                // Discarding this bool was the original defect: a dispute was
                // logged `resolved_supersede` while the corpus was untouched.
                // Returning before the commit rolls the transaction back, so the
                // contradiction stays open and honestly re-reviewable.
                let applied = brainiac_store::governance::apply_supersession(
                    &mut tx,
                    principal.org_id,
                    loser,
                    winner,
                    Some(principal.user_id),
                    "contradiction_supersede",
                )
                .await
                .map_err(internal)?;
                if !applied {
                    return Err((
                        StatusCode::CONFLICT,
                        "supersession could not be applied — the losing memory is already \
                         superseded by a different memory, or is out of scope; nothing was resolved"
                            .into(),
                    )
                        .into());
                }
            }
            "resolved_supersede"
        }
        "coexist" => "resolved_coexist",
        "dismiss" => "dismissed",
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown resolution `{other}` (supersede|coexist|dismiss)"),
            )
                .into())
        }
    };

    // Self-guarding transition, mirroring review_promotion: re-assert
    // `status = 'open'` so a dispute already resolved by a concurrent request
    // updates 0 rows rather than overwriting the recorded resolver. With the
    // `FOR UPDATE` above this should be unreachable — it is the belt to that
    // lock's braces, and the thing that makes the guarantee hold even if the
    // read is ever refactored.
    let updated = sqlx::query(
        "UPDATE contradictions
         SET status = $2, resolution_note = COALESCE($3, resolution_note),
             resolved_by = $4, resolved_at = now()
         WHERE id = $1 AND status = 'open'",
    )
    .bind(id)
    .bind(status)
    .bind(body.note.as_deref())
    .bind(principal.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?
    .rows_affected();
    if updated == 0 {
        return Err((
            StatusCode::CONFLICT,
            "contradiction was already resolved".into(),
        )
            .into());
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(ResolveContradictionResponse {
        contradiction_id: id,
        status: status.to_string(),
    }))
}

// ── feedback triage (agent verdicts a maintainer must answer) ───────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct FeedbackQueueQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    /// Filter to one memory kind (`fact`, `decision`, …).
    kind: Option<String>,
    /// Filter to one owning team.
    team_id: Option<Uuid>,
    /// Only disputes whose oldest open claim has stood at least this long.
    min_age_hours: Option<i64>,
    /// Only memories carrying at least this many open claims.
    min_claims: Option<i64>,
    /// Decay band: `past` | `d30` | `d90` | `d180` | `far` | `none`.
    band: Option<String>,
    /// Project id, or `none` for org-shared memories (PR2).
    project: Option<String>,
}

/// One filter value and the disputed-memory count behind it — so a filter
/// control can show what each option would leave and never offer one that
/// empties the queue.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackFacet {
    pub value: String,
    pub label: String,
    pub count: i64,
}

/// The facet breakdown of the FULL backlog, ignoring the current filter — the
/// menu, which must not shrink as the operator narrows or there is nothing left
/// to widen back out with.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackFacets {
    pub kinds: Vec<FeedbackFacet>,
    pub teams: Vec<FeedbackFacet>,
    pub bands: Vec<FeedbackFacet>,
    /// Value is a project id or `"none"`; label the name or `"org-shared"`.
    pub projects: Vec<FeedbackFacet>,
}

/// Open claim counts against one memory.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackClaims {
    pub wrong: i64,
    pub outdated: i64,
}

/// One open claim and the reporter behind it. Deprecating an org memory on an
/// unattributed tally is guessing; this is what makes it a decision.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackReport {
    /// wrong | outdated
    pub verdict: String,
    pub note: Option<String>,
    pub reporter_id: Uuid,
    /// Null when the org holds no email for the reporter.
    pub reporter_email: Option<String>,
    /// The reporter sits on the memory's owning team. Always false for
    /// org-wide memories — there is no owning team to sit on.
    pub reporter_on_owning_team: bool,
    /// How long ago this claim was filed. Seconds-since, like every other age
    /// on this payload.
    pub age_secs: i64,
}

/// The provenance record behind the disputed memory. Whole object null (not
/// omitted) when the memory has none — mirrors `MemoryProvenance` on the
/// detail endpoint.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackProvenance {
    pub actor_kind: String,
    pub actor_id: String,
    pub model_ref: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FlaggedMemory {
    pub memory_id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub team_id: Option<Uuid>,
    /// The owning team's name; null for org-wide memories.
    pub team: Option<String>,
    /// Project display name; null = org-shared (PR2).
    pub project: Option<String>,
    pub project_id: Option<Uuid>,
    pub confidence: Option<f32>,
    pub valid_to: Option<DateTime<Utc>>,
    pub provenance: Option<FeedbackProvenance>,
    pub claims: FeedbackClaims,
    /// DISTINCT reporters behind the open claims — the number that says whether
    /// a tally of five is five people or one agent five times.
    pub reporters: i64,
    /// The open claims themselves (most recent first, capped server-side).
    pub reports: Vec<FeedbackReport>,
    /// Age of the OLDEST open claim — how long the dispute has stood.
    pub oldest_claim_secs: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackQueueResponse {
    /// Memories matching the current filter, ignoring the page window — the
    /// real backlog. `flagged.len()` is only the page; rendering it as the
    /// queue depth understates the moment the backlog passes `limit`.
    pub total: i64,
    /// The facet menu for building a filter — counts over the FULL backlog, so
    /// it never shrinks as the operator narrows.
    pub facets: FeedbackFacets,
    pub flagged: Vec<FlaggedMemory>,
}

/// Memories carrying unresolved `wrong` / `outdated` claims from the agents
/// and operators who were served them — most-disputed first. This is where
/// MCP memory_feedback verdicts land for a human.
#[utoipa::path(
    get,
    path = "/v1/reviews/feedback",
    tag = "reviews",
    description = "Memories carrying unresolved wrong/outdated claims from the agents and operators served them, most-disputed first.",
    params(
        ("limit" = Option<i64>, Query, description = "Page size (default 50, clamped 1..200)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
        ("kind" = Option<String>, Query, description = "Filter to one memory kind"),
        ("team_id" = Option<Uuid>, Query, description = "Filter to one owning team"),
        ("min_age_hours" = Option<i64>, Query, description = "Oldest claim at least this old"),
        ("min_claims" = Option<i64>, Query, description = "At least this many open claims"),
        ("band" = Option<String>, Query, description = "Decay band: past|d30|d90|d180|far|none"),
        ("project" = Option<String>, Query, description = "Filter by project id, or `none` for org-shared"),
    ),
    responses(
        (status = 200, description = "Feedback triage queue page", body = FeedbackQueueResponse),
        (status = 400, description = "Unknown band value"),
    )
)]
pub(crate) async fn feedback_queue(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FeedbackQueueQuery>,
    headers: HeaderMap,
) -> Result<Json<FeedbackQueueResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let filter = brainiac_store::feedback::FlaggedFilter {
        kind: q.kind,
        team_id: q.team_id,
        min_age_hours: q.min_age_hours,
        min_claims: q.min_claims,
        band: q.band,
        project: q.project,
    };
    // A typo'd band would match no rows and read as "all clear" — the most
    // dangerous empty state a triage queue can show. Refuse it instead.
    if !filter.band_is_valid() {
        return Err((
            StatusCode::BAD_REQUEST,
            "unknown band (past|d30|d90|d180|far|none)".into(),
        )
            .into());
    }
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = brainiac_store::feedback::flagged(&mut tx, &filter, limit, offset)
        .await
        .map_err(internal)?;
    // `total` tracks the SAME filter as the rows, so "23 disputed" and the rows
    // paged through never disagree. `facets` ignores it — it is the menu.
    let total = brainiac_store::feedback::flagged_count(&mut tx, &filter)
        .await
        .map_err(internal)?;
    let (kinds, teams, bands, projects) = brainiac_store::feedback::flagged_facets(&mut tx)
        .await
        .map_err(internal)?;
    let facet = |f: brainiac_store::feedback::Facet| FeedbackFacet {
        value: f.value,
        label: f.label,
        count: f.count,
    };
    let facets = FeedbackFacets {
        kinds: kinds.into_iter().map(facet).collect(),
        teams: teams.into_iter().map(facet).collect(),
        bands: bands.into_iter().map(facet).collect(),
        projects: projects.into_iter().map(facet).collect(),
    };
    Ok(Json(FeedbackQueueResponse {
        total,
        facets,
        flagged: rows
            .iter()
            .map(|f| FlaggedMemory {
                memory_id: f.memory_id,
                title: f.title.clone(),
                content: f.content.clone(),
                kind: f.kind.clone(),
                status: f.status.clone(),
                team_id: f.team_id,
                team: f.team.clone(),
                project: f.project.clone(),
                project_id: f.project_id,
                confidence: f.confidence,
                valid_to: f.valid_to,
                provenance: f.provenance.as_ref().map(|p| FeedbackProvenance {
                    actor_kind: p.actor_kind.clone(),
                    actor_id: p.actor_id.clone(),
                    model_ref: p.model_ref.clone(),
                }),
                claims: FeedbackClaims {
                    wrong: f.wrong,
                    outdated: f.outdated,
                },
                reporters: f.reporters,
                reports: f
                    .reports
                    .iter()
                    .map(|c| FeedbackReport {
                        verdict: c.verdict.clone(),
                        note: c.note.clone(),
                        reporter_id: c.reporter_id,
                        reporter_email: c.reporter_email.clone(),
                        reporter_on_owning_team: c.reporter_on_owning_team,
                        age_secs: c.age_secs,
                    })
                    .collect(),
                oldest_claim_secs: f.oldest_claim_secs,
            })
            .collect(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ResolveFeedbackBody {
    /// reverified | deprecated | dismissed
    resolution: String,
    /// For `reverified`: the new validity budget (defaults to the kind TTL).
    days: Option<i64>,
    /// Why. Carried into the audit trail as the decision's rationale — the
    /// answer to "who deprecated this org memory, and on what grounds?".
    /// Recorded against the claims being closed, never over the reporter's note.
    note: Option<String>,
}

/// `valid_to` is null for `deprecated`/`dismissed` (only `reverified` moves
/// the boundary) — null, never absent.
#[derive(Serialize, ToSchema)]
pub(crate) struct ResolveFeedbackResponse {
    pub memory_id: Uuid,
    pub resolution: String,
    pub claims_closed: u64,
    pub valid_to: Option<DateTime<Utc>>,
}

/// Answer the open claims against a memory. The three answers are the three
/// things a maintainer can actually mean:
///   reverified — checked it, still true → extend its validity window
///   deprecated — the reporters are right → end it now, drop it from retrieval
///   dismissed  — the reports are noise → leave the memory as-is
/// Whichever is chosen, every open claim on that memory closes with it.
#[utoipa::path(
    post,
    path = "/v1/reviews/feedback/{id}/resolve",
    tag = "reviews",
    description = "Answer every open feedback claim against a memory: reverified (extend validity), deprecated (end it now), or dismissed (memory stands).",
    params(("id" = Uuid, Path, description = "Memory id")),
    request_body = ResolveFeedbackBody,
    responses(
        (status = 200, description = "Claims closed", body = ResolveFeedbackResponse),
        (status = 400, description = "Unknown resolution"),
        (status = 403, description = "Caller is not a maintainer of the owning team (or, for an org-wide memory, of any team)"),
        (status = 404, description = "Memory not found (or invisible under RLS)"),
        (status = 409, description = "Nothing to answer: the claims were already closed by a concurrent maintainer, or the memory's state forbids this resolution"),
    )
)]
pub(crate) async fn resolve_feedback_claims(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ResolveFeedbackBody>,
) -> Result<Json<ResolveFeedbackResponse>, HttpError> {
    if !brainiac_store::feedback::RESOLUTIONS.contains(&body.resolution.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "unknown resolution `{}` ({})",
                body.resolution,
                brainiac_store::feedback::RESOLUTIONS.join("|")
            ),
        )
            .into());
    }
    let principal = auth_of(&state, &headers, "write").await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    // FOR UPDATE is the serialization point for this whole endpoint: every
    // resolution path locks the same memory row first, so two maintainers
    // answering the same dispute queue up instead of interleaving. The second
    // one re-reads the row (and the claim count) as the first one left it and
    // takes the 409 below, rather than applying a second decision on top of a
    // corpus that already moved.
    //
    // Invisible memory ⇒ 404, not 403 (no oracle) — same stance as promotions.
    let row = sqlx::query(
        "SELECT team_id, kind, status::text AS status, superseded_by
         FROM memories WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "memory not found".into()))?;

    // A memory with no team is org-wide, and deprecating it is the most
    // destructive act on this endpoint — so a NULL team_id must mean a STRICTER
    // gate, not the absence of one. Skipping the check here (the old behaviour)
    // let any principal holding `write` permanently deprecate an org-level
    // memory. `is_any_maintainer` is the stance docs.rs:312-315 already takes for
    // org-wide pages.
    let allowed = match row.get::<Option<Uuid>, _>("team_id") {
        Some(team_id) => is_maintainer(&mut tx, &principal, team_id).await?,
        None => is_any_maintainer(&mut tx, &principal).await?,
    };
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "answering feedback claims requires a maintainer of the owning team \
             (org-wide memories: a maintainer of any team)"
                .into(),
        )
            .into());
    }

    // Nothing open ⇒ nothing to answer. Checked BEFORE any mutation and under
    // the row lock: `claims_closed: 0` used to render as a success while a
    // destructive resolution was applied to a dispute someone else had already
    // settled. A resolution with no claim behind it is a 409, not a 200.
    if brainiac_store::feedback::open_claim_count(&mut tx, id)
        .await
        .map_err(internal)?
        == 0
    {
        return Err((
            StatusCode::CONFLICT,
            "no open claims against this memory — it has already been answered".into(),
        )
            .into());
    }

    let status: String = row.get("status");
    let superseded = row.get::<Option<Uuid>, _>("superseded_by").is_some();

    let mut new_valid_to: Option<chrono::DateTime<chrono::Utc>> = None;
    match body.resolution.as_str() {
        "reverified" => {
            // "Still true, extend it" is incoherent against a row the org has
            // already retired: extend_validity guards only on `superseded_by`,
            // so without this a reverify would push valid_to a year out on a
            // deprecated memory and report 200. Refuse the state, don't record it.
            if status == "deprecated" || superseded {
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "cannot re-verify a {} memory — its window is closed; \
                         answer with `deprecated` or `dismissed`",
                        if superseded {
                            "superseded"
                        } else {
                            "deprecated"
                        }
                    ),
                )
                    .into());
            }
            let kind = brainiac_core::MemoryKind::parse(&row.get::<String, _>("kind"));
            let days = body
                .days
                .unwrap_or_else(|| kind.map_or(365, |k| i64::from(k.default_ttl_days())))
                .clamp(1, 3650);
            // Every outcome is now answered honestly. The old code assigned this
            // to `new_valid_to` and never looked: a None came back as
            // `200 {valid_to: null}` and the console rendered "validity window
            // extended" over a memory nothing had happened to.
            match brainiac_store::memories::extend_validity(&mut tx, id, days)
                .await
                .map_err(internal)?
            {
                brainiac_store::memories::ExtendOutcome::Extended(at) => new_valid_to = Some(at),
                brainiac_store::memories::ExtendOutcome::Superseded => {
                    return Err((
                        StatusCode::CONFLICT,
                        "memory was superseded concurrently — supersessions are final".into(),
                    )
                        .into())
                }
                brainiac_store::memories::ExtendOutcome::NotFound => {
                    return Err((StatusCode::NOT_FOUND, "memory not found".into()).into())
                }
            }
        }
        "deprecated" => {
            // The reporters were right: end the window now and drop it out of
            // retrieval, without inventing a supersessor it doesn't have.
            // Already-deprecated is not "success" — it means someone got here
            // first, and the claim count above says the dispute is still open,
            // so this is a genuinely conflicting state rather than a replay.
            if status == "deprecated" || superseded {
                return Err((StatusCode::CONFLICT, "memory is already deprecated".into()).into());
            }
            // Store-owned primitive, for the reason the contradiction path
            // already routes through apply_supersession: it deprecates the
            // memory, closes valid_to, recomposes the pages built on it, AND
            // records the transition in the promotions audit log. The inline SQL
            // this replaces did the first two and skipped the audit row — which
            // is why permanently retiring an org memory left no trace anywhere.
            let applied = brainiac_store::governance::apply_deprecation(
                &mut tx,
                principal.org_id,
                id,
                Some(principal.user_id),
                "feedback_deprecate",
            )
            .await
            .map_err(internal)?;
            // false under a held lock means the UPDATE policy refused a row the
            // SELECT policy showed us. Never commit the claim closure on top of a
            // deprecation that did not happen.
            if !applied {
                return Err((
                    StatusCode::CONFLICT,
                    "memory could not be deprecated under your scope".into(),
                )
                    .into());
            }
        }
        _ => {} // dismissed: the memory stands untouched
    }

    let closed = brainiac_store::feedback::resolve_claims(
        &mut tx,
        id,
        principal.user_id,
        &body.resolution,
        body.note.as_deref(),
    )
    .await
    .map_err(internal)?;
    // The count was non-zero under this same lock a few statements ago; if it is
    // zero now the lock did not hold and every guard above was decided on stale
    // state. Roll back rather than report a decision we cannot stand behind.
    if closed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "claims were closed concurrently — nothing was applied".into(),
        )
            .into());
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(ResolveFeedbackResponse {
        memory_id: id,
        resolution: body.resolution,
        claims_closed: closed,
        valid_to: new_valid_to,
    }))
}

/// Is this principal a maintainer of ANY team in their org? The gate for
/// org-wide (`team_id IS NULL`) resources, which have no owning team to check
/// against — mirrors the stance docs.rs takes for org-wide pages.
pub(crate) async fn is_any_maintainer(
    conn: &mut PgConnection,
    principal: &Principal,
) -> Result<bool, HttpError> {
    let row = sqlx::query(
        "SELECT 1 AS ok FROM team_members WHERE user_id = $1 AND role = 'maintainer' LIMIT 1",
    )
    .bind(principal.user_id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?;
    Ok(row.is_some())
}

// ── freshness lifecycle (TTL + re-verification) ─────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct ExpiringQuery {
    /// Window in days (default 30; 0 = only already-expired).
    days: Option<i64>,
    limit: Option<i64>,
}

/// A memory in the re-verification horizon. `days_left` is computed per row
/// against `now()` and is negative once the boundary has passed.
#[derive(Serialize, ToSchema)]
pub(crate) struct ExpiringMemory {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub content: String,
    pub team_id: Option<Uuid>,
    pub confidence: Option<f32>,
    pub valid_to: Option<DateTime<Utc>>,
    pub days_left: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ExpiringResponse {
    pub window_days: i64,
    pub memories: Vec<ExpiringMemory>,
}

/// The re-verification queue: live candidate/canonical memories whose
/// validity window closes within the horizon, oldest boundary first.
#[utoipa::path(
    get,
    path = "/v1/memories/expiring",
    tag = "memories",
    description = "The re-verification queue: live candidate/canonical memories whose validity window closes within the horizon, oldest boundary first.",
    params(
        ("days" = Option<i64>, Query, description = "Horizon in days (default 30, clamped 0..3650; 0 = only already-expired)"),
        ("limit" = Option<i64>, Query, description = "Max rows (default 50)"),
    ),
    responses((status = 200, description = "Expiring memories", body = ExpiringResponse))
)]
pub(crate) async fn memories_expiring(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExpiringQuery>,
    headers: HeaderMap,
) -> Result<Json<ExpiringResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let days = q.days.unwrap_or(30).clamp(0, 3650);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = brainiac_store::memories::expiring(&mut tx, days, q.limit.unwrap_or(50))
        .await
        .map_err(internal)?;
    let now = chrono::Utc::now();
    Ok(Json(ExpiringResponse {
        window_days: days,
        memories: rows
            .iter()
            .map(|m| ExpiringMemory {
                id: m.id,
                kind: m.kind.as_str().to_string(),
                status: m.status.as_str().to_string(),
                content: m.content.clone(),
                team_id: m.team_id,
                confidence: m.confidence,
                valid_to: m.valid_to,
                days_left: m.valid_to.map(|to| (to - now).num_days()),
            })
            .collect(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ReverifyBody {
    /// New validity budget from now; defaults to the kind's standard TTL.
    days: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ReverifyResponse {
    pub memory_id: Uuid,
    pub reverified: bool,
    /// The new validity boundary (never null on success).
    pub valid_to: DateTime<Utc>,
    /// The budget actually applied, after clamping.
    pub days: i64,
    pub claims_closed: u64,
}

/// Re-verification is a governance act: like promotion review, it requires
/// a maintainer of the owning team (org-wide memories: any org principal
/// with write scope).
#[utoipa::path(
    post,
    path = "/v1/memories/{id}/reverify",
    tag = "memories",
    description = "Re-verify a memory: extend its validity window from now and close any open feedback claims against it. Requires a maintainer of the owning team.",
    params(("id" = Uuid, Path, description = "Memory id")),
    request_body(content = ReverifyBody, description = "Optional validity budget; body may be omitted entirely"),
    responses(
        (status = 200, description = "Memory re-verified", body = ReverifyResponse),
        (status = 403, description = "Caller is not a maintainer of the owning team"),
        (status = 404, description = "Memory not found, or superseded (supersessions are final)"),
        (status = 409, description = "Memory was superseded concurrently"),
    )
)]
pub(crate) async fn memory_reverify(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Option<Json<ReverifyBody>>,
) -> Result<Json<ReverifyResponse>, HttpError> {
    let principal = auth_of(&state, &headers, "write").await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let row =
        sqlx::query("SELECT team_id, kind FROM memories WHERE id = $1 AND superseded_by IS NULL")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?
            .ok_or((
                StatusCode::NOT_FOUND,
                "memory not found (or superseded — supersessions are final)".into(),
            ))?;
    if let Some(team_id) = row.get::<Option<Uuid>, _>("team_id") {
        if !is_maintainer(&mut tx, &principal, team_id).await? {
            return Err((
                StatusCode::FORBIDDEN,
                "re-verification requires a maintainer of the owning team".into(),
            )
                .into());
        }
    }
    let kind = brainiac_core::MemoryKind::parse(&row.get::<String, _>("kind"));
    let default_days = kind.map_or(365, |k| i64::from(k.default_ttl_days()));
    let days = body
        .and_then(|b| b.days)
        .unwrap_or(default_days)
        .clamp(1, 3650);
    let new_valid_to = match brainiac_store::memories::extend_validity(&mut tx, id, days)
        .await
        .map_err(internal)?
    {
        brainiac_store::memories::ExtendOutcome::Extended(at) => at,
        // The lead SELECT already filtered `superseded_by IS NULL`, so this is a
        // concurrent supersession rather than the 404 the old bare `None` gave.
        brainiac_store::memories::ExtendOutcome::Superseded => {
            return Err((
                StatusCode::CONFLICT,
                "memory was superseded concurrently — supersessions are final".into(),
            )
                .into())
        }
        brainiac_store::memories::ExtendOutcome::NotFound => {
            return Err((StatusCode::NOT_FOUND, "memory not found".into()).into())
        }
    };
    // Re-verifying answers any open feedback claims against this memory —
    // a maintainer who just confirmed it is true has, in fact, responded.
    let claims_closed = brainiac_store::feedback::resolve_claims(
        &mut tx,
        id,
        principal.user_id,
        "reverified",
        None,
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(ReverifyResponse {
        memory_id: id,
        reverified: true,
        valid_to: new_valid_to,
        days,
        claims_closed,
    }))
}

// ── audit trail ─────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct AuditQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    /// Narrow to one kind of governance action. Applied in SQL, so `total`
    /// describes the FILTERED feed — a filter whose total still counted the
    /// unfiltered set would misreport the backlog it is meant to explain.
    kind: Option<String>,
}

/// The governance actions `/v1/audit` unions, and the accepted `kind` filter
/// values. Kept beside the SQL that produces them so the two cannot drift.
pub(crate) const AUDIT_KINDS: [&str; 3] = [
    "promotion_review",
    "contradiction_resolution",
    "feedback_resolution",
];

/// One governance action. `kind` is `promotion_review` |
/// `contradiction_resolution` | `feedback_resolution`; `memory_b` is only set
/// for contradictions. `detail` carries the decision's rationale where the path
/// captures one (the resolution note) and the policy rule otherwise.
#[derive(Serialize, ToSchema)]
pub(crate) struct AuditEvent {
    pub kind: String,
    pub id: Uuid,
    pub memory_id: Uuid,
    pub memory_b: Option<Uuid>,
    pub outcome: String,
    pub detail: Option<String>,
    /// Null for policy (auto) decisions — no human actor.
    pub actor_id: Option<Uuid>,
    pub at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AuditResponse {
    /// Total governance actions visible to the caller — the full feed length,
    /// independent of the page window.
    pub total: i64,
    pub events: Vec<AuditEvent>,
}

/// Reverse-chronological feed of governance actions: promotion reviews
/// (human and policy), contradiction resolutions, and dispute resolutions.
/// Reuses the reviewer / resolved-by columns the tables already carry; rows
/// resolve under the caller's RLS transaction so members see their org slice
/// only.
///
/// A deprecation reached through a dispute appears twice, deliberately: once as
/// the `feedback_resolution` decision and once as the `promotion_review` status
/// transition it applied. That is the same shape a contradiction supersession
/// already has — the decision and the transition are separate facts, and an
/// audit trail that collapsed them could not show a decision whose transition
/// never landed.
#[utoipa::path(
    get,
    path = "/v1/audit",
    tag = "reviews",
    description = "Reverse-chronological feed of governance actions: promotion reviews (human and policy), contradiction resolutions, and dispute resolutions.",
    params(
        ("limit" = Option<i64>, Query, description = "Page size (default 50, clamped 1..200)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
        ("kind" = Option<String>, Query, description = "Narrow to one action kind: promotion_review | contradiction_resolution | feedback_resolution"),
    ),
    responses(
        (status = 200, description = "Audit feed page", body = AuditResponse),
        (status = 400, description = "Unknown kind filter"),
    )
)]
pub(crate) async fn audit(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
    headers: HeaderMap,
) -> Result<Json<AuditResponse>, HttpError> {
    if let Some(kind) = q.kind.as_deref() {
        if !AUDIT_KINDS.contains(&kind) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown kind `{kind}` ({})", AUDIT_KINDS.join("|")),
            )
                .into());
        }
    }
    let principal = principal_of(&state, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    // The governance feed is a UNION of reviewed promotions, resolved
    // contradictions and answered disputes; keep the source SQL in one place so
    // the page and its total can never describe different sets.
    const AUDIT_FROM: &str = "FROM (
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
            UNION ALL
            -- Answering a dispute is a governance decision — it can permanently
            -- deprecate an org memory — and it was missing from this union
            -- entirely, so the feed could not answer 'who retired this?'.
            --
            -- One event per DECISION, not per claim: a resolve call closes every
            -- open claim on the memory in one transaction, sharing a single
            -- resolved_at/resolution/resolved_by. Grouping on those folds the N
            -- rows back into the one act a maintainer actually performed,
            -- instead of spamming the auditor with N identical entries.
            SELECT 'feedback_resolution' AS kind,
                   (array_agg(f.id ORDER BY f.id))[1] AS id,
                   f.memory_id,
                   NULL::uuid AS memory_b,
                   f.resolution AS outcome,
                   min(f.resolution_note) AS detail,
                   f.resolved_by AS actor_id,
                   f.resolved_at AS at
            FROM memory_feedback f
            JOIN memories m ON m.id = f.memory_id
            WHERE f.resolved_at IS NOT NULL
            GROUP BY f.memory_id, f.resolution, f.resolved_by, f.resolved_at
         ) audit";
    // The kind filter rides on BOTH queries — a NULL bind means "no filter", so
    // there is exactly one predicate and `total` always counts the same set the
    // page is drawn from. `total` is the whole (filtered) feed, never the page
    // length: a client showing `events.length` as the backlog would report "50"
    // forever.
    const AUDIT_WHERE: &str = "WHERE ($3::text IS NULL OR kind = $3)";
    let rows = sqlx::query(&format!(
        "SELECT * {AUDIT_FROM} {AUDIT_WHERE} ORDER BY at DESC LIMIT $1 OFFSET $2"
    ))
    .bind(limit)
    .bind(offset)
    .bind(q.kind.as_deref())
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    // $1/$2 are unused here but kept in the bind order so the two queries share
    // one predicate string.
    let total: i64 = sqlx::query(&format!("SELECT count(*) AS n {AUDIT_FROM} {AUDIT_WHERE}"))
        .bind(limit)
        .bind(offset)
        .bind(q.kind.as_deref())
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");
    let out: Vec<AuditEvent> = rows
        .iter()
        .map(|r| AuditEvent {
            kind: r.get("kind"),
            id: r.get("id"),
            memory_id: r.get("memory_id"),
            memory_b: r.get("memory_b"),
            outcome: r.get("outcome"),
            detail: r.get("detail"),
            actor_id: r.get("actor_id"),
            at: r.get("at"),
        })
        .collect();
    Ok(Json(AuditResponse { total, events: out }))
}

// ── graph ───────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub(crate) struct GraphCanonical {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GraphEntity {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub team_id: Uuid,
    /// Null when the entity has not been linked to a canonical hub yet.
    pub canonical_id: Option<Uuid>,
}

/// An evidence edge. `evidence` is null when the backing memory is hidden by
/// RLS — the edge itself stays visible (it's org metadata).
#[derive(Serialize, ToSchema)]
pub(crate) struct GraphEdge {
    pub src: Uuid,
    pub dst: Uuid,
    pub relation: String,
    pub memory_id: Option<Uuid>,
    pub evidence: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GraphResponse {
    pub canonicals: Vec<GraphCanonical>,
    pub entities: Vec<GraphEntity>,
    pub edges: Vec<GraphEdge>,
}

#[utoipa::path(
    get,
    path = "/v1/graph",
    tag = "graph",
    description = "The entity graph: canonical hubs, team-scoped surface entities, and evidence edges (bounded at 2000/2000/5000 rows).",
    responses((status = 200, description = "Entity graph", body = GraphResponse))
)]
pub(crate) async fn graph(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GraphResponse>, HttpError> {
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

    Ok(Json(GraphResponse {
        canonicals: canonicals
            .iter()
            .map(|r| GraphCanonical {
                id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
            })
            .collect(),
        entities: entities
            .iter()
            .map(|r| GraphEntity {
                id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
                team_id: r.get("team_id"),
                canonical_id: r.get("canonical_id"),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|r| GraphEdge {
                src: r.get("src_entity"),
                dst: r.get("dst_entity"),
                relation: r.get("relation"),
                memory_id: r.get("memory_id"),
                evidence: r.get("evidence"),
            })
            .collect(),
    }))
}

// ── analytics ───────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub(crate) struct AnalyticsReviews {
    pub pending_promotions: i64,
    pub oldest_pending_secs: i64,
    pub open_contradictions: i64,
    pub flagged_memories: i64,
    /// Review VELOCITY — the abandonment signal. A queue with a growing backlog
    /// and near-zero throughput is a review step nobody is working; a healthy one
    /// clears roughly what it takes in. Made observable so the failure the whole
    /// governance model rides on stops being silent (UAT relay
    /// `promotion-queue-backlog`). Human decisions only (auto-approvals excluded).
    pub reviewed_last_7d: i64,
    pub reviewed_last_30d: i64,
    /// Median seconds from a memory entering the queue to a human deciding it,
    /// over the last 30 days. `null` if nothing was reviewed. Against their own
    /// 48h SLO (ARCHITECTURE §7) this is the review-latency truth.
    pub median_time_to_review_secs: Option<i64>,
    /// Share of last-30d human reviews decided in under 5s — the rubber-stamp
    /// proxy. High + a deep backlog = clearing, not reviewing. `null` if none.
    pub rubber_stamp_rate: Option<f64>,
    /// Share of RESOLVED contradictions that were dismissed as "not a conflict",
    /// over the last 30 days. Since an unresolved contradiction now WITHHOLDS both
    /// sides from serving (the open-contradiction fix), a high dismiss rate means
    /// an over-eager detector is *suppressing real knowledge*, not just adding
    /// noise — a signal to retune it. `null` if nothing was resolved.
    pub contradiction_dismiss_rate: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AnalyticsGraph {
    pub entities: i64,
    pub canonicals: i64,
}

/// Ingest queue depth — shared by `/v1/analytics` and the observatory.
#[derive(Serialize, ToSchema)]
pub(crate) struct QueueDepth {
    pub ingest_depth: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AnalyticsResponse {
    pub memories_by_status: Vec<StatusCount>,
    pub reviews: AnalyticsReviews,
    pub graph: AnalyticsGraph,
    pub queue: QueueDepth,
    pub embedding_model: String,
}

#[utoipa::path(
    get,
    path = "/v1/analytics",
    tag = "analytics",
    description = "Governance health counters under the caller's RLS view: memories by status, review backlog, graph size, ingest queue depth.",
    responses((status = 200, description = "Governance counters", body = AnalyticsResponse))
)]
pub(crate) async fn analytics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<AnalyticsResponse>, HttpError> {
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
    // Review velocity (the abandonment signal). Human decisions only — a
    // reviewer_id present means a person, not the auto-approve policy, decided.
    // time_to_review = queue latency (created → decided), the SLO number.
    // rubber-stamp proxy = share of decisions taken within 5s of the SAME
    // reviewer's previous decision (a burst = clearing backlog, not reading);
    // computed with lag() since no per-decision dwell time is captured.
    let velocity = sqlx::query(
        "WITH human AS (
             SELECT reviewer_id, created_at, reviewed_at,
                    EXTRACT(EPOCH FROM reviewed_at - created_at)::bigint AS ttr,
                    EXTRACT(EPOCH FROM reviewed_at - lag(reviewed_at)
                        OVER (PARTITION BY reviewer_id ORDER BY reviewed_at)) AS gap
             FROM promotions
             WHERE reviewer_id IS NOT NULL AND reviewed_at IS NOT NULL
               AND reviewed_at > now() - interval '30 days'
         )
         SELECT
           count(*) FILTER (WHERE reviewed_at > now() - interval '7 days')  AS r7,
           count(*)                                                          AS r30,
           percentile_cont(0.5) WITHIN GROUP (ORDER BY ttr)::bigint          AS median_ttr,
           avg(CASE WHEN gap IS NOT NULL AND gap < 5 THEN 1.0 ELSE 0.0 END)::float8 AS stamp_rate,
           count(*) FILTER (WHERE gap IS NOT NULL)                           AS with_gap
         FROM human",
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
    // Dismiss rate over recently-resolved contradictions — visibility for the
    // over-eager-detector footgun the withhold-by-default fix introduced.
    let dismiss = sqlx::query(
        "SELECT avg(CASE WHEN status = 'dismissed' THEN 1.0 ELSE 0.0 END)::float8 AS rate,
                count(*) AS resolved_n
         FROM contradictions
         WHERE status <> 'open' AND resolved_at > now() - interval '30 days'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let flagged_memories = brainiac_store::feedback::flagged_count(
        &mut tx,
        &brainiac_store::feedback::FlaggedFilter::default(),
    )
    .await
    .map_err(internal)?;
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

    Ok(Json(AnalyticsResponse {
        memories_by_status: by_status
            .iter()
            .map(|r| StatusCount {
                status: r.get("status"),
                count: r.get("n"),
            })
            .collect(),
        reviews: AnalyticsReviews {
            pending_promotions: review.get("pending"),
            oldest_pending_secs: review.get("oldest_secs"),
            open_contradictions: contradictions_open,
            flagged_memories,
            reviewed_last_7d: velocity.get::<Option<i64>, _>("r7").unwrap_or(0),
            reviewed_last_30d: velocity.get::<Option<i64>, _>("r30").unwrap_or(0),
            median_time_to_review_secs: velocity.get("median_ttr"),
            // Only meaningful once a reviewer has a run of decisions to compare.
            rubber_stamp_rate: if velocity.get::<Option<i64>, _>("with_gap").unwrap_or(0) > 0 {
                velocity.get::<Option<f64>, _>("stamp_rate")
            } else {
                None
            },
            contradiction_dismiss_rate: if dismiss.get::<i64, _>("resolved_n") > 0 {
                dismiss.get::<Option<f64>, _>("rate")
            } else {
                None
            },
        },
        graph: AnalyticsGraph {
            entities,
            canonicals,
        },
        queue: QueueDepth {
            ingest_depth: queue_depth,
        },
        embedding_model: state.embedder.model_name().to_string(),
    }))
}

// ── observatory (the dashboard module's richer payload) ─────────────────

/// One point of a weekly series; `week` is an ISO label (`IYYY-Www`) so the
/// captured/promoted series stay joinable client-side.
#[derive(Serialize, ToSchema)]
pub(crate) struct WeeklyPoint {
    pub week: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ObservatoryWeekly {
    pub captured: Vec<WeeklyPoint>,
    pub promoted: Vec<WeeklyPoint>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct KindTeamCount {
    pub kind: String,
    pub team: String,
    pub count: i64,
}

/// The project twin of [`KindTeamCount`] (PROJECT-PLAN PR3). `project` is the
/// display name, with `org-shared` standing in for the null bucket.
#[derive(Serialize, ToSchema)]
pub(crate) struct KindProjectCount {
    pub kind: String,
    pub project: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct TopEntity {
    pub name: String,
    pub kind: String,
    pub memories: i64,
    pub teams: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ObservatoryReview {
    pub pending: i64,
    pub oldest_pending_secs: i64,
    pub reviewed: i64,
    pub avg_latency_secs: i64,
    pub auto_promoted: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ObservatoryResponse {
    pub totals: Vec<StatusCount>,
    pub weekly: ObservatoryWeekly,
    pub by_kind: Vec<KindTeamCount>,
    /// Kind×project volumes — the axis-swap twin of `by_kind` (PR3).
    pub by_project: Vec<KindProjectCount>,
    pub top_entities: Vec<TopEntity>,
    pub review: ObservatoryReview,
    pub contradictions: Vec<StatusCount>,
    pub queue: QueueDepth,
    pub embedding_model: String,
}

#[utoipa::path(
    get,
    path = "/v1/analytics/observatory",
    tag = "analytics",
    description = "The dashboard payload: status totals, weekly captured/promoted flow, kind×team volumes, top canonical themes, review latency, contradiction mix, queue depth.",
    responses((status = 200, description = "Observatory dashboard payload", body = ObservatoryResponse))
)]
pub(crate) async fn observatory(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ObservatoryResponse>, HttpError> {
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

    // The project twin of by_kind. LEFT JOIN so org-shared rows form their
    // own labelled column rather than vanishing (PR3).
    let by_project = sqlx::query(
        "SELECT m.kind, coalesce(pj.name, 'org-shared') AS project, count(*) AS n
         FROM memories m LEFT JOIN projects pj ON pj.id = m.project_id
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

    let weekly_points = |rows: &[sqlx::postgres::PgRow]| -> Vec<WeeklyPoint> {
        rows.iter()
            .map(|r| WeeklyPoint {
                week: r.get("week"),
                count: r.get("n"),
            })
            .collect()
    };

    Ok(Json(ObservatoryResponse {
        totals: by_status
            .iter()
            .map(|r| StatusCount {
                status: r.get("status"),
                count: r.get("n"),
            })
            .collect(),
        weekly: ObservatoryWeekly {
            captured: weekly_points(&captured),
            promoted: weekly_points(&promoted),
        },
        by_kind: by_kind
            .iter()
            .map(|r| KindTeamCount {
                kind: r.get("kind"),
                team: r.get("team"),
                count: r.get("n"),
            })
            .collect(),
        by_project: by_project
            .iter()
            .map(|r| KindProjectCount {
                kind: r.get("kind"),
                project: r.get("project"),
                count: r.get("n"),
            })
            .collect(),
        top_entities: top_entities
            .iter()
            .map(|r| TopEntity {
                name: r.get("name"),
                kind: r.get("kind"),
                memories: r.get("memories"),
                teams: r.get("teams"),
            })
            .collect(),
        review: ObservatoryReview {
            pending: review.get("pending"),
            oldest_pending_secs: review.get("oldest_secs"),
            reviewed: review.get("reviewed"),
            avg_latency_secs: review.get("avg_latency_secs"),
            auto_promoted: review.get("auto_promoted"),
        },
        contradictions: contradictions
            .iter()
            .map(|r| StatusCount {
                status: r.get("status"),
                count: r.get("n"),
            })
            .collect(),
        queue: QueueDepth {
            ingest_depth: queue_depth,
        },
        embedding_model: state.embedder.model_name().to_string(),
    }))
}

// ── Knowledge Health (the leadership product surface) ───────────────────
//
// One call, one page a VP Eng gets weekly. Where `observatory` is an operator's
// dashboard of the pipeline, this answers the *organizational* question the whole
// architecture exists for: is the org's collective knowledge consistent, current,
// liquid, and governed — the four things no individual can see. It rolls the
// org-level signals into a single tracked score plus a ranked "what needs your
// attention" list, so the value is legible to a buyer who never opens the graph.

#[derive(Serialize, ToSchema)]
pub(crate) struct KhPillars {
    /// 100 − penalty for the org contradicting itself (cross-team conflicts hurt most).
    pub consistency: i64,
    /// Share of the corpus that is still current (not superseded/expired).
    pub currency: i64,
    /// How much knowledge crosses team lines — the "together-picture" density.
    pub liquidity: i64,
    /// Is the review queue actually being worked (backlog age vs the 48h SLO).
    pub governance: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct KhSignals {
    pub total_memories: i64,
    pub canonical_entities: i64,
    /// Canonical entities carrying knowledge from ≥2 teams — the graph doing its job.
    pub cross_team_entities: i64,
    pub open_contradictions: i64,
    /// Open contradictions where the two sides belong to DIFFERENT teams — the ones
    /// no individual team can see. The flagship signal.
    pub cross_team_contradictions: i64,
    /// Canonical entities anchored by memories of ≥2 DIFFERENT projects —
    /// knowledge crossing application lines (PROJECT-PLAN PR3). Zero until
    /// writes are project-stamped; that is coverage, not health.
    pub cross_project_entities: i64,
    /// Open contradictions whose two sides are stamped with DIFFERENT
    /// projects — two applications believing incompatible things (PR3).
    pub cross_project_contradictions: i64,
    /// Superseded/expired beliefs still sitting in the corpus (landmines).
    pub stale_beliefs: i64,
    // ── the knowledge base (KB4) ────────────────────────────────────────
    /// Pages whose memories moved and which have NOT recomposed yet. Every one
    /// is a page currently telling readers something the org no longer believes.
    pub pages_dirty: i64,
    /// How long the most overdue page has been out of date, in seconds. THE
    /// propagation SLA: the product's promise is that a resolved contradiction
    /// reaches every page automatically, and this is the number that says
    /// whether "automatically" means minutes or means never.
    pub oldest_dirty_secs: i64,
    /// Page revisions awaiting a human — the KB's own review backlog.
    pub pages_pending_review: i64,
    pub pages_published: i64,
    /// Page reads served in the last 30 days (0025) — consumption, the half of
    /// liquidity the visibility mix cannot see. Zero on a fresh deployment is
    /// normal; zero six months in means the wiki is decoration.
    pub page_reads_30d: i64,
    /// The subset of the last 30 days' reads that came through MCP — coding
    /// agents consuming pages, which is exactly the loop the KB exists for.
    pub agent_page_reads_30d: i64,
    /// Reads in the last 30 days that were served while the page was DIRTY —
    /// someone consumed a belief the org had already moved past. This is the
    /// number that ranks rot by harm rather than by age.
    pub dirty_page_reads_30d: i64,
    /// Published pages no one has ever read. Not an emergency — a candidate
    /// list: promote them where readers are, or stop composing them.
    pub pages_never_read: i64,
    // ── the library (LIBRARY-PLAN follow-up 2) ──────────────────────────
    /// Adopted rules — the org's live, ratified judgment.
    pub standards_adopted: i64,
    /// Candidates waiting at the gate (mined, ratified, or agent-proposed).
    pub standards_at_gate: i64,
    /// How long the oldest candidate has waited, in seconds. Mining and agents
    /// both file into this queue; a queue nobody works makes the whole intake
    /// theatre, and the number says which it is.
    pub oldest_gate_secs: i64,
    /// Adopted rules the org has had time to use and hasn't touched in a
    /// month. THE Library signal: a standard nobody follows is a wish, and
    /// this is the number that says so without anyone having to notice.
    pub standards_dormant: i64,
    pub skills_published: i64,
    /// Published skills nobody has fetched in a month.
    pub skills_dormant: i64,
    pub org_wide: i64,
    pub team_only: i64,
    pub siloed_private: i64,
    /// org_wide / total, as a percentage — knowledge liquidity.
    pub liquidity_pct: i64,
    pub review_backlog: i64,
    pub oldest_review_secs: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct KhAttention {
    /// critical | warning | info — encodes urgency in form, not just number.
    pub severity: String,
    /// contradiction | staleness | silo | governance | library
    pub kind: String,
    pub headline: String,
    pub detail: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct TrendPoint {
    pub captured_at: DateTime<Utc>,
    pub score: i64,
    pub consistency: i64,
    pub currency: i64,
    pub liquidity: i64,
    pub governance: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct KnowledgeHealthResponse {
    /// 0–100 composite the org tracks week over week.
    pub score: i64,
    /// Healthy | Watch | At risk | Critical.
    pub grade: String,
    pub pillars: KhPillars,
    pub signals: KhSignals,
    /// Ranked, most-urgent-first — the whole point: turn the score into action.
    pub attention: Vec<KhAttention>,
    /// Recorded snapshots oldest→newest — the score over time. The report's power
    /// is the line, not the point. Empty until the first snapshot is taken.
    pub trend: Vec<TrendPoint>,
    pub embedding_model: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SnapshotResponse {
    pub captured_at: DateTime<Utc>,
    pub score: i64,
    pub grade: String,
}

/// The org's own promotion-review SLO (ARCHITECTURE §7): median review under
/// 48h or the governance flywheel dies. The governance pillar and the
/// attention-list breach both key off it — and so does the KB3 publish breaker,
/// which is why the constant (like the pillar math) lives in core.
use brainiac_core::health::{LIBRARY_DORMANT_DAYS, LIBRARY_GATE_SLO_SECS, REVIEW_SLO_SECS};

fn grade_of(score: i64) -> &'static str {
    match score {
        85..=100 => "Healthy",
        70..=84 => "Watch",
        55..=69 => "At risk",
        _ => "Critical",
    }
}

/// The numeric core of a Knowledge Health reading — every score + the signals
/// behind it, RLS-scoped to the caller's org. Shared by the live report (GET) and
/// the trend snapshot writer (POST) so the number a leader watches and the number
/// recorded to history are computed one way, never two.
// pub(crate): the alert sweep (crate::alerts) evaluates the same org-true
// numbers this report renders — one computation, two consumers, no drift.
pub(crate) struct HealthCore {
    pub(crate) total: i64,
    pub(crate) stale: i64,
    pub(crate) org_wide: i64,
    pub(crate) team_only: i64,
    pub(crate) siloed: i64,
    pub(crate) open_contra: i64,
    pub(crate) cross_contra: i64,
    /// PROJECT-PLAN PR3: the project axis of the two flagship counters.
    pub(crate) cross_project_contra: i64,
    pub(crate) canon: i64,
    pub(crate) cross_entities: i64,
    pub(crate) cross_project_entities: i64,
    pub(crate) backlog: i64,
    pub(crate) oldest: i64,
    pub(crate) consistency: i64,
    pub(crate) currency: i64,
    pub(crate) liquidity: i64,
    pub(crate) governance: i64,
    pub(crate) score: i64,
}

/// Compute the health pillars + signals.
///
/// `org_filter` is `None` for the live endpoints — they run under a scoped_tx,
/// so RLS already restricts every table to the caller's org (and their visible
/// slice of it). It is `Some(org)` for the scheduled cross-org sweep, which runs
/// on the RLS-bypassing admin pool: the explicit filter is then the only thing
/// scoping each query to one org, and it sees that org's TRUE totals (every
/// visibility), independent of any viewer's vantage. The `$1 IS NULL OR …`
/// shape lets one query serve both callers.
pub(crate) async fn compute_health_core(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_filter: Option<Uuid>,
) -> Result<HealthCore, HttpError> {
    // Corpus size + currency (deprecated or past valid_to = stale).
    let corpus = sqlx::query(
        "SELECT
           count(*) AS total,
           count(*) FILTER (WHERE status = 'deprecated'
                              OR (valid_to IS NOT NULL AND valid_to < now())) AS stale,
           count(*) FILTER (WHERE visibility = 'org')     AS org_wide,
           count(*) FILTER (WHERE visibility = 'team')    AS team_only,
           count(*) FILTER (WHERE visibility = 'private') AS private_siloed
         FROM memories WHERE status <> 'rejected'
           AND ($1::uuid IS NULL OR org_id = $1)",
    )
    .bind(org_filter)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    let total: i64 = corpus.get("total");
    let stale: i64 = corpus.get("stale");
    let org_wide: i64 = corpus.get("org_wide");
    let team_only: i64 = corpus.get("team_only");
    let siloed: i64 = corpus.get("private_siloed");

    // Contradictions — total open, and the cross-team subset (the flagship).
    let contra = sqlx::query(
        "SELECT
           count(*) AS open,
           count(*) FILTER (WHERE ma.team_id IS DISTINCT FROM mb.team_id) AS cross_team,
           count(*) FILTER (WHERE ma.project_id IS NOT NULL AND mb.project_id IS NOT NULL
                              AND ma.project_id IS DISTINCT FROM mb.project_id) AS cross_project
         FROM contradictions c
         JOIN memories ma ON ma.id = c.memory_a
         JOIN memories mb ON mb.id = c.memory_b
         WHERE c.status = 'open'
           AND ($1::uuid IS NULL OR ma.org_id = $1)",
    )
    .bind(org_filter)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    let open_contra: i64 = contra.get("open");
    let cross_contra: i64 = contra.get("cross_team");
    let cross_project_contra: i64 = contra.get("cross_project");

    // Graph coverage: canonical entities, how many span ≥2 teams, and how many
    // span ≥2 PROJECTS (via anchored memories' stamps — entities are
    // team-scoped, so project spread only exists on the memory side; PR3).
    let graph = sqlx::query(
        "WITH spans AS (
           SELECT ce.id, count(DISTINCT e.team_id) AS teams
           FROM canonical_entities ce
           JOIN entity_links el ON el.canonical_id = ce.id
           JOIN entities e ON e.id = el.entity_id
           WHERE ($1::uuid IS NULL OR ce.org_id = $1)
           GROUP BY ce.id),
         pspans AS (
           SELECT ce.id, count(DISTINCT m.project_id) AS projects
           FROM canonical_entities ce
           JOIN entity_links el ON el.canonical_id = ce.id
           JOIN entities e ON e.id = el.entity_id
           JOIN memory_entities me ON me.entity_id = e.id
           JOIN memories m ON m.id = me.memory_id
           WHERE m.project_id IS NOT NULL
             AND ($1::uuid IS NULL OR ce.org_id = $1)
           GROUP BY ce.id)
         SELECT (SELECT count(*) FROM spans) AS canon,
                (SELECT count(*) FILTER (WHERE teams >= 2) FROM spans) AS cross_team,
                (SELECT count(*) FILTER (WHERE projects >= 2) FROM pspans) AS cross_project",
    )
    .bind(org_filter)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    let canon: i64 = graph.get("canon");
    let cross_entities: i64 = graph.get("cross_team");
    let cross_project_entities: i64 = graph.get("cross_project");

    // Governance: review backlog + oldest pending age (the SLO clock).
    let gov = sqlx::query(
        "SELECT count(*) AS pending,
                COALESCE(EXTRACT(EPOCH FROM now() - min(created_at)), 0)::bigint AS oldest
         FROM promotions WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL
           AND ($1::uuid IS NULL OR org_id = $1)",
    )
    .bind(org_filter)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    let backlog: i64 = gov.get("pending");
    let oldest: i64 = gov.get("oldest");

    // ── pillar scores ──────────────────────────────────────────────────
    // The formulas live in brainiac_core::health, NOT here, because the KB3
    // publish circuit breaker gates on the same two pillars. A breaker that
    // computed "currency" differently from the dashboard it is named after would
    // be indefensible — so there is exactly one implementation, and this report
    // is one of its two callers.
    use brainiac_core::health as hp;
    let consistency = hp::consistency_pillar(open_contra, cross_contra);
    let currency = hp::currency_pillar(total, stale);
    let liquidity = hp::liquidity_pillar(canon, cross_entities);
    let governance = hp::governance_pillar(backlog, oldest);
    let score = hp::composite_score(consistency, currency, liquidity, governance, cross_contra);

    Ok(HealthCore {
        total,
        stale,
        org_wide,
        team_only,
        siloed,
        open_contra,
        cross_contra,
        cross_project_contra,
        canon,
        cross_entities,
        cross_project_entities,
        backlog,
        oldest,
        consistency,
        currency,
        liquidity,
        governance,
        score,
    })
}

#[utoipa::path(
    get,
    path = "/v1/analytics/knowledge-health",
    tag = "analytics",
    description = "The leadership Knowledge Health report: a tracked composite score over four pillars (consistency, currency, liquidity, governance), the org-level signals behind it, and a ranked attention list. RLS-scoped — a leader sees their org's view.",
    responses((status = 200, description = "Knowledge Health report", body = KnowledgeHealthResponse))
)]
pub(crate) async fn knowledge_health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<KnowledgeHealthResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    // Org-TRUE pillar signals: the composite score is a leadership metric, so it
    // must be the org's real totals, not the caller's team-limited RLS slice (an
    // INNER JOIN on `memories` there silently drops a cross-team contradiction
    // whose other side the member can't read, inflating the consistency pillar).
    // Compute on the RLS-bypassing admin pool scoped to this org. The detail lists
    // below stay on the viewer's `tx`, so only the aggregate NUMBERS become
    // org-true — no team-private claim CONTENT leaks.
    let HealthCore {
        total,
        stale,
        org_wide,
        team_only,
        siloed,
        open_contra,
        cross_contra,
        cross_project_contra,
        canon,
        cross_entities,
        cross_project_entities,
        backlog,
        oldest,
        consistency,
        currency,
        liquidity,
        governance,
        score,
    } = {
        let mut atx = state.admin_pool.begin().await.map_err(internal)?;
        compute_health_core(&mut atx, Some(principal.org_id)).await?
    };
    let grade = grade_of(score).to_string();

    // ── attention list (ranked; the score made actionable) ──────────────
    let mut attention: Vec<KhAttention> = Vec::new();

    // Every open cross-team contradiction, with the actual competing claims.
    let cross_rows = sqlx::query(
        "SELECT ta.name AS team_a, ma.content AS claim_a,
                tb.name AS team_b, mb.content AS claim_b
         FROM contradictions c
         JOIN memories ma ON ma.id = c.memory_a JOIN teams ta ON ta.id = ma.team_id
         JOIN memories mb ON mb.id = c.memory_b JOIN teams tb ON tb.id = mb.team_id
         WHERE c.status = 'open' AND ma.team_id IS DISTINCT FROM mb.team_id
         ORDER BY c.created_at LIMIT 5",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    for r in &cross_rows {
        let (ta, tb): (String, String) = (r.get("team_a"), r.get("team_b"));
        let (ca, cb): (String, String) = (r.get("claim_a"), r.get("claim_b"));
        attention.push(KhAttention {
            severity: "critical".into(),
            kind: "contradiction".into(),
            headline: format!("{ta} and {tb} disagree — and neither can see it"),
            detail: format!(
                "{ta}: \"{}\"  vs  {tb}: \"{}\"",
                clip(&ca, 90),
                clip(&cb, 90)
            ),
        });
    }

    // Stale org-visible beliefs still being served (the widest-blast landmines).
    let stale_rows = sqlx::query(
        "SELECT t.name AS team, m.content, m.valid_to::date AS expired
         FROM memories m JOIN teams t ON t.id = m.team_id
         WHERE m.status <> 'rejected' AND m.visibility = 'org'
           AND (m.status = 'deprecated' OR (m.valid_to IS NOT NULL AND m.valid_to < now()))
         ORDER BY m.valid_to LIMIT 5",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    for r in &stale_rows {
        let team: String = r.get("team");
        let content: String = r.get("content");
        let expired: Option<chrono::NaiveDate> = r.get("expired");
        attention.push(KhAttention {
            severity: "warning".into(),
            kind: "staleness".into(),
            headline: format!("Org-wide belief expired but still served ({team})"),
            detail: format!(
                "\"{}\"{}",
                clip(&content, 100),
                expired
                    .map(|d| format!(" — superseded {d}"))
                    .unwrap_or_default()
            ),
        });
    }

    // Governance: SLO breach on the review queue.
    if oldest > REVIEW_SLO_SECS {
        attention.push(KhAttention {
            severity: "warning".into(),
            kind: "governance".into(),
            headline: "Review queue is past the 48h SLO".into(),
            detail: format!(
                "{backlog} item(s) pending; oldest waiting {} days. Unreviewed knowledge is served as if governed.",
                oldest / 86400
            ),
        });
    }

    let liquidity_pct = if total > 0 {
        (org_wide as f64 / total as f64 * 100.0).round() as i64
    } else {
        0
    };

    // ── the knowledge base (KB4) ────────────────────────────────────────
    // The KB's own health, in the leadership report rather than a page nobody
    // visits. `oldest_dirty_secs` is the propagation SLA made visible: the
    // product promises that a resolved contradiction reaches every page by
    // itself, and a page that has been dirty for three days is that promise
    // quietly failing. It belongs where a leader will see it go red.
    let kb = sqlx::query(
        "SELECT
           count(*) FILTER (WHERE dirty_at IS NOT NULL)             AS dirty,
           count(*) FILTER (WHERE status = 'published')             AS published,
           COALESCE(EXTRACT(EPOCH FROM now() - min(dirty_at)), 0)::bigint AS oldest_dirty
         FROM documents",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let pages_dirty: i64 = kb.get("dirty");
    let pages_published: i64 = kb.get("published");
    let oldest_dirty_secs: i64 = kb.get("oldest_dirty");

    let kb_review = sqlx::query(
        "SELECT count(*) AS pending FROM document_revisions
         WHERE policy_decision = 'needs_review' AND reviewed_by IS NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let pages_pending_review: i64 = kb_review.get("pending");

    // Consumption (0025). RLS scopes both queries to pages this viewer can
    // see, like every other number in the report. The pillar math deliberately
    // does NOT consume these yet — measure first, calibrate the lever after
    // there is data to calibrate against (the same posture as confidence).
    let reads = sqlx::query(
        "SELECT
           count(*) FILTER (WHERE read_at > now() - interval '30 days') AS reads_30d,
           count(*) FILTER (WHERE via = 'mcp'
                              AND read_at > now() - interval '30 days') AS agent_reads_30d,
           count(*) FILTER (WHERE was_dirty
                              AND read_at > now() - interval '30 days') AS dirty_reads_30d
         FROM document_reads",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let page_reads_30d: i64 = reads.get("reads_30d");
    let agent_page_reads_30d: i64 = reads.get("agent_reads_30d");
    let dirty_page_reads_30d: i64 = reads.get("dirty_reads_30d");
    let pages_never_read: i64 = sqlx::query(
        "SELECT count(*) AS never_read FROM documents d
         WHERE d.status = 'published'
           AND NOT EXISTS (SELECT 1 FROM document_reads r WHERE r.document_id = d.id)",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?
    .get("never_read");

    // One hour is generous for a loop that runs every tick; past it, propagation
    // is not "eventual", it is broken, and the pages are lying in the meantime.
    const PROPAGATION_SLA_SECS: i64 = 3600;
    if pages_dirty > 0 && oldest_dirty_secs > PROPAGATION_SLA_SECS {
        attention.push(KhAttention {
            severity: if oldest_dirty_secs > 24 * 3600 {
                "critical"
            } else {
                "warning"
            }
            .into(),
            kind: "staleness".into(),
            headline: format!(
                "{pages_dirty} knowledge-base page(s) are serving beliefs the org has moved past"
            ),
            detail: format!(
                "The most overdue has been waiting {} to recompose. Pages are supposed to \
                 self-heal within minutes of a memory changing — if this number keeps growing, \
                 the compose worker is not running and the wiki is rotting like any other.",
                human_age(oldest_dirty_secs)
            ),
        });
    }
    if pages_pending_review > 0 {
        attention.push(KhAttention {
            severity: "info".into(),
            kind: "governance".into(),
            headline: format!("{pages_pending_review} page revision(s) waiting on a human"),
            detail: "A page revision publishes only when a maintainer signs it. Until then \
                     readers see the previous version."
                .into(),
        });
    }
    // Rot that is being CONSUMED outranks rot that merely exists: a dirty page
    // nobody opens is a chore, a dirty page being read is misleading someone
    // right now.
    if dirty_page_reads_30d > 0 {
        attention.push(KhAttention {
            severity: "warning".into(),
            kind: "staleness".into(),
            headline: format!(
                "{dirty_page_reads_30d} page read(s) this month served out-of-date content"
            ),
            detail: "Someone opened a page after its underlying memories had moved on but \
                     before it recomposed. If propagation is healthy this window is minutes \
                     wide; if this number keeps growing, readers are routinely acting on \
                     beliefs the org has already corrected."
                .into(),
        });
    }
    if pages_never_read > 0 && pages_published > 0 {
        attention.push(KhAttention {
            severity: "info".into(),
            kind: "silo".into(),
            headline: format!("{pages_never_read} published page(s) have never been read"),
            detail: "Compiled, reviewed, and consumed by no one. Not an emergency — a \
                     candidate list: link them where readers already are, or stop spending \
                     review effort keeping them current."
                .into(),
        });
    }

    // ── the library (LIBRARY-PLAN follow-up 2) ──────────────────────────
    // The normative layer cannot recompose its way to honesty: the only test
    // of a rule is whether practice follows it. These items are that test,
    // reported where a leader will see it rather than on a board someone has
    // to remember to open. The pillar math deliberately does not consume them
    // (see brainiac_core::health) — going red is the promise, not a weight.
    let lib = brainiac_store::library::health_signals(&mut tx, LIBRARY_DORMANT_DAYS)
        .await
        .map_err(internal)?;

    if lib.standards_dormant > 0 {
        attention.push(KhAttention {
            severity: "warning".into(),
            kind: "library".into(),
            headline: format!(
                "{} adopted rule(s) nobody has followed in {LIBRARY_DORMANT_DAYS} days",
                lib.standards_dormant
            ),
            detail: "Each of these was ratified by a named human and has since gone untouched \
                     by every agent and every check. A standard nobody follows is a wish that \
                     costs credibility: retire it in the open, or find out why the org quietly \
                     stopped agreeing with it."
                .into(),
        });
    }
    if lib.standards_at_gate > 0 && lib.oldest_gate_secs > LIBRARY_GATE_SLO_SECS {
        attention.push(KhAttention {
            severity: "warning".into(),
            kind: "library".into(),
            headline: format!(
                "{} rule candidate(s) waiting at the gate — the oldest for {}",
                lib.standards_at_gate,
                human_age(lib.oldest_gate_secs)
            ),
            detail: "Sweeps and agents propose; only a human adopts. A queue nobody works \
                     turns the whole intake into theatre — and the proposers keep filing. \
                     Adopt them, or reject them: a rejection is remembered and stops the \
                     signal coming back."
                .into(),
        });
    } else if lib.standards_at_gate > 0 {
        attention.push(KhAttention {
            severity: "info".into(),
            kind: "library".into(),
            headline: format!(
                "{} rule candidate(s) waiting at the gate",
                lib.standards_at_gate
            ),
            detail: "Proposed by the mining sweep, a maintainer, or an agent mid-session. \
                     Nothing reaches an agent as policy until one of them is adopted."
                .into(),
        });
    }
    if lib.skills_dormant > 0 {
        attention.push(KhAttention {
            severity: "info".into(),
            kind: "library".into(),
            headline: format!(
                "{} published skill(s) unused for {LIBRARY_DORMANT_DAYS} days",
                lib.skills_dormant
            ),
            detail: "Published, and pulled by nobody. Either the agents do not know it exists \
                     or it does not help — both are worth knowing before someone spends \
                     another release maintaining it."
                .into(),
        });
    }

    // The trend: recorded snapshots, oldest→newest, RLS-scoped to this org.
    let trend = sqlx::query(
        "SELECT captured_at, score, consistency, currency, liquidity, governance
         FROM knowledge_health_snapshots
         ORDER BY captured_at ASC LIMIT 52",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?
    .iter()
    .map(|r| TrendPoint {
        captured_at: r.get("captured_at"),
        score: r.get::<i32, _>("score") as i64,
        consistency: r.get::<i32, _>("consistency") as i64,
        currency: r.get::<i32, _>("currency") as i64,
        liquidity: r.get::<i32, _>("liquidity") as i64,
        governance: r.get::<i32, _>("governance") as i64,
    })
    .collect();

    Ok(Json(KnowledgeHealthResponse {
        score,
        grade,
        pillars: KhPillars {
            consistency,
            currency,
            liquidity,
            governance,
        },
        signals: KhSignals {
            total_memories: total,
            canonical_entities: canon,
            cross_team_entities: cross_entities,
            open_contradictions: open_contra,
            cross_team_contradictions: cross_contra,
            cross_project_entities,
            cross_project_contradictions: cross_project_contra,
            stale_beliefs: stale,
            pages_dirty,
            oldest_dirty_secs,
            pages_pending_review,
            pages_published,
            page_reads_30d,
            agent_page_reads_30d,
            dirty_page_reads_30d,
            pages_never_read,
            standards_adopted: lib.standards_adopted,
            standards_at_gate: lib.standards_at_gate,
            oldest_gate_secs: lib.oldest_gate_secs,
            standards_dormant: lib.standards_dormant,
            skills_published: lib.skills_published,
            skills_dormant: lib.skills_dormant,
            org_wide,
            team_only,
            siloed_private: siloed,
            liquidity_pct,
            review_backlog: backlog,
            oldest_review_secs: oldest,
        },
        attention,
        trend,
        embedding_model: state.embedder.model_name().to_string(),
    }))
}

/// Record a Knowledge Health snapshot for the caller's org — the tick that builds
/// the trend line. An org runs this on a schedule (their cron or ours); each call
/// captures the current score + pillars + flagship signals into history. Needs a
/// `write` principal, since it mutates. Returns the snapshot it took.
#[utoipa::path(
    post,
    path = "/v1/analytics/knowledge-health/snapshot",
    tag = "analytics",
    description = "Record a Knowledge Health snapshot into the org's trend history (call weekly). Returns the captured score + grade.",
    responses((status = 200, description = "Snapshot recorded", body = SnapshotResponse))
)]
pub(crate) async fn knowledge_health_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SnapshotResponse>, HttpError> {
    let principal = auth_of(&state, &headers, "write").await?.principal;
    // Compute AND persist the snapshot at org-TRUE totals on the admin pool, so a
    // narrower-visibility member can no longer write a viewer-scoped number into
    // the shared org trend that the scheduled sweep (`snapshot_all_orgs`, also
    // org-true) feeds — mixing the two made the tracked trend line viewer-
    // dependent. Mirrors the sweep's insert, once, for the caller's org.
    let mut atx = state.admin_pool.begin().await.map_err(internal)?;
    let c = compute_health_core(&mut atx, Some(principal.org_id)).await?;
    let captured_at: DateTime<Utc> = sqlx::query(
        "INSERT INTO knowledge_health_snapshots
           (org_id, score, consistency, currency, liquidity, governance,
            cross_team_contradictions, stale_beliefs, total_memories)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING captured_at",
    )
    .bind(principal.org_id)
    .bind(c.score as i32)
    .bind(c.consistency as i32)
    .bind(c.currency as i32)
    .bind(c.liquidity as i32)
    .bind(c.governance as i32)
    .bind(c.cross_contra as i32)
    .bind(c.stale as i32)
    .bind(c.total as i32)
    .fetch_one(&mut *atx)
    .await
    .map_err(internal)?
    .get("captured_at");
    atx.commit().await.map_err(internal)?;
    Ok(Json(SnapshotResponse {
        captured_at,
        score: c.score,
        grade: grade_of(c.score).to_string(),
    }))
}

/// The scheduled cross-org Knowledge Health snapshot (the `health_snapshot`
/// sweep). Runs on the RLS-bypassing admin pool and writes one snapshot row per
/// org from that org's TRUE totals — this is what fills the trend line over
/// weeks without a leader having to click "snapshot" by hand. Mirrors the POST
/// handler's insert, once per org. Returns (orgs snapshotted, summary).
pub(crate) async fn snapshot_all_orgs(admin: &sqlx::PgPool) -> anyhow::Result<(usize, String)> {
    let orgs: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM orgs")
        .fetch_all(admin)
        .await?;
    let mut n = 0usize;
    for org in orgs {
        let mut tx = admin.begin().await?;
        let c = compute_health_core(&mut tx, Some(org))
            .await
            .map_err(|e| anyhow::anyhow!("health compute for org {org}: {}", e.message))?;
        sqlx::query(
            "INSERT INTO knowledge_health_snapshots
               (org_id, score, consistency, currency, liquidity, governance,
                cross_team_contradictions, stale_beliefs, total_memories)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(org)
        .bind(c.score as i32)
        .bind(c.consistency as i32)
        .bind(c.currency as i32)
        .bind(c.liquidity as i32)
        .bind(c.governance as i32)
        .bind(c.cross_contra as i32)
        .bind(c.stale as i32)
        .bind(c.total as i32)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        n += 1;
    }
    Ok((n, format!("{n} orgs snapshotted")))
}

// ── Practice divergence (the standardization surface) ───────────────────

#[derive(Serialize, ToSchema)]
pub(crate) struct PracticeDivergence {
    /// The named practice, e.g. "service retry policy".
    pub practice: String,
    /// One line: what actually differs between the teams.
    pub summary: String,
    /// The adjudicator's recommended single standard.
    pub recommended_standard: String,
    /// high | medium | low.
    pub impact: String,
    /// Each group's approach: [{team, approach}] on the team axis,
    /// [{project, approach}] on the project axis.
    pub approaches: serde_json::Value,
    /// The divergence class (PROJECT-PLAN PR3): `team` (two teams disagree)
    /// or `project` (two applications solve the same thing differently).
    pub axis: String,
    /// Which model adjudicated it — provenance for a decision-shaping report.
    pub model_ref: Option<String>,
    pub detected_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PracticeDivergenceResponse {
    /// Detected divergences, highest-impact first. Empty until the first scan.
    pub divergences: Vec<PracticeDivergence>,
}

#[utoipa::path(
    get,
    path = "/v1/analytics/practice-divergence",
    tag = "analytics",
    description = "Practice divergences the org should standardize — cross-team clusters an LLM sweep judged to be the SAME practice solved DIFFERENTLY, each with a recommended standard. RLS-scoped. Populated by the `scan-divergence` sweep.",
    responses((status = 200, description = "Detected practice divergences", body = PracticeDivergenceResponse))
)]
pub(crate) async fn practice_divergence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<PracticeDivergenceResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = sqlx::query(
        "SELECT practice, summary, recommended_standard, impact, positions, model_ref, detected_at, axis
         FROM practice_divergences
         ORDER BY CASE impact WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, detected_at DESC",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let divergences = rows
        .iter()
        .map(|r| PracticeDivergence {
            practice: r.get("practice"),
            summary: r.get("summary"),
            recommended_standard: r.get("recommended_standard"),
            impact: r.get("impact"),
            approaches: r.get("positions"),
            axis: r.get("axis"),
            model_ref: r.get("model_ref"),
            detected_at: r.get("detected_at"),
        })
        .collect();
    Ok(Json(PracticeDivergenceResponse { divergences }))
}

/// Trim a claim to a legible length for the attention list, on a char boundary.
/// "2d 16h" — a leader reads durations, not seconds.
fn human_age(secs: i64) -> String {
    if secs <= 0 {
        return "no time".into();
    }
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    match (d, h) {
        (0, 0) => format!("{m}m"),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h"),
    }
}

fn clip(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() > n {
        format!("{}…", t.chars().take(n).collect::<String>())
    } else {
        t.to_string()
    }
}

// ── cortex map (multi-level graph; never ships the whole graph at once) ──

/// A team lobe: the team plus its memory/entity volumes.
#[derive(Serialize, ToSchema)]
pub(crate) struct TeamLobe {
    pub id: Uuid,
    pub name: String,
    pub memories: i64,
    pub entities: i64,
}

/// A canonical hub with its team spread. `teams` is the DISTINCT team count;
/// `team_ids` is the set itself.
#[derive(Serialize, ToSchema)]
pub(crate) struct OverviewCanonical {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub memories: i64,
    pub teams: i64,
    pub team_ids: Vec<Uuid>,
    /// DISTINCT projects whose stamped memories anchor this hub (PR3).
    pub projects: i64,
    pub project_ids: Vec<Uuid>,
}

/// Binding strength between two team lobes = canonicals both teams link into.
#[derive(Serialize, ToSchema)]
pub(crate) struct TeamLink {
    pub a: Uuid,
    pub b: Uuid,
    pub shared: i64,
}

/// A project lobe (PROJECT-PLAN PR3): the application/domain plus its
/// stamped-memory volume and the entities those memories anchor. Only
/// projects with at least one stamped memory appear — an empty lobe is not
/// information; org-shared rows belong to every lobe and to none.
#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectLobe {
    pub id: Uuid,
    pub name: String,
    pub memories: i64,
    pub entities: i64,
}

/// Binding strength between two project lobes = canonicals anchored by
/// stamped memories of BOTH projects.
#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectLink {
    pub a: Uuid,
    pub b: Uuid,
    pub shared: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GraphOverviewResponse {
    pub teams: Vec<TeamLobe>,
    pub canonicals: Vec<OverviewCanonical>,
    pub team_links: Vec<TeamLink>,
    /// The project lens (PR3): lobes/links parallel to the team ones. Empty
    /// until writes are project-stamped.
    pub projects: Vec<ProjectLobe>,
    pub project_links: Vec<ProjectLink>,
}

/// L0/L1: team lobes with volumes, top canonical hubs with team spread, and
/// team-pair binding strength (shared canonicals). Bounded by construction.
#[utoipa::path(
    get,
    path = "/v1/graph/overview",
    tag = "graph",
    description = "Cortex map L0/L1: team lobes with volumes, the top 60 canonical hubs with their team spread, and team-pair binding strength.",
    responses((status = 200, description = "Cortex overview", body = GraphOverviewResponse))
)]
pub(crate) async fn graph_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GraphOverviewResponse>, HttpError> {
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
                array_agg(DISTINCT e.team_id) AS team_ids,
                count(DISTINCT m.project_id) FILTER (WHERE m.project_id IS NOT NULL) AS project_count,
                array_remove(array_agg(DISTINCT m.project_id), NULL) AS project_ids
         FROM canonical_entities ce
         JOIN entity_links l ON l.canonical_id = ce.id
         JOIN entities e ON e.id = l.entity_id
         LEFT JOIN memory_entities me ON me.entity_id = e.id
         LEFT JOIN memories m ON m.id = me.memory_id
         GROUP BY ce.id, ce.name, ce.kind
         ORDER BY memories DESC, team_count DESC, ce.name
         LIMIT 60",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    // The project lens (PR3): lobes from stamped memories, entity counts via
    // what those memories anchor. Projects with zero stamped rows are omitted.
    let projects = sqlx::query(
        "SELECT p.id, p.name,
                count(DISTINCT m.id) AS memories,
                count(DISTINCT me.entity_id) AS entities
         FROM projects p
         JOIN memories m ON m.project_id = p.id
         LEFT JOIN memory_entities me ON me.memory_id = m.id
         GROUP BY p.id, p.name
         ORDER BY p.name",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;

    let project_links = sqlx::query(
        "SELECT pa.project_id AS a, pb.project_id AS b, count(DISTINCT pa.canonical_id) AS shared
         FROM (SELECT DISTINCT l.canonical_id, m.project_id
               FROM entity_links l
               JOIN memory_entities me ON me.entity_id = l.entity_id
               JOIN memories m ON m.id = me.memory_id
               WHERE m.project_id IS NOT NULL) pa
         JOIN (SELECT DISTINCT l.canonical_id, m.project_id
               FROM entity_links l
               JOIN memory_entities me ON me.entity_id = l.entity_id
               JOIN memories m ON m.id = me.memory_id
               WHERE m.project_id IS NOT NULL) pb
           ON pa.canonical_id = pb.canonical_id AND pa.project_id < pb.project_id
         GROUP BY 1, 2",
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

    Ok(Json(GraphOverviewResponse {
        teams: teams
            .iter()
            .map(|r| TeamLobe {
                id: r.get("id"),
                name: r.get("name"),
                memories: r.get("memories"),
                entities: r.get("entities"),
            })
            .collect(),
        canonicals: canonicals
            .iter()
            .map(|r| OverviewCanonical {
                id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
                memories: r.get("memories"),
                // `teams` is the JSON name; the column is `team_count`.
                teams: r.get("team_count"),
                team_ids: r.get("team_ids"),
                projects: r.get("project_count"),
                project_ids: r.get("project_ids"),
            })
            .collect(),
        team_links: team_links
            .iter()
            .map(|r| TeamLink {
                a: r.get("a"),
                b: r.get("b"),
                shared: r.get("shared"),
            })
            .collect(),
        projects: projects
            .iter()
            .map(|r| ProjectLobe {
                id: r.get("id"),
                name: r.get("name"),
                memories: r.get("memories"),
                entities: r.get("entities"),
            })
            .collect(),
        project_links: project_links
            .iter()
            .map(|r| ProjectLink {
                a: r.get("a"),
                b: r.get("b"),
                shared: r.get("shared"),
            })
            .collect(),
    }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct CanonicalSummary {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub summary: Option<String>,
}

/// A team-scoped surface form linked into the canonical hub, with the link's
/// confidence and resolution method.
#[derive(Serialize, ToSchema)]
pub(crate) struct SurfaceForm {
    pub entity_id: Uuid,
    pub name: String,
    pub kind: String,
    pub team_id: Uuid,
    pub team: String,
    pub confidence: Option<f32>,
    pub method: Option<String>,
}

/// A 1-hop edge touching a surface form; `evidence` is null when the backing
/// memory is invisible to the caller.
#[derive(Serialize, ToSchema)]
pub(crate) struct CanonicalEdge {
    pub src: Uuid,
    pub src_name: String,
    pub dst: Uuid,
    pub dst_name: String,
    pub relation: String,
    pub memory_id: Option<Uuid>,
    pub evidence: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct NeighborCanonical {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub shared_edges: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AnchoredMemory {
    pub id: Uuid,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub team: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct CanonicalDetailResponse {
    pub canonical: CanonicalSummary,
    pub surface_forms: Vec<SurfaceForm>,
    pub edges: Vec<CanonicalEdge>,
    pub neighbors: Vec<NeighborCanonical>,
    pub memories: Vec<AnchoredMemory>,
}

/// L2/L3: one canonical entity's neighborhood — surface forms per team,
/// 1-hop evidence edges (content RLS-scoped), neighbor canonicals reachable
/// through those edges, and the anchored memories the caller may read.
#[utoipa::path(
    get,
    path = "/v1/graph/canonical/{id}",
    tag = "graph",
    description = "Cortex map L2/L3: one canonical entity's neighborhood — surface forms per team, 1-hop evidence edges, neighbor canonicals, and readable anchored memories.",
    params(("id" = Uuid, Path, description = "Canonical entity id")),
    responses(
        (status = 200, description = "Canonical neighborhood", body = CanonicalDetailResponse),
        (status = 404, description = "Canonical entity not found"),
    )
)]
pub(crate) async fn graph_canonical(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<CanonicalDetailResponse>, HttpError> {
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

    Ok(Json(CanonicalDetailResponse {
        canonical: CanonicalSummary {
            id: canonical.get("id"),
            name: canonical.get("name"),
            kind: canonical.get("kind"),
            summary: canonical.get("summary"),
        },
        surface_forms: surface_forms
            .iter()
            .map(|r| SurfaceForm {
                // `entity_id` is the JSON name; the column is `id`.
                entity_id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
                team_id: r.get("team_id"),
                team: r.get("team"),
                confidence: r.get("confidence"),
                method: r.get("method"),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|r| CanonicalEdge {
                src: r.get("src_entity"),
                src_name: r.get("src_name"),
                dst: r.get("dst_entity"),
                dst_name: r.get("dst_name"),
                relation: r.get("relation"),
                memory_id: r.get("memory_id"),
                evidence: r.get("evidence"),
            })
            .collect(),
        neighbors: neighbors
            .iter()
            .map(|r| NeighborCanonical {
                id: r.get("id"),
                name: r.get("name"),
                kind: r.get("kind"),
                shared_edges: r.get("shared_edges"),
            })
            .collect(),
        memories: memories
            .iter()
            .map(|r| AnchoredMemory {
                id: r.get("id"),
                content: r.get("content"),
                kind: r.get("kind"),
                status: r.get("status"),
                team: r.get("team"),
            })
            .collect(),
    }))
}

// ── archive (the memory ledger: as-of browsing + full lineage) ───────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct MemoriesListParams {
    /// Full-text query: websearch syntax over content, or a title substring.
    q: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    team: Option<Uuid>,
    visibility: Option<String>,
    /// Project facet: a project id, or `none` for org-shared rows (PR2).
    project: Option<String>,
    /// `recent` (default) | `valid_from` | `valid_to`. Pair with `dir`.
    sort: Option<String>,
    /// `asc` | `desc` (default). Ignored for `recent`.
    dir: Option<String>,
    /// RFC3339. When set, returns rows VALID at that instant — including
    /// deprecated ones that were true then. The archive's time travel.
    as_of: Option<String>,
    /// Compute the cross-filtered facet menu alongside the page. Off by default
    /// so a headless page-walk pays only for the rows it reads.
    #[serde(default, deserialize_with = "de_truthy")]
    facets: bool,
    #[serde(default = "default_list_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

/// Build the shared `MemoryFilter` from the request. Kept next to the handler
/// so REST and (later) MCP construct it identically.
fn memory_filter(
    p: &MemoriesListParams,
    as_of: Option<chrono::DateTime<chrono::Utc>>,
) -> brainiac_store::archive::MemoryFilter {
    brainiac_store::archive::MemoryFilter {
        q: p.q.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        kind: p.kind.clone(),
        status: p.status.clone(),
        team_id: p.team,
        visibility: p.visibility.clone(),
        project: p
            .project
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        as_of,
    }
}

/// Lenient boolean for query params: accepts `1`/`true`/`yes`/`on` (and their
/// negatives), because a browser `?facets=1` and a headless `?facets=true` must
/// both work — axum's default bool deserializer only accepts `true`/`false`.
pub(crate) fn de_truthy<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::String(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|v| v != 0),
        _ => false,
    })
}

fn default_list_limit() -> i64 {
    50
}

/// The archive's memory row. NOTE the timestamps here are **RFC3339
/// strings**, not `DateTime` — this handler stringifies them (unlike the
/// contradiction/audit/expiring payloads, which pass `DateTime` through).
/// `created_at` is non-null in practice but is read as an optional column, so
/// it stays `Option<String>` to keep the emitted shape identical.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryRow {
    pub id: Uuid,
    /// A short label for the claim. `None` for anything captured before
    /// migration 0023, and for anything the extractor wrote (it does not
    /// produce one yet) — readers fall back to `content`.
    pub title: Option<String>,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub visibility: String,
    pub team: String,
    pub team_id: Uuid,
    /// Project display name; null = org-shared (PROJECT-PLAN PR2).
    pub project: Option<String>,
    pub project_id: Option<Uuid>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
    pub created_at: Option<String>,
    pub confidence: Option<f32>,
}

/// One facet option and the count behind it, across the whole console. Value is
/// what a filter sends back; label is what the UI shows (they differ only for
/// teams, where the value is the id and the label is the name).
#[derive(Serialize, ToSchema)]
pub(crate) struct WireFacet {
    pub value: String,
    pub label: String,
    pub count: i64,
}

impl From<brainiac_store::feedback::Facet> for WireFacet {
    fn from(f: brainiac_store::feedback::Facet) -> Self {
        WireFacet {
            value: f.value,
            label: f.label,
            count: f.count,
        }
    }
}

fn wire_facets(v: Vec<brainiac_store::feedback::Facet>) -> Vec<WireFacet> {
    v.into_iter().map(Into::into).collect()
}

/// The archive's cross-filtered facet menu. Present only when `?facets=1`.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryFacetMenu {
    pub kinds: Vec<WireFacet>,
    pub statuses: Vec<WireFacet>,
    pub teams: Vec<WireFacet>,
    pub visibilities: Vec<WireFacet>,
    /// Value is a project id or `"none"`; label the name or `"org-shared"`.
    pub projects: Vec<WireFacet>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryListResponse {
    /// The filtered depth — what matches, ignoring the page window. `memories`
    /// is only the page; this is the number the archive counts by.
    pub total: i64,
    /// Cross-filtered facet menu, or null when `?facets=1` was not passed.
    pub facets: Option<MemoryFacetMenu>,
    pub memories: Vec<MemoryRow>,
}

/// Store row (DateTime) → wire row (RFC3339 strings), matching `memory_row`.
fn memory_list_row(r: brainiac_store::archive::MemoryListRow) -> MemoryRow {
    MemoryRow {
        id: r.id,
        title: r.title,
        content: r.content,
        kind: r.kind,
        status: r.status,
        visibility: r.visibility,
        team: r.team,
        team_id: r.team_id,
        project: r.project,
        project_id: r.project_id,
        valid_from: r.valid_from.map(|d| d.to_rfc3339()),
        valid_to: r.valid_to.map(|d| d.to_rfc3339()),
        superseded_by: r.superseded_by,
        created_at: r.created_at.map(|d| d.to_rfc3339()),
        confidence: r.confidence,
    }
}

fn memory_row(r: &sqlx::postgres::PgRow) -> MemoryRow {
    let ts = |col: &str| {
        r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
            .map(|d| d.to_rfc3339())
    };
    MemoryRow {
        id: r.get("id"),
        title: r.get("title"),
        content: r.get("content"),
        kind: r.get("kind"),
        status: r.get("status"),
        visibility: r.get("visibility"),
        team: r.get("team"),
        team_id: r.get("team_id"),
        project: r.get("project"),
        project_id: r.get("project_id"),
        valid_from: ts("valid_from"),
        valid_to: ts("valid_to"),
        superseded_by: r.get("superseded_by"),
        created_at: ts("created_at"),
        confidence: r.get("confidence"),
    }
}

#[utoipa::path(
    get,
    path = "/v1/memories",
    tag = "memories",
    description = "Browse the memory archive, optionally as-of an instant (time travel over the validity windows). Returns the filtered total alongside the page.",
    params(
        ("kind" = Option<String>, Query, description = "Filter by memory kind"),
        ("status" = Option<String>, Query, description = "Filter by memory status"),
        ("team" = Option<Uuid>, Query, description = "Filter by owning team id"),
        ("project" = Option<String>, Query, description = "Filter by project id, or `none` for org-shared rows"),
        ("as_of" = Option<String>, Query, description = "RFC3339 instant: return rows VALID then, including since-deprecated ones"),
        ("limit" = Option<i64>, Query, description = "Page size (default 50, clamped 1..200)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
    ),
    responses(
        (status = 200, description = "Archive page", body = MemoryListResponse),
        (status = 400, description = "as_of is not RFC3339"),
    )
)]
pub(crate) async fn memories_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<MemoriesListParams>,
    headers: HeaderMap,
) -> Result<Json<MemoryListResponse>, HttpError> {
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
    let sort = brainiac_store::archive::MemorySort::parse(p.sort.as_deref(), p.dir.as_deref());
    let filter = memory_filter(&p, as_of);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let rows = brainiac_store::archive::list(&mut tx, &filter, sort, limit, offset)
        .await
        .map_err(internal)?;
    let total = brainiac_store::archive::count(&mut tx, &filter)
        .await
        .map_err(internal)?;

    // Facets are the expensive part — four grouped passes — so a headless
    // page-walk that never renders a filter menu skips them entirely.
    let facets = if p.facets {
        let f = brainiac_store::archive::facets(&mut tx, &filter)
            .await
            .map_err(internal)?;
        Some(MemoryFacetMenu {
            kinds: wire_facets(f.kinds),
            statuses: wire_facets(f.statuses),
            teams: wire_facets(f.teams),
            visibilities: wire_facets(f.visibilities),
            projects: wire_facets(f.projects),
        })
    } else {
        None
    };

    Ok(Json(MemoryListResponse {
        total,
        facets,
        memories: rows.into_iter().map(memory_list_row).collect(),
    }))
}

/// One row of the as-of skeleton. RFC3339 timestamps, like the rest of the
/// archive payload.
#[derive(Serialize, ToSchema)]
pub(crate) struct ValidityRow {
    pub id: Uuid,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub status: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ValidityResponse {
    pub rows: Vec<ValidityRow>,
}

#[utoipa::path(
    get,
    path = "/v1/memories/validity",
    tag = "memories",
    description = "The as-of skeleton: {id, valid_from, valid_to, status} for every memory matching the filter (as_of excluded). Tiny by design — it lets a client scrub the archive's time axis instantly without holding the full corpus. Takes the same filters as /v1/memories minus paging.",
    params(
        ("q" = Option<String>, Query, description = "Full-text filter"),
        ("kind" = Option<String>, Query, description = "Filter by kind"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("team" = Option<Uuid>, Query, description = "Filter by owning team"),
        ("visibility" = Option<String>, Query, description = "Filter by visibility"),
    ),
    responses((status = 200, description = "Validity skeleton", body = ValidityResponse)),
)]
pub(crate) async fn memories_validity(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<MemoriesListParams>,
    headers: HeaderMap,
) -> Result<Json<ValidityResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    // as_of is meaningless here — the skeleton spans the whole timeline so the
    // client can apply time travel over it — so it is forced off.
    let filter = memory_filter(&p, None);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let skel = brainiac_store::archive::validity_skeleton(&mut tx, &filter)
        .await
        .map_err(internal)?;
    Ok(Json(ValidityResponse {
        rows: skel
            .into_iter()
            .map(|s| ValidityRow {
                id: s.id,
                valid_from: s.valid_from.map(|d| d.to_rfc3339()),
                valid_to: s.valid_to.map(|d| d.to_rfc3339()),
                status: s.status,
            })
            .collect(),
    }))
}

/// The provenance record behind a memory. The whole object is **null** (not
/// omitted) when the memory has no provenance row.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryProvenance {
    pub actor_kind: String,
    pub actor_id: String,
    pub model_ref: Option<String>,
    pub source_kind: Option<String>,
    pub source_ref: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryEntityRef {
    pub name: String,
    pub kind: String,
    pub team: String,
}

/// A promotion attempt on this memory. Timestamps are RFC3339 strings here.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryPromotion {
    pub from_status: String,
    pub to_status: String,
    pub policy_decision: String,
    pub policy_rule: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: Option<String>,
}

/// One link of the supersession lineage. `depth` is **signed**: negative for
/// predecessors (walking back), positive for successors (walking forward).
#[derive(Serialize, ToSchema)]
pub(crate) struct ChainLink {
    pub id: Uuid,
    pub content: String,
    pub status: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub depth: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryChain {
    pub predecessors: Vec<ChainLink>,
    pub successors: Vec<ChainLink>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryDetailResponse {
    pub memory: MemoryRow,
    pub provenance: Option<MemoryProvenance>,
    pub entities: Vec<MemoryEntityRef>,
    pub promotions: Vec<MemoryPromotion>,
    pub chain: MemoryChain,
}

#[utoipa::path(
    get,
    path = "/v1/memories/{id}",
    tag = "memories",
    description = "One memory with its provenance, anchored entities, promotion history, and supersession lineage (signed chain depth: negative = predecessors).",
    params(("id" = Uuid, Path, description = "Memory id")),
    responses(
        (status = 200, description = "Memory detail", body = MemoryDetailResponse),
        (status = 404, description = "Memory not found (or invisible under RLS)"),
    )
)]
pub(crate) async fn memory_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<MemoryDetailResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let row = sqlx::query(
        "SELECT m.id, m.title, m.content, m.kind, m.status::text AS status,
                m.visibility::text AS visibility, t.name AS team, m.team_id,
                pj.name AS project, m.project_id,
                m.valid_from, m.valid_to, m.superseded_by, m.created_at, m.confidence,
                pv.actor_kind, pv.actor_id, pv.model_ref,
                s.kind AS source_kind, s.external_ref AS source_ref
         FROM memories m
         JOIN teams t ON t.id = m.team_id
         LEFT JOIN projects pj ON pj.id = m.project_id
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
    // `dir` carries the sign: -1 for predecessors, +1 for successors.
    let chain_link = |r: &sqlx::postgres::PgRow, dir: i64| ChainLink {
        id: r.get("id"),
        content: r.get("content"),
        status: r.get("status"),
        valid_from: ts(r, "valid_from"),
        valid_to: ts(r, "valid_to"),
        depth: i64::from(r.get::<i32, _>("depth")) * dir,
    };

    Ok(Json(MemoryDetailResponse {
        memory: memory_row(&row),
        provenance: row
            .get::<Option<String>, _>("actor_kind")
            .map(|actor_kind| MemoryProvenance {
                actor_kind,
                actor_id: row.get("actor_id"),
                model_ref: row.get("model_ref"),
                source_kind: row.get("source_kind"),
                source_ref: row.get("source_ref"),
            }),
        entities: entities
            .iter()
            .map(|r| MemoryEntityRef {
                name: r.get("name"),
                kind: r.get("kind"),
                team: r.get("team"),
            })
            .collect(),
        promotions: promotions
            .iter()
            .map(|r| MemoryPromotion {
                from_status: r.get("from_status"),
                to_status: r.get("to_status"),
                policy_decision: r.get("policy_decision"),
                policy_rule: r.get("policy_rule"),
                reviewed_at: ts(r, "reviewed_at"),
                created_at: ts(r, "created_at"),
            })
            .collect(),
        chain: MemoryChain {
            predecessors: predecessors.iter().map(|r| chain_link(r, -1)).collect(),
            successors: successors.iter().map(|r| chain_link(r, 1)).collect(),
        },
    }))
}

// ── ingest monitor (recent sources + pipeline runs, list form) ───────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct RecentParams {
    #[serde(default = "default_recent_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_recent_limit() -> i64 {
    30
}

/// A source with its pipeline rollup. `created_at` is an RFC3339 **string**
/// here (this handler stringifies it). `status` is derived from the queue
/// job + memory count: queued | retrying | processed | failed | unknown.
#[derive(Serialize, ToSchema)]
pub(crate) struct SourceRow {
    pub id: Uuid,
    pub kind: String,
    pub external_ref: Option<String>,
    pub created_at: String,
    pub team: Option<String>,
    pub status: String,
    /// Queue delivery attempts; null when no queue job is known.
    pub attempts: Option<i32>,
    pub memories: i64,
    pub promoted: i64,
    pub pending_review: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SourceFeedResponse {
    /// Total sources visible to the caller — the full feed length, independent
    /// of the page window.
    pub total: i64,
    pub sources: Vec<SourceRow>,
}

/// Recent sources with their pipeline rollup — the monitor's feed. Queue
/// state joins outside RLS (queue schema is org-blind); org membership is
/// proven by the RLS read of `sources` itself.
#[utoipa::path(
    get,
    path = "/v1/sources",
    tag = "ingest",
    description = "Recent ingest sources with their pipeline rollup (memories, promoted, pending review) and derived queue status.",
    params(
        ("limit" = Option<i64>, Query, description = "Page size (default 30, clamped 1..100)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
    ),
    responses((status = 200, description = "Recent sources page", body = SourceFeedResponse))
)]
pub(crate) async fn sources_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<RecentParams>,
    headers: HeaderMap,
) -> Result<Json<SourceFeedResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = p.limit.clamp(1, 100);
    let offset = p.offset.max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let rows = sqlx::query(
        "SELECT s.id, s.kind, s.external_ref, s.created_at, t.name AS team,
                COALESCE(p.memories, 0) AS memories,
                COALESCE(p.promoted, 0) AS promoted,
                COALESCE(p.pending, 0) AS pending
         FROM sources s
         LEFT JOIN teams t ON t.id = s.team_id
         LEFT JOIN LATERAL (
             SELECT count(*) AS memories,
                    count(*) FILTER (WHERE m.status IN ('candidate','canonical')) AS promoted,
                    count(pr.id) FILTER (WHERE pr.policy_decision = 'needs_review'
                                           AND pr.reviewed_at IS NULL) AS pending
             FROM memories m
             JOIN provenance pv ON pv.id = m.provenance_id
             LEFT JOIN promotions pr ON pr.memory_id = m.id
             WHERE pv.source_id = s.id
         ) p ON true
         ORDER BY s.created_at DESC, s.id
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let total: i64 = sqlx::query("SELECT count(*) AS n FROM sources")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");
    drop(tx);

    let ids: Vec<String> = rows
        .iter()
        .map(|r| r.get::<Uuid, _>("id").to_string())
        .collect();
    let jobs = sqlx::query(
        "SELECT payload->>'source_id' AS sid, 'queued' AS state, attempts, NULL::text AS outcome
         FROM queue.jobs WHERE payload->>'source_id' = ANY($1)
         UNION ALL
         SELECT payload->>'source_id' AS sid, 'archived' AS state, attempts, outcome
         FROM queue.archive WHERE payload->>'source_id' = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(state.store.pool())
    .await
    .map_err(internal)?;
    let job_of = |sid: &str| {
        jobs.iter()
            .filter(|j| j.get::<Option<String>, _>("sid").as_deref() == Some(sid))
            .last()
    };

    let out: Vec<SourceRow> = rows
        .iter()
        .map(|r| {
            let id = r.get::<Uuid, _>("id");
            let memories: i64 = r.get("memories");
            let promoted: i64 = r.get("promoted");
            let pending: i64 = r.get("pending");
            let job = job_of(&id.to_string());
            let (job_state, attempts, outcome) = match job {
                Some(j) => (
                    Some(j.get::<String, _>("state")),
                    Some(j.get::<i32, _>("attempts")),
                    j.get::<Option<String>, _>("outcome"),
                ),
                None => (None, None, None),
            };
            let status = match (&job_state, &outcome, memories) {
                (Some(s), _, _) if s == "queued" && attempts == Some(0) => "queued",
                (Some(s), _, _) if s == "queued" => "retrying",
                (Some(s), Some(o), _) if s == "archived" && o == "ok" => "processed",
                (Some(_), _, _) => "failed",
                (None, _, 0) => "unknown",
                (None, _, _) => "processed",
            };
            SourceRow {
                id,
                kind: r.get("kind"),
                external_ref: r.get("external_ref"),
                created_at: r
                    .get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .to_rfc3339(),
                team: r.get("team"),
                status: status.to_string(),
                attempts,
                memories,
                promoted,
                pending_review: pending,
            }
        })
        .collect();

    Ok(Json(SourceFeedResponse {
        total,
        sources: out,
    }))
}

/// One worker run. `started_at` is an RFC3339 **string**; `duration_secs`
/// measures against `now()` while the run is still open.
#[derive(Serialize, ToSchema)]
pub(crate) struct PipelineRunRow {
    pub id: Uuid,
    pub stage: String,
    pub status: String,
    pub detail: Option<String>,
    pub started_at: String,
    pub duration_secs: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PipelineRunsResponse {
    /// Total pipeline runs visible to the caller — the full trail length,
    /// independent of the page window.
    pub total: i64,
    pub runs: Vec<PipelineRunRow>,
}

/// Recent pipeline runs — the worker's own audit trail, org-scoped by RLS.
#[utoipa::path(
    get,
    path = "/v1/pipeline/runs",
    tag = "ingest",
    description = "Recent pipeline runs — the worker's own audit trail, newest first, org-scoped by RLS. Paged: `total` reports the full trail, `offset` reaches beyond the first page.",
    params(
        ("limit" = Option<i64>, Query, description = "Page size (default 30, clamped 1..200)"),
        ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
    ),
    responses((status = 200, description = "Recent pipeline runs page", body = PipelineRunsResponse))
)]
pub(crate) async fn pipeline_runs(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<RecentParams>,
    headers: HeaderMap,
) -> Result<Json<PipelineRunsResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = p.limit.clamp(1, 200);
    let offset = p.offset.max(0);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = sqlx::query(
        "SELECT id, stage, status, detail, started_at, finished_at,
                EXTRACT(EPOCH FROM (COALESCE(finished_at, now()) - started_at))::bigint AS secs
         FROM pipeline_runs
         ORDER BY started_at DESC, id
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let total: i64 = sqlx::query("SELECT count(*) AS n FROM pipeline_runs")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?
        .get("n");
    Ok(Json(PipelineRunsResponse {
        total,
        runs: rows
            .iter()
            .map(|r| PipelineRunRow {
                id: r.get("id"),
                stage: r.get("stage"),
                status: r.get("status"),
                detail: r.get("detail"),
                started_at: r
                    .get::<chrono::DateTime<chrono::Utc>, _>("started_at")
                    .to_rfc3339(),
                duration_secs: r.get("secs"),
            })
            .collect(),
    }))
}

// ── keys (ground): blast-radius preview + the principal picker ───────────

/// Documentation-only mirror of one element of `OrgUser::teams`. The value is
/// produced by Postgres `json_agg(json_build_object('id','name','role'))` and
/// is forwarded verbatim as `serde_json::Value` so the bytes cannot drift;
/// this struct exists purely to give the schema its real shape.
#[derive(Serialize, ToSchema)]
pub(crate) struct OrgUserTeam {
    pub id: Uuid,
    pub name: String,
    /// `team_members.role` — e.g. `member` | `maintainer`.
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OrgUser {
    pub id: Uuid,
    pub email: String,
    /// Raw `json_agg` output — an array of `{id, name, role}` (empty array,
    /// never null, when the user is on no team).
    #[schema(value_type = Vec<OrgUserTeam>)]
    pub teams: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OrgUsersResponse {
    pub users: Vec<OrgUser>,
}

/// Org directory for the key-mint picker (admin scope — it feeds token
/// management). Users/teams carry no RLS; org scoping is explicit.
#[utoipa::path(
    get,
    path = "/v1/org/users",
    tag = "keys",
    description = "Org directory for the key-mint picker: every user in the caller's org with their team memberships and roles. Requires admin scope.",
    responses((status = 200, description = "Org directory", body = OrgUsersResponse))
)]
pub(crate) async fn org_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<OrgUsersResponse>, HttpError> {
    let ctx = crate::http::auth_of(&state, &headers, "admin").await?;
    // Scoped tx: team_members RLS needs app.org_id set; org filter stays
    // explicit for users/teams (no RLS there).
    let mut tx = state
        .store
        .scoped_tx(&ctx.principal)
        .await
        .map_err(internal)?;
    let rows = sqlx::query(
        "SELECT u.id, u.email,
                COALESCE(json_agg(json_build_object('id', t.id, 'name', t.name, 'role', tm.role)
                         ORDER BY t.name) FILTER (WHERE t.id IS NOT NULL), '[]'::json) AS teams
         FROM users u
         LEFT JOIN team_members tm ON tm.user_id = u.id
         LEFT JOIN teams t ON t.id = tm.team_id
         WHERE u.org_id = $1
         GROUP BY u.id, u.email
         ORDER BY u.email",
    )
    .bind(ctx.principal.org_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    Ok(Json(OrgUsersResponse {
        users: rows
            .iter()
            .map(|r| OrgUser {
                id: r.get("id"),
                email: r.get("email"),
                teams: r.get::<serde_json::Value, _>("teams"),
            })
            .collect(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PreviewBody {
    user_id: Uuid,
}

/// What a key minted for this principal could read, by visibility tier.
#[derive(Serialize, ToSchema)]
pub(crate) struct TokenVisibility {
    pub total: i64,
    pub org: i64,
    pub team: i64,
    pub private: i64,
    pub canonical: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct TokenPreviewResponse {
    pub user_id: Uuid,
    pub email: String,
    /// Team names, not ids — this is the human-facing picker preview.
    pub teams: Vec<String>,
    pub visible: TokenVisibility,
}

/// Blast radius: exactly what a key minted for this principal could read,
/// computed by opening a transaction AS that principal — the same RLS path
/// the runtime uses, so the preview can't drift from enforcement.
#[utoipa::path(
    post,
    path = "/v1/tokens/preview",
    tag = "keys",
    description = "Blast-radius preview for a key minted for a given user: what that principal could read, computed under their own RLS. Requires admin scope.",
    request_body = PreviewBody,
    responses(
        (status = 200, description = "Blast radius", body = TokenPreviewResponse),
        (status = 404, description = "User not found in this org"),
    )
)]
pub(crate) async fn token_preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PreviewBody>,
) -> Result<Json<TokenPreviewResponse>, HttpError> {
    let ctx = crate::http::auth_of(&state, &headers, "admin").await?;
    let org_id = ctx.principal.org_id;

    // The candidate must belong to the caller's org. Resolve identity and
    // team memberships under the CALLER's scope (team_members RLS needs
    // app.org_id), then re-open as the candidate for the radius itself.
    let (user_email, team_ids) = {
        let mut tx = state
            .store
            .scoped_tx(&ctx.principal)
            .await
            .map_err(internal)?;
        let user = sqlx::query("SELECT email FROM users WHERE id = $1 AND org_id = $2")
            .bind(body.user_id)
            .bind(org_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "user not found in this org".into()))?;
        let teams = sqlx::query(
            "SELECT tm.team_id FROM team_members tm
             JOIN teams t ON t.id = tm.team_id
             WHERE tm.user_id = $1 AND t.org_id = $2",
        )
        .bind(body.user_id)
        .bind(org_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?;
        (
            user.get::<String, _>("email"),
            teams
                .iter()
                .map(|r| r.get::<Uuid, _>("team_id"))
                .collect::<Vec<_>>(),
        )
    };

    let candidate = brainiac_core::Principal {
        org_id,
        user_id: body.user_id,
        team_ids: team_ids.clone(),
    };
    let mut tx = state.store.scoped_tx(&candidate).await.map_err(internal)?;
    let counts = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE visibility = 'org') AS org_tier,
                count(*) FILTER (WHERE visibility = 'team') AS team_tier,
                count(*) FILTER (WHERE visibility = 'private') AS private_tier,
                count(*) FILTER (WHERE status = 'canonical') AS canonical
         FROM memories",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let team_names = sqlx::query("SELECT t.name FROM teams t WHERE t.id = ANY($1) ORDER BY t.name")
        .bind(&team_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?;

    Ok(Json(TokenPreviewResponse {
        user_id: body.user_id,
        email: user_email,
        teams: team_names
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect(),
        visible: TokenVisibility {
            total: counts.get("total"),
            org: counts.get("org_tier"),
            team: counts.get("team_tier"),
            private: counts.get("private_tier"),
            canonical: counts.get("canonical"),
        },
    }))
}
