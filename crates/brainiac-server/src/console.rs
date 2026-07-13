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
}

/// `{status, count}` — the shape every status histogram in this module emits
/// (memories-by-status, contradiction tabs).
#[derive(Serialize, ToSchema)]
pub(crate) struct StatusCount {
    pub status: String,
    pub count: i64,
}

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
) -> Result<Json<ReviewDecisionResponse>, HttpError> {
    let principal = auth_of(state, headers, "write").await?.principal;
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
    Ok(Json(ReviewDecisionResponse {
        promotion_id: id,
        memory_id: pending.memory_id,
        decision: decision.to_string(),
        memory_status: new_status.as_str().to_string(),
    }))
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
                )
                    .into());
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
                )
                    .into());
            }
            // Store-owned primitive: deprecates the loser, closes valid_to,
            // sets superseded_by, AND records the transition in the
            // promotions audit log — the inline SQL this replaces skipped
            // the audit row.
            brainiac_store::governance::apply_supersession(
                &mut tx,
                principal.org_id,
                loser,
                winner,
                Some(principal.user_id),
                "contradiction_supersede",
            )
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
            )
                .into())
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
    Ok(Json(ResolveContradictionResponse {
        contradiction_id: id,
        status: status.to_string(),
    }))
}

// ── feedback triage (agent verdicts a maintainer must answer) ───────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct FeedbackQueueQuery {
    limit: Option<i64>,
}

/// Open claim counts against one memory.
#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackClaims {
    pub wrong: i64,
    pub outdated: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FlaggedMemory {
    pub memory_id: Uuid,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub team_id: Option<Uuid>,
    pub valid_to: Option<DateTime<Utc>>,
    pub claims: FeedbackClaims,
    /// Reporter notes on the open claims (most recent first, capped).
    pub notes: Vec<String>,
    /// Age of the OLDEST open claim — how long the dispute has stood.
    pub oldest_claim_secs: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FeedbackQueueResponse {
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
    params(("limit" = Option<i64>, Query, description = "Max rows (default 50)")),
    responses((status = 200, description = "Feedback triage queue", body = FeedbackQueueResponse))
)]
pub(crate) async fn feedback_queue(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FeedbackQueueQuery>,
    headers: HeaderMap,
) -> Result<Json<FeedbackQueueResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = brainiac_store::feedback::flagged(&mut tx, q.limit.unwrap_or(50))
        .await
        .map_err(internal)?;
    Ok(Json(FeedbackQueueResponse {
        flagged: rows
            .iter()
            .map(|f| FlaggedMemory {
                memory_id: f.memory_id,
                content: f.content.clone(),
                kind: f.kind.clone(),
                status: f.status.clone(),
                team_id: f.team_id,
                valid_to: f.valid_to,
                claims: FeedbackClaims {
                    wrong: f.wrong,
                    outdated: f.outdated,
                },
                notes: f.notes.clone(),
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
        (status = 403, description = "Caller is not a maintainer of the owning team"),
        (status = 404, description = "Memory not found (or invisible under RLS)"),
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

    // Invisible memory ⇒ 404, not 403 (no oracle) — same stance as promotions.
    let row = sqlx::query("SELECT team_id, kind FROM memories WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "memory not found".into()))?;
    if let Some(team_id) = row.get::<Option<Uuid>, _>("team_id") {
        if !is_maintainer(&mut tx, &principal, team_id).await? {
            return Err((
                StatusCode::FORBIDDEN,
                "answering feedback claims requires a maintainer of the owning team".into(),
            )
                .into());
        }
    }

    let mut new_valid_to: Option<chrono::DateTime<chrono::Utc>> = None;
    match body.resolution.as_str() {
        "reverified" => {
            let kind = brainiac_core::MemoryKind::parse(&row.get::<String, _>("kind"));
            let days = body
                .days
                .unwrap_or_else(|| kind.map_or(365, |k| i64::from(k.default_ttl_days())))
                .clamp(1, 3650);
            new_valid_to = brainiac_store::memories::extend_validity(&mut tx, id, days)
                .await
                .map_err(internal)?;
        }
        "deprecated" => {
            // The reporters were right: end the window now and drop it out of
            // retrieval, without inventing a supersessor it doesn't have.
            sqlx::query(
                "UPDATE memories
                 SET status = 'deprecated'::memory_status, valid_to = now(), updated_at = now()
                 WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
        }
        _ => {} // dismissed: the memory stands untouched
    }

    let closed =
        brainiac_store::feedback::resolve_claims(&mut tx, id, principal.user_id, &body.resolution)
            .await
            .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(ResolveFeedbackResponse {
        memory_id: id,
        resolution: body.resolution,
        claims_closed: closed,
        valid_to: new_valid_to,
    }))
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
    let new_valid_to = brainiac_store::memories::extend_validity(&mut tx, id, days)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "memory not found".into()))?;
    // Re-verifying answers any open feedback claims against this memory —
    // a maintainer who just confirmed it is true has, in fact, responded.
    let claims_closed =
        brainiac_store::feedback::resolve_claims(&mut tx, id, principal.user_id, "reverified")
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
}

/// One governance action. `kind` is `promotion_review` |
/// `contradiction_resolution`; `memory_b` is only set for contradictions.
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
    pub events: Vec<AuditEvent>,
}

/// Reverse-chronological feed of governance actions: promotion reviews
/// (human and policy) and contradiction resolutions. Reuses the reviewer /
/// resolved-by columns both tables already carry; rows resolve under the
/// caller's RLS transaction so members see their org slice only.
#[utoipa::path(
    get,
    path = "/v1/audit",
    tag = "reviews",
    description = "Reverse-chronological feed of governance actions: promotion reviews (human and policy) and contradiction resolutions.",
    params(("limit" = Option<i64>, Query, description = "Max events (default 50, clamped 1..200)")),
    responses((status = 200, description = "Audit feed", body = AuditResponse))
)]
pub(crate) async fn audit(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
    headers: HeaderMap,
) -> Result<Json<AuditResponse>, HttpError> {
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
    Ok(Json(AuditResponse { events: out }))
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
    let contradictions_open: i64 =
        sqlx::query("SELECT count(*) AS n FROM contradictions WHERE status = 'open'")
            .fetch_one(&mut *tx)
            .await
            .map_err(internal)?
            .get("n");
    let flagged_memories = brainiac_store::feedback::flagged_count(&mut tx)
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
}

/// Binding strength between two team lobes = canonicals both teams link into.
#[derive(Serialize, ToSchema)]
pub(crate) struct TeamLink {
    pub a: Uuid,
    pub b: Uuid,
    pub shared: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GraphOverviewResponse {
    pub teams: Vec<TeamLobe>,
    pub canonicals: Vec<OverviewCanonical>,
    pub team_links: Vec<TeamLink>,
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

/// The archive's memory row. NOTE the timestamps here are **RFC3339
/// strings**, not `DateTime` — this handler stringifies them (unlike the
/// contradiction/audit/expiring payloads, which pass `DateTime` through).
/// `created_at` is non-null in practice but is read as an optional column, so
/// it stays `Option<String>` to keep the emitted shape identical.
#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryRow {
    pub id: Uuid,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub visibility: String,
    pub team: String,
    pub team_id: Uuid,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
    pub created_at: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemoryListResponse {
    pub total: i64,
    pub memories: Vec<MemoryRow>,
}

fn memory_row(r: &sqlx::postgres::PgRow) -> MemoryRow {
    let ts = |col: &str| {
        r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
            .map(|d| d.to_rfc3339())
    };
    MemoryRow {
        id: r.get("id"),
        content: r.get("content"),
        kind: r.get("kind"),
        status: r.get("status"),
        visibility: r.get("visibility"),
        team: r.get("team"),
        team_id: r.get("team_id"),
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

    Ok(Json(MemoryListResponse {
        total,
        memories: rows.iter().map(memory_row).collect(),
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
    params(("limit" = Option<i64>, Query, description = "Max rows (default 30, clamped 1..100)")),
    responses((status = 200, description = "Recent sources", body = SourceFeedResponse))
)]
pub(crate) async fn sources_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<RecentParams>,
    headers: HeaderMap,
) -> Result<Json<SourceFeedResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = p.limit.clamp(1, 100);
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
         ORDER BY s.created_at DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
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

    Ok(Json(SourceFeedResponse { sources: out }))
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
    pub runs: Vec<PipelineRunRow>,
}

/// Recent pipeline runs — the worker's own audit trail, org-scoped by RLS.
#[utoipa::path(
    get,
    path = "/v1/pipeline/runs",
    tag = "ingest",
    description = "Recent pipeline runs — the worker's own audit trail, newest first, org-scoped by RLS.",
    params(("limit" = Option<i64>, Query, description = "Max rows (default 30, clamped 1..200)")),
    responses((status = 200, description = "Recent pipeline runs", body = PipelineRunsResponse))
)]
pub(crate) async fn pipeline_runs(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<RecentParams>,
    headers: HeaderMap,
) -> Result<Json<PipelineRunsResponse>, HttpError> {
    let principal = principal_of(&state, &headers).await?;
    let limit = p.limit.clamp(1, 200);
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rows = sqlx::query(
        "SELECT id, stage, status, detail, started_at, finished_at,
                EXTRACT(EPOCH FROM (COALESCE(finished_at, now()) - started_at))::bigint AS secs
         FROM pipeline_runs
         ORDER BY started_at DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    Ok(Json(PipelineRunsResponse {
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
