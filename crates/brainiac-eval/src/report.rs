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
