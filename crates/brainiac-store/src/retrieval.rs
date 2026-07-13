//! Retrieval engine — the hot path (ARCHITECTURE.md §4).
//!
//! Stages implemented in v0 (rerank is deferred to the bake-off):
//! 1. embed query (caller-supplied [`Embedder`])
//! 2. parallel candidates: pgvector ANN top-50 + FTS top-50 (both RLS-scoped)
//! 3. reciprocal rank fusion → top 30
//! 4. graph expansion: anchors of the top hits → 1–2 hop neighbors via the
//!    canonical bridge → their strongest memories (bounded +10). This is
//!    where cross-team knowledge surfaces; each graph extra is scored as a
//!    decayed fraction of the anchoring direct hit rather than pinned to 0.
//! 5. assembly: temporal filter + supersession-chain dedupe (as-of aware),
//!    then a blended re-rank (relevance dominant, recency/feedback nudges) over
//!    direct and graph hits alike; entity anchors attached to the emitted set.
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
use brainiac_core::rerank::Reranker;
use brainiac_core::scoring::{blended_score, graph_relevance, FeedbackSignal};
use brainiac_core::temporal::{dedupe_for_time, valid_at};
use brainiac_core::{Memory, MemoryStatus};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{entities, feedback, memories};

const CANDIDATES_PER_RETRIEVER: i64 = 50;
const FUSED_POOL: usize = 30;
const GRAPH_ANCHOR_TOP: usize = 10;
const GRAPH_EXTRA_MEMORIES: i64 = 10;
const GRAPH_HOPS: u8 = 2;
const RRF_K: f64 = 60.0;

/// Stage-5 rerank budget (ARCHITECTURE.md §4: "cross-encoder rerank over ≤40
/// candidates"). The assembled+deduped survivor set is already ≤ `FUSED_POOL`
/// (30) + `GRAPH_EXTRA_MEMORIES` (10) = 40, so in practice every survivor is
/// reranked; the cap is an explicit ceiling so a future wider assembly can't
/// silently hand a cross-encoder an unbounded batch. Survivors beyond the cap
/// (none today) keep their pre-rerank relevance.
const RERANK_MAX_CANDIDATES: usize = 40;

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

/// A canonical entity that anchors a hit (ARCHITECTURE.md §5.1: results carry
/// "memories + provenance + entity anchors"). For direct hits these are the
/// query-independent anchors of the memory; for `via_graph` hits they are also
/// the bridge the hit was surfaced through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityAnchor {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub memory: Memory,
    /// Final blended ranking key (brainiac-core::scoring): relevance as the
    /// dominant term, nudged by recency decay and net reader feedback. Relevance
    /// is the fused RRF score for direct hits and a graph-derived score (anchor
    /// strength × hop decay) for graph-expansion extras, so both live on one
    /// scale and a strong cross-team hit can outrank a weak direct hit.
    pub score: f64,
    pub via_graph: bool,
    /// Canonical entities anchoring this hit (id + name), name-sorted; empty
    /// when the memory has no canonical-linked entities.
    pub anchors: Vec<EntityAnchor>,
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

/// Hybrid retrieval with NO stage-5 reranker — the byte-identical baseline.
/// Thin delegate to [`search_reranked`] with `None`; every existing caller
/// (eval, mcp, tests, the default HTTP path) uses this, so turning the seam
/// off is guaranteed identical by construction, not by matching behavior.
pub async fn search(
    tx: &mut sqlx::PgConnection,
    pool: &PgPool,
    embedder: &dyn Embedder,
    embedding_version: i32,
    req: &RetrievalRequest,
) -> Result<Vec<RetrievalHit>> {
    search_reranked(tx, pool, embedder, None, embedding_version, req).await
}

/// Hybrid retrieval with the optional stage-5 cross-encoder rerank
/// (ARCHITECTURE.md §4). When `reranker` is `None` this is byte-identical to
/// [`search`]; when `Some`, the assembled+deduped ≤40 survivors are rescored
/// by the reranker BEFORE the recency/feedback blend and truncation — the
/// reranker's joint `(query, candidate)` score REPLACES the fused/graph
/// relevance term feeding [`blended_score`], so the tiebreak nudges still apply
/// on top of it and both direct and graph hits share one post-rerank scale.
pub async fn search_reranked(
    tx: &mut sqlx::PgConnection,
    pool: &PgPool,
    embedder: &dyn Embedder,
    reranker: Option<&dyn Reranker>,
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
    // Anchor strength for graph-expansion scoring: the strongest direct hit
    // (fused is score-descending, so the first is the max). Graph extras are
    // scored as a decayed fraction of this — see `graph_relevance`.
    let anchor_strength = fused.first().map(|(_, s)| *s).unwrap_or(0.0);

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

    // Stage 5: cross-encoder rerank (ARCHITECTURE.md §4). Over the ≤40
    // assembled+deduped survivors, BEFORE the blend, a reranker rescores each
    // (query, candidate-text) pair jointly. Its score REPLACES the fused/graph
    // relevance below, so the recency/feedback nudges still ride on top. With
    // no reranker configured this whole stage is skipped and relevance stays
    // the fused/graph score — byte-identical to [`search`].
    let rerank_scores: Option<std::collections::HashMap<Uuid, f64>> = match reranker {
        Some(r) => {
            let capped: Vec<(Uuid, &str)> = deduped
                .iter()
                .take(RERANK_MAX_CANDIDATES)
                .map(|m| (m.id, m.content.as_str()))
                .collect();
            let scores = r.rerank(&req.query, &capped).await?;
            Some(
                capped
                    .iter()
                    .map(|(id, _)| *id)
                    .zip(scores.into_iter().map(|s| s as f64))
                    .collect(),
            )
        }
        None => None,
    };

    // Stage 6: blended ranking (ARCHITECTURE.md §4 "order by relevance +
    // recency"). Fused relevance stays dominant; recency and net reader
    // feedback are tiebreak-scale nudges (brainiac-core::scoring). One batched
    // feedback lookup for the whole survivor set — never an N+1. Trust is
    // RLS-scoped like every read, so a memory's verdicts from invisible orgs
    // never leak in (memory_feedback is org-scoped).
    let survivor_ids: Vec<Uuid> = deduped.iter().map(|m| m.id).collect();
    let trust = feedback::trust_for(tx, &survivor_ids).await?;

    let mut ranked: Vec<RetrievalHit> = deduped
        .into_iter()
        .map(|m| {
            // Direct hits carry their fused RRF score; graph extras (fused 0.0)
            // earn a graph-derived relevance = anchor strength × hop decay, so a
            // strong cross-team hit can outrank a weak direct hit instead of
            // always sinking. Both then feed the same recency/feedback blend.
            let via_graph = graph_ids.contains(&m.id);
            // Pre-rerank relevance: fused RRF for direct hits, graph-derived for
            // expansion extras (the byte-identical baseline).
            let base_relevance = match fused_score.get(&m.id) {
                Some(s) => *s,
                None if via_graph => graph_relevance(anchor_strength),
                None => 0.0,
            };
            // Stage 5: if a reranker scored this survivor, its joint score
            // replaces the base relevance; survivors past the cap (none today)
            // fall back to the base. No reranker ⇒ always the base.
            let relevance = rerank_scores
                .as_ref()
                .and_then(|rs| rs.get(&m.id).copied())
                .unwrap_or(base_relevance);
            let t = trust.get(&m.id).cloned().unwrap_or_default();
            let feedback = FeedbackSignal {
                helpful: t.helpful,
                wrong: t.wrong,
                outdated: t.outdated,
            };
            let blended = blended_score(relevance, age_days(&m, as_of), feedback);
            RetrievalHit {
                score: blended,
                via_graph,
                anchors: Vec::new(),
                memory: m,
            }
        })
        .collect();
    // Stable sort: candidates that tie on the blended key keep their prior
    // (fused-then-graph) order, so the nudges only ever reorder near-ties.
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(req.k);

    // Attach entity anchors to the emitted hits (ARCHITECTURE.md §5.1). Batched
    // over just the final top-k, RLS-scoped like every read.
    let hit_ids: Vec<Uuid> = ranked.iter().map(|h| h.memory.id).collect();
    let mut anchors = entities::canonical_anchors_for(tx, &hit_ids).await?;
    for h in &mut ranked {
        h.anchors = anchors.remove(&h.memory.id).unwrap_or_default();
    }
    Ok(ranked)
}

/// Age of a memory (in days) at the query's as-of instant, for the recency
/// term. Anchored on `created_at` — when the knowledge was captured/confirmed
/// into the corpus — NOT `valid_from`. Recency here means "how current is this
/// knowledge", i.e. how recently we learned or re-verified it; a decision made
/// two years ago that is still valid (open `valid_to`) is current knowledge
/// even though its `valid_from` is old, so anchoring on `valid_from` would
/// wrongly decay long-standing truths. `created_at` is always set, so there is
/// no fallback. Negative ages (a clock skew, or an as-of before capture) are
/// clamped to fresh by the decay function.
///
/// Age is quantized to WHOLE DAYS (`num_days` truncates toward zero). Recency
/// is a coarse currency signal — a memory captured seconds or hours before
/// another is not meaningfully "fresher", and sub-day resolution would let
/// insertion micro-timing break otherwise-exact relevance ties. Day
/// granularity keeps the term a genuine tiebreak: candidates from the same day
/// stay tied on recency and hold their fused order.
fn age_days(m: &Memory, as_of: DateTime<Utc>) -> f64 {
    (as_of - m.created_at).num_days() as f64
}
