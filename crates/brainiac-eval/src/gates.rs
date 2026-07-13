//! CI regression gates (EVAL.md §3.2) — baseline-relative thresholds.
//!
//! The hard gates (RLS leaks = 0, superseded-in-top3 = 0) live on
//! [`crate::report::RetrievalReport::gate_failures`] and are unconditional.
//! This module adds the SOFT gates: scores may not regress below a committed
//! baseline by more than the §3.2 deltas. The baseline is a small JSON file
//! (`results/baseline.json`) written by `brainiac eval --write-baseline` and
//! recalibrated deliberately — never silently — when the corpus or embedder
//! changes.
//!
//! §3.2 rows not yet wired here: extraction F1 / B³ (pipeline profile,
//! nightly per-provider) and retrieval p95 (needs reference hardware).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::report::RetrievalReport;

/// §3.2 deltas, in NDCG points (1 pt = 0.01).
const OVERALL_NDCG_DELTA: f64 = 0.01;
const STRATUM_NDCG_DELTA: f64 = 0.02;
const TEMPORAL_RANK1_DELTA: f64 = 0.02;
/// Thesis proxy: graph expansion must keep the cross-team stratum at least
/// this far above pure-semantic retrieval. §3.2 phrases it as "flat-vector
/// baseline + 5 pts"; the flat baseline isn't recomputed per run, so the
/// semantic stratum stands in for it.
const CROSS_TEAM_MARGIN_OVER_SEMANTIC: f64 = 0.05;

/// A baseline written before the reranker axis existed carries no `reranker`
/// field; it describes the no-reranker path, so an absent field reads as
/// `"none"` and the committed `results/baseline.json` stays valid unchanged.
fn default_reranker() -> String {
    "none".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Which embedder produced this baseline; a mismatch is a hard error —
    /// comparing scores across embedders is meaningless.
    pub embedding_model: String,
    /// Which stage-5 reranker produced this baseline (`"none"` = no reranker).
    /// Like the embedder, a mismatch is a hard error: rerankers reorder the
    /// result set, so scores are only comparable within one reranker. Absent in
    /// pre-reranker baselines → defaults to `"none"` (see [`default_reranker`]).
    #[serde(default = "default_reranker")]
    pub reranker: String,
    pub fixture_version: String,
    pub overall_ndcg_at_10: f64,
    pub per_stratum_ndcg_at_10: BTreeMap<String, f64>,
    pub temporal_rank1_accuracy: f64,
}

impl Baseline {
    pub fn from_report(report: &RetrievalReport) -> anyhow::Result<Self> {
        Ok(Self {
            embedding_model: report.embedding_model.clone(),
            reranker: report.reranker.clone(),
            fixture_version: report.fixture_version.clone(),
            overall_ndcg_at_10: report
                .overall
                .ndcg_at_10
                .ok_or_else(|| anyhow::anyhow!("report has no graded queries"))?,
            per_stratum_ndcg_at_10: report
                .per_stratum
                .iter()
                .filter_map(|(k, v)| v.ndcg_at_10.map(|n| (k.clone(), n)))
                .collect(),
            temporal_rank1_accuracy: report.temporal_rank1_accuracy,
        })
    }
}

/// Compare a run against the committed baseline. Returns human-readable
/// breaches; empty = pass. Strata present in the baseline but absent from
/// the run (or vice versa) are breaches too — silent scope shrink is how
/// regressions hide.
pub fn regression_failures(report: &RetrievalReport, baseline: &Baseline) -> Vec<String> {
    let mut fails = Vec::new();

    if report.embedding_model != baseline.embedding_model {
        fails.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate the baseline instead of comparing across embedders",
            report.embedding_model, baseline.embedding_model
        ));
        return fails;
    }

    // A reranker mismatch is refused exactly like the embedder mismatch: a
    // reranker reorders the surviving candidates, so NDCG/MRR under `lexical`
    // and under `none` measure different rankings — comparing them is
    // meaningless. Recalibrate a per-reranker baseline instead.
    if report.reranker != baseline.reranker {
        fails.push(format!(
            "reranker mismatch: run={} baseline={} — recalibrate the baseline instead of comparing across rerankers",
            report.reranker, baseline.reranker
        ));
        return fails;
    }

    match report.overall.ndcg_at_10 {
        Some(n) if n >= baseline.overall_ndcg_at_10 - OVERALL_NDCG_DELTA => {}
        Some(n) => fails.push(format!(
            "overall NDCG@10 regressed: {:.3} < baseline {:.3} − {:.2}",
            n, baseline.overall_ndcg_at_10, OVERALL_NDCG_DELTA
        )),
        None => fails.push("run produced no graded queries".into()),
    }

    for (stratum, floor) in &baseline.per_stratum_ndcg_at_10 {
        match report.per_stratum.get(stratum).and_then(|s| s.ndcg_at_10) {
            Some(n) if n >= floor - STRATUM_NDCG_DELTA => {}
            Some(n) => fails.push(format!(
                "stratum `{stratum}` NDCG@10 regressed: {:.3} < baseline {:.3} − {:.2}",
                n, floor, STRATUM_NDCG_DELTA
            )),
            None => fails.push(format!(
                "stratum `{stratum}` present in baseline but missing from the run"
            )),
        }
    }

    if report.temporal_rank1_accuracy < baseline.temporal_rank1_accuracy - TEMPORAL_RANK1_DELTA {
        fails.push(format!(
            "temporal rank-1 accuracy regressed: {:.3} < baseline {:.3} − {:.2}",
            report.temporal_rank1_accuracy, baseline.temporal_rank1_accuracy, TEMPORAL_RANK1_DELTA
        ));
    }

    // Thesis check: the cross-team stratum must clear pure semantic by the
    // §3.2 margin — that's the graph expansion earning its keep.
    if let (Some(cross), Some(semantic)) = (
        report
            .per_stratum
            .get("cross_team_graph")
            .and_then(|s| s.ndcg_at_10),
        report
            .per_stratum
            .get("semantic")
            .and_then(|s| s.ndcg_at_10),
    ) {
        if cross < semantic + CROSS_TEAM_MARGIN_OVER_SEMANTIC {
            fails.push(format!(
                "thesis check failed: cross-team {:.3} < semantic {:.3} + {:.2} — graph expansion is not adding value",
                cross, semantic, CROSS_TEAM_MARGIN_OVER_SEMANTIC
            ));
        }
    }

    fails
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{LatencyBreakdown, LatencyStats, StratumScores};

    fn scores(n: Option<f64>) -> StratumScores {
        StratumScores {
            queries: 10,
            ndcg_at_10: n,
            mrr: 0.5,
            recall_at_5: Some(0.5),
        }
    }

    fn report(overall: f64, strata: &[(&str, f64)], temporal: f64) -> RetrievalReport {
        RetrievalReport {
            fixture_version: "v1".into(),
            embedding_model: "deterministic-bow-v1".into(),
            reranker: "none".into(),
            overall: scores(Some(overall)),
            per_stratum: strata
                .iter()
                .map(|(k, v)| (k.to_string(), scores(Some(*v))))
                .collect(),
            temporal_rank1_accuracy: temporal,
            superseded_in_top3: 0,
            negative_empty_rate: 0.0,
            rls_leaks: vec![],
            queries_run: 83,
            latency: LatencyBreakdown {
                overall: LatencyStats::from_samples(vec![]),
                per_stratum: BTreeMap::new(),
                per_suite: BTreeMap::new(),
            },
        }
    }

    fn baseline() -> Baseline {
        Baseline::from_report(&report(
            0.685,
            &[("semantic", 0.422), ("cross_team_graph", 0.772)],
            0.786,
        ))
        .expect("baseline")
    }

    #[test]
    fn identical_run_passes() {
        let r = report(
            0.685,
            &[("semantic", 0.422), ("cross_team_graph", 0.772)],
            0.786,
        );
        assert!(regression_failures(&r, &baseline()).is_empty());
    }

    #[test]
    fn within_delta_passes_beyond_fails() {
        let ok = report(
            0.676,
            &[("semantic", 0.403), ("cross_team_graph", 0.753)],
            0.767,
        );
        assert!(regression_failures(&ok, &baseline()).is_empty());
        let bad = report(
            0.67,
            &[("semantic", 0.422), ("cross_team_graph", 0.772)],
            0.786,
        );
        let fails = regression_failures(&bad, &baseline());
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("overall NDCG@10 regressed"));
    }

    #[test]
    fn missing_stratum_is_a_breach() {
        let r = report(0.685, &[("semantic", 0.422)], 0.786);
        let fails = regression_failures(&r, &baseline());
        assert!(fails
            .iter()
            .any(|f| f.contains("cross_team_graph") && f.contains("missing")));
    }

    #[test]
    fn embedder_mismatch_short_circuits() {
        let mut r = report(0.9, &[("semantic", 0.9), ("cross_team_graph", 0.99)], 0.9);
        r.embedding_model = "qwen:text-embedding-v4".into();
        let fails = regression_failures(&r, &baseline());
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("embedder mismatch"));
    }

    #[test]
    fn reranker_mismatch_short_circuits() {
        let mut r = report(0.9, &[("semantic", 0.9), ("cross_team_graph", 0.99)], 0.9);
        r.reranker = "lexical-overlap-v1".into();
        let fails = regression_failures(&r, &baseline());
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("reranker mismatch"));
    }

    #[test]
    fn pre_reranker_baseline_json_defaults_to_none() {
        // A baseline written before the reranker axis has no `reranker` key; it
        // must still parse and read as the no-reranker path.
        let json = r#"{
            "embedding_model": "deterministic-bow-v1",
            "fixture_version": "v1",
            "overall_ndcg_at_10": 0.685,
            "per_stratum_ndcg_at_10": {"semantic": 0.422, "cross_team_graph": 0.772},
            "temporal_rank1_accuracy": 0.786
        }"#;
        let b: Baseline = serde_json::from_str(json).expect("legacy baseline parses");
        assert_eq!(b.reranker, "none");
        let r = report(
            0.685,
            &[("semantic", 0.422), ("cross_team_graph", 0.772)],
            0.786,
        );
        assert!(regression_failures(&r, &b).is_empty());
    }

    #[test]
    fn thesis_check_fires_when_graph_stops_helping() {
        let r = report(
            0.685,
            &[("semantic", 0.75), ("cross_team_graph", 0.772)],
            0.786,
        );
        let fails = regression_failures(&r, &baseline());
        assert!(fails.iter().any(|f| f.contains("thesis check failed")));
    }
}
