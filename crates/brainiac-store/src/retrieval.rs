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
//! Every SQL stage runs inside the caller's scoped transaction, so the
//! principal's RLS applies to ANN, FTS, and graph reads alike.

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::fusion::reciprocal_rank_fusion;
use brainiac_core::temporal::{dedupe_for_time, valid_at};
use brainiac_core::Memory;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{entities, memories};

const CANDIDATES_PER_RETRIEVER: i64 = 50;
const FUSED_POOL: usize = 30;
const GRAPH_ANCHOR_TOP: usize = 10;
const GRAPH_EXTRA_MEMORIES: i64 = 10;
const GRAPH_HOPS: u8 = 2;
const RRF_K: f64 = 60.0;

pub struct RetrievalRequest {
    pub query: String,
    pub k: usize,
    /// Point-in-time view; `None` = now.
    pub as_of: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub memory: Memory,
    /// RRF-fused score for direct hits; 0.0 for graph-expansion extras
    /// (appended after the fused ranking).
    pub score: f64,
    pub via_graph: bool,
}

pub async fn search(
    tx: &mut sqlx::PgConnection,
    embedder: &dyn Embedder,
    embedding_version: i32,
    req: &RetrievalRequest,
) -> Result<Vec<RetrievalHit>> {
    let as_of = req.as_of.unwrap_or_else(Utc::now);

    // Stage 2: candidate lists (ranked best-first).
    let query_vec = embedder.embed(&req.query);
    let vector_hits =
        memories::search_vector(tx, embedding_version, &query_vec, CANDIDATES_PER_RETRIEVER)
            .await?;
    let fts_hits = memories::search_fts(tx, &req.query, CANDIDATES_PER_RETRIEVER).await?;

    let vector_ranked: Vec<Uuid> = vector_hits.iter().map(|(id, _)| *id).collect();
    let fts_ranked: Vec<Uuid> = fts_hits.iter().map(|(id, _)| *id).collect();

    // Stage 3: fusion.
    let fused = reciprocal_rank_fusion(&[vector_ranked, fts_ranked], RRF_K, FUSED_POOL);
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
    // correct for that time.
    let ordered: Vec<Memory> = ordered.into_iter().filter(|m| valid_at(m, as_of)).collect();
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
