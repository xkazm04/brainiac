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

/// Wall-clock latency of the retrieval `search` call, in milliseconds
/// (EVAL.md §2.5/§3.1/§3.2). INFORMATIONAL ONLY: these numbers depend on the
/// host, the pool warmth and the Postgres cache, so — like §3.2's retrieval
/// p95 row, which gates.rs:12 flags as "needs reference hardware" — they are
/// recorded and diffed but are NOT a regression gate. Treat a shift as a
/// signal to investigate on fixed hardware, never as a CI pass/fail.
#[derive(Debug, Clone, Serialize)]
pub struct LatencyStats {
    /// Number of retrieval calls this summarizes.
    pub count: usize,
    pub mean_ms: f64,
    /// Median (50th percentile), nearest-rank.
    pub p50_ms: f64,
    /// 95th percentile, nearest-rank.
    pub p95_ms: f64,
}

impl LatencyStats {
    /// Summarize a set of per-call millisecond samples. Nearest-rank
    /// percentiles (no interpolation) — stable and obvious for the small
    /// per-config sample counts here. An empty sample set yields all-zero.
    pub fn from_samples(mut samples: Vec<f64>) -> Self {
        let count = samples.len();
        if count == 0 {
            return Self {
                count: 0,
                mean_ms: 0.0,
                p50_ms: 0.0,
                p95_ms: 0.0,
            };
        }
        let mean_ms = samples.iter().sum::<f64>() / count as f64;
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let percentile = |q: f64| -> f64 {
            // Nearest-rank: rank = ceil(q * n), 1-based, clamped to [1, n].
            let rank = (q * count as f64).ceil().max(1.0) as usize;
            samples[rank.min(count) - 1]
        };
        Self {
            count,
            mean_ms,
            p50_ms: percentile(0.50),
            p95_ms: percentile(0.95),
        }
    }
}

/// Per-config retrieval latency (EVAL.md §2.5). INFORMATIONAL, never a gate —
/// see [`LatencyStats`]. `overall` spans every retrieval call across the
/// qa/temporal/leak suites; the breakdowns slice the same samples by QA
/// stratum and by suite.
#[derive(Debug, Clone, Serialize)]
pub struct LatencyBreakdown {
    pub overall: LatencyStats,
    pub per_stratum: BTreeMap<String, LatencyStats>,
    pub per_suite: BTreeMap<String, LatencyStats>,
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
    /// Retrieval wall-clock latency, overall + per stratum + per suite.
    /// INFORMATIONAL: recorded and diffed, but NOT a regression gate — the
    /// numbers are host-dependent (see [`LatencyStats`] and gates.rs:12).
    pub latency: LatencyBreakdown,
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
    /// Wall-clock latency of this query's retrieval `search` call, in
    /// milliseconds. INFORMATIONAL (see [`LatencyStats`]) — never gated.
    pub latency_ms: f64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_empty_is_zero() {
        let s = LatencyStats::from_samples(vec![]);
        assert_eq!(s.count, 0);
        assert_eq!(s.mean_ms, 0.0);
        assert_eq!(s.p50_ms, 0.0);
        assert_eq!(s.p95_ms, 0.0);
    }

    #[test]
    fn latency_nearest_rank_percentiles() {
        // 1..=10; unsorted input must be handled.
        let s = LatencyStats::from_samples(vec![10.0, 1.0, 7.0, 3.0, 9.0, 2.0, 8.0, 4.0, 6.0, 5.0]);
        assert_eq!(s.count, 10);
        assert!((s.mean_ms - 5.5).abs() < 1e-9);
        // Nearest-rank: p50 → ceil(0.5*10)=5th → value 5; p95 → ceil(0.95*10)=10th → 10.
        assert_eq!(s.p50_ms, 5.0);
        assert_eq!(s.p95_ms, 10.0);
    }

    #[test]
    fn latency_single_sample() {
        let s = LatencyStats::from_samples(vec![42.0]);
        assert_eq!(s.count, 1);
        assert_eq!(s.p50_ms, 42.0);
        assert_eq!(s.p95_ms, 42.0);
    }
}
