//! Retrieval engine — the hot path (ARCHITECTURE.md §4).
//!
//! Stages implemented in v0 (rerank is deferred to the bake-off):
//! 1. embed query (caller-supplied [`Embedder`])
//! 2. parallel candidates: pgvector ANN top-50 + FTS top-50 (both RLS-scoped)
//! 3. reciprocal rank fusion → top 30
//! 4. graph expansion: anchors of the top hits → 1–2 hop neighbors via the
//!    canonical bridge → their strongest memories (bounded +10). This is
//!    where cross-team knowledge surfaces.
//! 5. assembly: temporal filter + supersession-chain dedupe (as-of aware),
//!    fused order preserved, graph extras appended.
//!
//! The graph + assembly stages run inside the caller's scoped transaction. The
//! two candidate retrievers (stage 2) run concurrently on separate pooled
//! connections, each re-stamped with the caller's exact RLS scope (org/user,
//! and the worker escape if set), so the principal's RLS applies to ANN, FTS,
//! and graph reads alike.

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::fusion::{
    query_is_identifier_heavy, reciprocal_rank_fusion, weighted_reciprocal_rank_fusion,
};
use brainiac_core::temporal::{dedupe_for_time, valid_at};
use brainiac_core::{Memory, MemoryStatus};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{entities, memories};

const CANDIDATES_PER_RETRIEVER: i64 = 50;
const FUSED_POOL: usize = 30;
const GRAPH_ANCHOR_TOP: usize = 10;
const GRAPH_EXTRA_MEMORIES: i64 = 10;
const GRAPH_HOPS: u8 = 2;
const RRF_K: f64 = 60.0;

// Fusion weights for identifier-heavy queries (repo/service names, dotted
// paths, error codes): exact lexical matches should lead, so the FTS list
// pulls harder than the vector list. Plain queries bypass these entirely and
// use unweighted RRF — byte-identical to prior behavior. Order mirrors the
// `[vector, fts]` list order passed to the fusion.
const IDENT_VECTOR_WEIGHT: f64 = 1.0;
const IDENT_FTS_WEIGHT: f64 = 2.0;

/// Metadata narrowing on top of relevance. All fields are conjunctive;
/// the default filters nothing (beyond the standing `rejected` exclusion).
#[derive(Debug, Clone, Default)]
pub struct RetrievalFilters {
    /// Memory kinds to keep (`fact`, `decision`, …); empty = all kinds.
    pub kinds: Vec<String>,
    /// Trust floor: `Candidate` keeps candidate+canonical, `Canonical`
    /// keeps canonical only. `None` = today's default (all but rejected).
    pub min_status: Option<MemoryStatus>,
    /// Restrict to one team's memories.
    pub team_id: Option<Uuid>,
    /// Minimum extractor confidence (memories with NULL confidence drop).
    pub min_confidence: Option<f32>,
}

impl RetrievalFilters {
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
            && self.min_status.is_none()
            && self.team_id.is_none()
            && self.min_confidence.is_none()
    }

    /// Statuses admitted by the floor, as SQL enum literals; `None` = no
    /// floor (only the standing `rejected` exclusion applies).
    pub(crate) fn allowed_statuses(&self) -> Option<Vec<String>> {
        let floor = self.min_status?;
        let order = [
            MemoryStatus::Raw,
            MemoryStatus::Candidate,
            MemoryStatus::Canonical,
        ];
        Some(
            order
                .iter()
                .skip_while(|s| **s != floor)
                .map(|s| s.as_str().to_string())
                .collect(),
        )
    }

    /// The post-SQL check, applied to graph-expansion extras too (they join
    /// the result set after the filtered candidate stage).
    fn admits(&self, m: &Memory) -> bool {
        (self.kinds.is_empty() || self.kinds.iter().any(|k| k == m.kind.as_str()))
            && self
                .allowed_statuses()
                .is_none_or(|ok| ok.iter().any(|s| s == m.status.as_str()))
            && self.team_id.is_none_or(|t| m.team_id == Some(t))
            && self
                .min_confidence
                .is_none_or(|c| m.confidence.is_some_and(|mc| mc >= c))
    }
}

pub struct RetrievalRequest {
    pub query: String,
    pub k: usize,
    /// Point-in-time view; `None` = now.
    pub as_of: Option<DateTime<Utc>>,
    pub filters: RetrievalFilters,
}

#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub memory: Memory,
    /// RRF-fused score for direct hits; 0.0 for graph-expansion extras
    /// (appended after the fused ranking).
    pub score: f64,
    pub via_graph: bool,
}

/// The RLS scope of the caller's transaction, mirrored onto the pooled
/// connections that run the candidate retrievers so they see EXACTLY what the
/// caller would — scoped_tx or worker_tx alike.
struct RlsScope {
    org_id: String,
    user_id: String,
    /// `'on'` for worker transactions; absent otherwise.
    worker: Option<String>,
}

/// Read the principal's per-connection RLS settings back off the caller's tx.
/// `set_config(..., true)` made them transaction-local, so `current_setting`
/// on the same tx recovers them without needing the `Principal` object.
async fn read_scope(tx: &mut sqlx::PgConnection) -> Result<RlsScope> {
    let row = sqlx::query(
        "SELECT current_setting('app.org_id', true) AS org_id,
                current_setting('app.user_id', true) AS user_id,
                current_setting('app.worker', true) AS worker",
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(RlsScope {
        org_id: row
            .try_get::<Option<String>, _>("org_id")?
            .unwrap_or_default(),
        user_id: row
            .try_get::<Option<String>, _>("user_id")?
            .unwrap_or_default(),
        worker: row.try_get::<Option<String>, _>("worker")?,
    })
}

/// Open a fresh pooled transaction and stamp it with the caller's RLS scope.
/// The pool's `after_connect` hook has already dropped the session to the
/// non-owner `brainiac_app` role, so these settings are load-bearing: without
/// them the candidate query would silently return nothing (or, on a raw
/// unscoped session, leak). Transaction-local (`set_config(..., true)`) so the
/// connection returns to the pool clean.
async fn scoped_pool_tx<'a>(pool: &'a PgPool, scope: &RlsScope) -> Result<crate::Tx<'a>> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "SELECT set_config('app.org_id', $1, true),
                set_config('app.user_id', $2, true),
                set_config('app.worker', $3, true)",
    )
    .bind(&scope.org_id)
    .bind(&scope.user_id)
    .bind(scope.worker.as_deref().unwrap_or("off"))
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}

pub async fn search(
    tx: &mut sqlx::PgConnection,
    pool: &PgPool,
    embedder: &dyn Embedder,
    embedding_version: i32,
    req: &RetrievalRequest,
) -> Result<Vec<RetrievalHit>> {
    let as_of = req.as_of.unwrap_or_else(Utc::now);

    // Stage 2: candidate lists (ranked best-first), filters pushed into SQL
    // so narrowed searches don't waste their candidate budget on rows the
    // assembly stage would drop anyway. The two retrievers run CONCURRENTLY on
    // separate pooled connections — a single &mut PgConnection can't be shared
    // across futures — each re-stamped with the caller's RLS scope so both
    // still read under the caller's principal.
    let query_vec = embedder.embed(&req.query).await?;
    let scope = read_scope(tx).await?;

    let vector_fut = async {
        let mut c = scoped_pool_tx(pool, &scope).await?;
        let hits = memories::search_vector(
            &mut c,
            embedding_version,
            &query_vec,
            CANDIDATES_PER_RETRIEVER,
            &req.filters,
        )
        .await?;
        // Read-only: let the tx roll back as the connection returns to the pool.
        Ok::<_, anyhow::Error>(hits)
    };
    let fts_fut = async {
        let mut c = scoped_pool_tx(pool, &scope).await?;
        let hits = memories::search_fts(&mut c, &req.query, CANDIDATES_PER_RETRIEVER, &req.filters)
            .await?;
        Ok::<_, anyhow::Error>(hits)
    };
    let (vector_hits, fts_hits) = tokio::try_join!(vector_fut, fts_fut)?;

    let vector_ranked: Vec<Uuid> = vector_hits.iter().map(|(id, _)| *id).collect();
    let fts_ranked: Vec<Uuid> = fts_hits.iter().map(|(id, _)| *id).collect();

    // Stage 3: fusion. Identifier-heavy queries bias toward the lexical list;
    // plain queries use unweighted RRF (byte-identical to prior behavior).
    let lists = [vector_ranked, fts_ranked];
    let fused = if query_is_identifier_heavy(&req.query) {
        weighted_reciprocal_rank_fusion(
            &lists,
            &[IDENT_VECTOR_WEIGHT, IDENT_FTS_WEIGHT],
            RRF_K,
            FUSED_POOL,
        )
    } else {
        reciprocal_rank_fusion(&lists, RRF_K, FUSED_POOL)
    };
    let fused_ids: Vec<Uuid> = fused.iter().map(|(id, _)| *id).collect();
    let fused_score: std::collections::HashMap<Uuid, f64> = fused.iter().cloned().collect();

    // Stage 4: graph expansion from the strongest direct hits.
    let anchor_source: Vec<Uuid> = fused_ids.iter().take(GRAPH_ANCHOR_TOP).copied().collect();
    let anchors = entities::anchors_of_memories(tx, &anchor_source).await?;
    let neighbor_entities = entities::neighbors(tx, &anchors, GRAPH_HOPS, 200).await?;
    let graph_memories =
        memories::for_entities(tx, &neighbor_entities, GRAPH_EXTRA_MEMORIES).await?;

    // Stage 5: assembly. Fetch direct hits, keep fused order, append graph
    // extras that aren't already present.
    let direct = memories::get_by_ids(tx, &fused_ids).await?;
    let mut ordered: Vec<Memory> = Vec::with_capacity(direct.len() + graph_memories.len());
    let by_id: std::collections::HashMap<Uuid, Memory> =
        direct.into_iter().map(|m| (m.id, m)).collect();
    for id in &fused_ids {
        if let Some(m) = by_id.get(id) {
            ordered.push(m.clone());
        }
    }
    let mut seen: std::collections::HashSet<Uuid> = ordered.iter().map(|m| m.id).collect();
    let mut graph_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for m in graph_memories {
        if seen.insert(m.id) {
            graph_ids.insert(m.id);
            ordered.push(m);
        }
    }

    // Temporal correctness: drop rows outside their validity window at the
    // requested time, then collapse supersession chains to the single member
    // correct for that time. Metadata filters re-apply here because graph
    // extras bypass the filtered candidate stage.
    let ordered: Vec<Memory> = ordered
        .into_iter()
        .filter(|m| valid_at(m, as_of) && req.filters.admits(m))
        .collect();
    let deduped = dedupe_for_time(&ordered, as_of);

    Ok(deduped
        .into_iter()
        .take(req.k)
        .map(|m| RetrievalHit {
            score: fused_score.get(&m.id).copied().unwrap_or(0.0),
            via_graph: graph_ids.contains(&m.id),
            memory: m,
        })
        .collect())
}
