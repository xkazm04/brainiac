//! Result shapes — serialized to JSON and append-committed to
//! `results/history/` so score trajectories stay diffable in Git.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct StratumScores {
    pub queries: usize,
    pub ndcg_at_10: Option<f64>,
    pub mrr: f64,
    pub recall_at_5: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalReport {
    pub fixture_version: String,
    pub embedding_model: String,
    /// Stage-5 reranker that produced this run: the reranker's `model_name`, or
    /// `"none"` when retrieval ran with no reranker (the byte-identical
    /// pre-stage-5 path). Tagged alongside `embedding_model` because — exactly
    /// like the embedder — scores are only comparable within one reranker; the
    /// regression gate refuses a cross-reranker baseline comparison.
    pub reranker: String,
    pub overall: StratumScores,
    pub per_stratum: BTreeMap<String, StratumScores>,
    /// Temporal suite: fraction of as-of cases with the correct memory at rank 1.
    pub temporal_rank1_accuracy: f64,
    /// Temporal suite: superseded memories that appeared in top-3 of
    /// current-time queries (forbidden_top3 violations).
    pub superseded_in_top3: usize,
    /// Negative stratum: queries that returned zero hits (higher = better
    /// refusal behavior for the deterministic embedder baseline).
    pub negative_empty_rate: f64,
    /// HARD GATE: forbidden memories that surfaced for an unauthorized asker,
    /// at any rank. Must be zero; anything else is a build failure.
    pub rls_leaks: Vec<String>,
    pub queries_run: usize,
}

// ── per-query drill-down (the aggregate says THAT a score moved; this says
// WHICH queries moved it) ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DiagExpected {
    /// Fixture memory id.
    pub memory: String,
    pub grade: u8,
    /// 1-based rank where it actually surfaced; None = missing from results.
    pub rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagHit {
    pub rank: usize,
    /// Fixture memory id when resolvable, else the raw UUID.
    pub memory: String,
    /// Truncated content so the file reads without a DB at hand.
    pub content: String,
    pub score: f64,
    pub via_graph: bool,
    /// Relevance grade from the gold set; None = ungraded hit.
    pub grade: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryDiagnostic {
    /// qa | temporal | leak
    pub suite: String,
    pub id: String,
    pub stratum: Option<String>,
    pub asker: Option<String>,
    pub query: String,
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    pub ndcg_at_10: Option<f64>,
    pub reciprocal_rank: Option<f64>,
    pub recall_at_5: Option<f64>,
    pub expected: Vec<DiagExpected>,
    pub got: Vec<DiagHit>,
    /// Human-readable rule breaches (leaked memory, superseded in top-3,
    /// temporal rank-1 miss, …). Empty + pass=false = soft miss only.
    pub violations: Vec<String>,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalDiagnostics {
    pub fixture_version: String,
    pub embedding_model: String,
    /// Stage-5 reranker tag (mirrors [`RetrievalReport::reranker`]).
    pub reranker: String,
    pub queries: Vec<QueryDiagnostic>,
}

impl RetrievalDiagnostics {
    /// Failures first (the reason this artifact exists), then by suite/id.
    pub fn sort_failures_first(&mut self) {
        self.queries
            .sort_by_key(|q| (q.pass, q.suite.clone(), q.id.clone()));
    }
}

impl RetrievalReport {
    /// Evaluate the hard gates (EVAL.md §3.2). Returns human-readable
    /// failures; empty = pass.
    pub fn gate_failures(&self) -> Vec<String> {
        let mut fails = Vec::new();
        if !self.rls_leaks.is_empty() {
            fails.push(format!(
                "RLS leaks detected ({}): {}",
                self.rls_leaks.len(),
                self.rls_leaks.join(", ")
            ));
        }
        if self.superseded_in_top3 > 0 {
            fails.push(format!(
                "superseded memories in top-3 of current-time queries: {}",
                self.superseded_in_top3
            ));
        }
        fails
    }
}
