//! The `retrieval` eval profile (EVAL.md §2.4, §2.5, §3): gold memories are
//! already seeded; run every QA query under its asker's principal, score
//! rankings against graded relevance, check temporal correctness, and treat
//! any RLS leak as a hard failure.
//!
//! Alongside the aggregate report, the run collects a per-query diagnostic
//! (expected vs got, with fixture ids and content) so a score regression is
//! attributable to specific queries without re-running anything.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::metrics::{ndcg_at_k, recall_at_k, reciprocal_rank};
use brainiac_core::rerank::Reranker;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_store::retrieval::{search_reranked, RetrievalHit, RetrievalRequest};
use brainiac_store::Store;
use uuid::Uuid;

use crate::report::{
    DiagExpected, DiagHit, LatencyBreakdown, LatencyStats, QueryDiagnostic, RetrievalDiagnostics,
    RetrievalReport, StratumScores,
};
use crate::seed::principal_for_user;

const K: usize = 10;
const DIAG_CONTENT_CHARS: usize = 140;

struct PerQuery {
    stratum: String,
    ndcg: Option<f64>,
    rr: f64,
    recall5: Option<f64>,
    empty: bool,
}

/// Fixture-id lookup for hits: uuid → fixture memory id.
fn fixture_id_map(fx: &Fixtures) -> HashMap<Uuid, String> {
    fx.memories
        .memories
        .iter()
        .map(|m| (stable_uuid(&m.id), m.id.clone()))
        .collect()
}

fn diag_hits(
    hits: &[RetrievalHit],
    names: &HashMap<Uuid, String>,
    grades: &HashMap<Uuid, u8>,
) -> Vec<DiagHit> {
    hits.iter()
        .enumerate()
        .map(|(i, h)| DiagHit {
            rank: i + 1,
            memory: names
                .get(&h.memory.id)
                .cloned()
                .unwrap_or_else(|| h.memory.id.to_string()),
            content: h.memory.content.chars().take(DIAG_CONTENT_CHARS).collect(),
            score: h.score,
            via_graph: h.via_graph,
            grade: grades.get(&h.memory.id).copied(),
        })
        .collect()
}

pub async fn run(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
    reranker: Option<&dyn Reranker>,
    embedding_version: i32,
) -> Result<(RetrievalReport, RetrievalDiagnostics)> {
    // Tag for the report: the reranker's model name, or "none" when the run
    // uses the byte-identical pre-stage-5 path.
    let reranker_tag = reranker
        .map(|r| r.model_name().to_string())
        .unwrap_or_else(|| "none".into());
    let mut per_query: Vec<PerQuery> = Vec::new();
    let mut diagnostics: Vec<QueryDiagnostic> = Vec::new();
    let mut leaks: Vec<String> = Vec::new();
    let mut temporal_hits = 0usize;
    let mut temporal_total = 0usize;
    let mut superseded_in_top3 = 0usize;
    let names = fixture_id_map(fx);

    // Per-query retrieval latency samples (Direction 2, EVAL.md §2.5).
    // INFORMATIONAL only — see `LatencyStats`; never a regression gate.
    let mut qa_latencies: Vec<(String, f64)> = Vec::new();
    let mut temporal_latencies: Vec<f64> = Vec::new();
    let mut leak_latencies: Vec<f64> = Vec::new();

    // The forbidden-at-current-time set, DERIVED from the seeded supersession
    // graph rather than trusted to annotation. asof.yaml declares a two-part
    // metric — "exact hit at rank 1; superseded memories absent from top-3 on
    // current-time queries" — but the second half was delegated entirely to
    // `forbidden_top3` strings (3 of them across 54 QA queries, against 6 gold
    // memories that actually carry `superseded_by`). Add a supersession pair to
    // the fixtures without also editing forbidden_top3 and the regression was
    // invisible: a gate that passes when quality regressed. The seeder writes
    // superseded_by/valid_to/status for these, so derive the set from the data.
    let superseded_ids: Vec<(&str, Uuid)> = fx
        .memories
        .memories
        .iter()
        .filter(|m| m.superseded_by.is_some())
        .map(|m| (m.id.as_str(), stable_uuid(&m.id)))
        .collect();

    // ── QA suite ─────────────────────────────────────────────────────────
    for q in &fx.qa.queries {
        let principal = principal_for_user(fx, &q.asking_as.user)
            .ok_or_else(|| anyhow::anyhow!("unknown asker {}", q.asking_as.user))?;
        let mut tx = store.scoped_tx(&principal).await?;
        let t0 = Instant::now();
        let hits = search_reranked(
            &mut tx,
            store.pool(),
            embedder,
            reranker,
            embedding_version,
            &RetrievalRequest {
                query: q.query.clone(),
                k: K,
                as_of: q.as_of,
                filters: Default::default(),
            },
        )
        .await?;
        let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
        drop(tx);
        qa_latencies.push((q.stratum.clone(), latency_ms));

        let ranked: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
        let grades: HashMap<Uuid, u8> = q
            .relevant
            .iter()
            .map(|g| (stable_uuid(&g.memory), g.grade))
            .collect();

        let mut violations: Vec<String> = Vec::new();
        // Temporal sub-checks folded into QA items.
        for forbidden in &q.forbidden_top3 {
            let fid = stable_uuid(forbidden);
            if let Some(pos) = ranked.iter().take(3).position(|id| *id == fid) {
                superseded_in_top3 += 1;
                violations.push(format!(
                    "superseded memory {forbidden} in top-3 (rank {})",
                    pos + 1
                ));
            }
        }
        // The derived half: on a CURRENT-time query (no as_of), no memory that the
        // supersession graph says is superseded may sit in the top-3 — regardless
        // of whether a fixture author remembered to annotate it. Skip ids the
        // fixture already lists so a hit is never double-counted.
        if q.as_of.is_none() {
            for (fx_id, sid) in &superseded_ids {
                if q.forbidden_top3.iter().any(|f| f == fx_id) {
                    continue;
                }
                if let Some(pos) = ranked.iter().take(3).position(|id| id == sid) {
                    superseded_in_top3 += 1;
                    violations.push(format!(
                        "superseded memory {fx_id} in top-3 (rank {}) of a current-time query \
                         [derived from the supersession graph]",
                        pos + 1
                    ));
                }
            }
        }
        let is_negative = q.stratum == "negative";
        if is_negative && !ranked.is_empty() {
            violations.push(format!(
                "negative query returned {} hits (expected none)",
                ranked.len()
            ));
        }
        if !is_negative && !q.relevant.is_empty() && ranked.is_empty() {
            violations.push("graded query returned zero hits".into());
        }

        let ndcg = ndcg_at_k(&ranked, &grades, K);
        let rr = reciprocal_rank(&ranked, &grades);
        let recall5 = recall_at_k(&ranked, &grades, 5);
        diagnostics.push(QueryDiagnostic {
            suite: "qa".into(),
            id: q.id.clone(),
            stratum: Some(q.stratum.clone()),
            asker: Some(q.asking_as.user.clone()),
            query: q.query.clone(),
            as_of: q.as_of,
            ndcg_at_10: ndcg,
            reciprocal_rank: if q.relevant.is_empty() {
                None
            } else {
                Some(rr)
            },
            recall_at_5: recall5,
            latency_ms,
            expected: q
                .relevant
                .iter()
                .map(|g| DiagExpected {
                    memory: g.memory.clone(),
                    grade: g.grade,
                    rank: ranked
                        .iter()
                        .position(|id| *id == stable_uuid(&g.memory))
                        .map(|p| p + 1),
                })
                .collect(),
            got: diag_hits(&hits, &names, &grades),
            pass: violations.is_empty(),
            violations,
        });
        per_query.push(PerQuery {
            stratum: q.stratum.clone(),
            ndcg,
            rr,
            recall5,
            empty: ranked.is_empty(),
        });
    }

    // ── temporal as-of suite (§2.4): rank-1 exactness ────────────────────
    for t in &fx.temporal.cases {
        // As-of questions run as a maximally-privileged org member for the
        // owning team of the expected memory — the suite measures temporal
        // logic, not visibility (leak.yaml owns that).
        let expected = fx
            .memories
            .memories
            .iter()
            .find(|m| m.id == t.expected_memory)
            .ok_or_else(|| anyhow::anyhow!("temporal expected memory missing"))?;
        let asker = fx
            .org
            .users
            .iter()
            .find(|u| u.teams.contains(&expected.team))
            .ok_or_else(|| anyhow::anyhow!("no user in team {}", expected.team))?;
        let principal = principal_for_user(fx, &asker.id).expect("asker principal");
        let mut tx = store.scoped_tx(&principal).await?;
        let t0 = Instant::now();
        let hits = search_reranked(
            &mut tx,
            store.pool(),
            embedder,
            reranker,
            embedding_version,
            &RetrievalRequest {
                query: t.question.clone(),
                k: K,
                as_of: Some(t.as_of),
                filters: Default::default(),
            },
        )
        .await?;
        let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
        drop(tx);
        temporal_latencies.push(latency_ms);
        temporal_total += 1;
        let expected_uuid = stable_uuid(&t.expected_memory);
        let rank1_hit = hits.first().map(|h| h.memory.id) == Some(expected_uuid);
        if rank1_hit {
            temporal_hits += 1;
        }
        let expected_rank = hits
            .iter()
            .position(|h| h.memory.id == expected_uuid)
            .map(|p| p + 1);
        diagnostics.push(QueryDiagnostic {
            suite: "temporal".into(),
            id: t.id.clone(),
            stratum: None,
            asker: Some(asker.id.clone()),
            query: t.question.clone(),
            as_of: Some(t.as_of),
            ndcg_at_10: None,
            reciprocal_rank: None,
            recall_at_5: None,
            latency_ms,
            expected: vec![DiagExpected {
                memory: t.expected_memory.clone(),
                grade: 3,
                rank: expected_rank,
            }],
            got: diag_hits(&hits, &names, &HashMap::new()),
            violations: if rank1_hit {
                vec![]
            } else {
                vec![match expected_rank {
                    Some(r) => format!(
                        "expected {} at rank 1, found at rank {r}",
                        t.expected_memory
                    ),
                    None => format!(
                        "expected {} at rank 1, absent from results",
                        t.expected_memory
                    ),
                }]
            },
            pass: rank1_hit,
        });
    }

    // ── leak suite (hard invariant) ──────────────────────────────────────
    for q in &fx.leak.queries {
        let principal = principal_for_user(fx, &q.asking_as.user)
            .ok_or_else(|| anyhow::anyhow!("unknown asker {}", q.asking_as.user))?;
        let mut tx = store.scoped_tx(&principal).await?;
        let t0 = Instant::now();
        let hits = search_reranked(
            &mut tx,
            store.pool(),
            embedder,
            reranker,
            embedding_version,
            &RetrievalRequest {
                query: q.query.clone(),
                k: 50, // deep k: a leak at ANY rank is a failure
                as_of: None,
                filters: Default::default(),
            },
        )
        .await?;
        let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
        drop(tx);
        leak_latencies.push(latency_ms);
        let mut violations: Vec<String> = Vec::new();
        for forbidden in &q.forbidden {
            let fid = stable_uuid(forbidden);
            if let Some(pos) = hits.iter().position(|h| h.memory.id == fid) {
                leaks.push(format!("{}:{forbidden}", q.id));
                violations.push(format!(
                    "forbidden memory {forbidden} surfaced at rank {}",
                    pos + 1
                ));
            }
        }
        diagnostics.push(QueryDiagnostic {
            suite: "leak".into(),
            id: q.id.clone(),
            stratum: None,
            asker: Some(q.asking_as.user.clone()),
            query: q.query.clone(),
            as_of: None,
            ndcg_at_10: None,
            reciprocal_rank: None,
            recall_at_5: None,
            latency_ms,
            expected: q
                .forbidden
                .iter()
                .map(|f| DiagExpected {
                    memory: f.clone(),
                    grade: 0,
                    rank: hits
                        .iter()
                        .position(|h| h.memory.id == stable_uuid(f))
                        .map(|p| p + 1),
                })
                .collect(),
            got: diag_hits(&hits, &names, &HashMap::new()),
            pass: violations.is_empty(),
            violations,
        });
    }

    // ── aggregate ────────────────────────────────────────────────────────
    let negative: Vec<&PerQuery> = per_query
        .iter()
        .filter(|p| p.stratum == "negative")
        .collect();
    let negative_empty_rate = if negative.is_empty() {
        1.0
    } else {
        negative.iter().filter(|p| p.empty).count() as f64 / negative.len() as f64
    };

    let mut per_stratum: BTreeMap<String, StratumScores> = BTreeMap::new();
    let strata: std::collections::HashSet<String> =
        per_query.iter().map(|p| p.stratum.clone()).collect();
    for s in strata {
        let qs: Vec<&PerQuery> = per_query.iter().filter(|p| p.stratum == s).collect();
        per_stratum.insert(s, aggregate(&qs));
    }
    let all: Vec<&PerQuery> = per_query.iter().collect();

    // ── latency breakdown (Direction 2, informational) ───────────────────
    // `overall` spans every retrieval call across all three suites; the slices
    // group the SAME samples by QA stratum and by suite.
    let mut per_stratum_latency: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (s, ms) in &qa_latencies {
        per_stratum_latency.entry(s.clone()).or_default().push(*ms);
    }
    let qa_ms: Vec<f64> = qa_latencies.iter().map(|(_, ms)| *ms).collect();
    let overall_ms: Vec<f64> = qa_ms
        .iter()
        .chain(temporal_latencies.iter())
        .chain(leak_latencies.iter())
        .copied()
        .collect();
    let latency = LatencyBreakdown {
        overall: LatencyStats::from_samples(overall_ms),
        per_stratum: per_stratum_latency
            .into_iter()
            .map(|(k, v)| (k, LatencyStats::from_samples(v)))
            .collect(),
        per_suite: [
            ("qa", qa_ms),
            ("temporal", temporal_latencies),
            ("leak", leak_latencies),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), LatencyStats::from_samples(v)))
        .collect(),
    };

    let report = RetrievalReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        reranker: reranker_tag,
        overall: aggregate(&all),
        per_stratum,
        temporal_rank1_accuracy: if temporal_total == 0 {
            0.0
        } else {
            temporal_hits as f64 / temporal_total as f64
        },
        superseded_in_top3,
        negative_empty_rate,
        rls_leaks: leaks,
        queries_run: per_query.len() + temporal_total + fx.leak.queries.len(),
        latency,
    };
    let mut diagnostics = RetrievalDiagnostics {
        fixture_version: report.fixture_version.clone(),
        embedding_model: report.embedding_model.clone(),
        reranker: report.reranker.clone(),
        queries: diagnostics,
    };
    diagnostics.sort_failures_first();
    Ok((report, diagnostics))
}

fn aggregate(queries: &[&PerQuery]) -> StratumScores {
    let ndcgs: Vec<f64> = queries.iter().filter_map(|p| p.ndcg).collect();
    let recalls: Vec<f64> = queries.iter().filter_map(|p| p.recall5).collect();
    // MRR averages over graded queries only (negative stratum has no
    // relevant items, so RR is undefined there — excluded).
    let graded: Vec<&&PerQuery> = queries.iter().filter(|p| p.ndcg.is_some()).collect();
    StratumScores {
        queries: queries.len(),
        ndcg_at_10: mean(&ndcgs),
        mrr: mean(&graded.iter().map(|p| p.rr).collect::<Vec<_>>()).unwrap_or(0.0),
        recall_at_5: mean(&recalls),
    }
}

fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}
