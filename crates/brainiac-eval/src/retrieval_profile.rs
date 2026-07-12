//! The `retrieval` eval profile (EVAL.md §2.4, §2.5, §3): gold memories are
//! already seeded; run every QA query under its asker's principal, score
//! rankings against graded relevance, check temporal correctness, and treat
//! any RLS leak as a hard failure.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::metrics::{ndcg_at_k, recall_at_k, reciprocal_rank};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_store::retrieval::{search, RetrievalRequest};
use brainiac_store::Store;
use uuid::Uuid;

use crate::report::{RetrievalReport, StratumScores};
use crate::seed::principal_for_user;

const K: usize = 10;

struct PerQuery {
    stratum: String,
    ndcg: Option<f64>,
    rr: f64,
    recall5: Option<f64>,
    empty: bool,
}

pub async fn run(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
    embedding_version: i32,
) -> Result<RetrievalReport> {
    let mut per_query: Vec<PerQuery> = Vec::new();
    let mut leaks: Vec<String> = Vec::new();
    let mut temporal_hits = 0usize;
    let mut temporal_total = 0usize;
    let mut superseded_in_top3 = 0usize;

    // ── QA suite ─────────────────────────────────────────────────────────
    for q in &fx.qa.queries {
        let principal = principal_for_user(fx, &q.asking_as.user)
            .ok_or_else(|| anyhow::anyhow!("unknown asker {}", q.asking_as.user))?;
        let mut tx = store.scoped_tx(&principal).await?;
        let hits = search(
            &mut tx,
            embedder,
            embedding_version,
            &RetrievalRequest {
                query: q.query.clone(),
                k: K,
                as_of: q.as_of,
                filters: Default::default(),
            },
        )
        .await?;
        drop(tx);

        let ranked: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
        let grades: HashMap<Uuid, u8> = q
            .relevant
            .iter()
            .map(|g| (stable_uuid(&g.memory), g.grade))
            .collect();

        // Temporal sub-checks folded into QA items.
        for forbidden in &q.forbidden_top3 {
            let fid = stable_uuid(forbidden);
            if ranked.iter().take(3).any(|id| *id == fid) {
                superseded_in_top3 += 1;
            }
        }

        per_query.push(PerQuery {
            stratum: q.stratum.clone(),
            ndcg: ndcg_at_k(&ranked, &grades, K),
            rr: reciprocal_rank(&ranked, &grades),
            recall5: recall_at_k(&ranked, &grades, 5),
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
        let hits = search(
            &mut tx,
            embedder,
            embedding_version,
            &RetrievalRequest {
                query: t.question.clone(),
                k: K,
                as_of: Some(t.as_of),
                filters: Default::default(),
            },
        )
        .await?;
        drop(tx);
        temporal_total += 1;
        if hits.first().map(|h| h.memory.id) == Some(stable_uuid(&t.expected_memory)) {
            temporal_hits += 1;
        }
    }

    // ── leak suite (hard invariant) ──────────────────────────────────────
    for q in &fx.leak.queries {
        let principal = principal_for_user(fx, &q.asking_as.user)
            .ok_or_else(|| anyhow::anyhow!("unknown asker {}", q.asking_as.user))?;
        let mut tx = store.scoped_tx(&principal).await?;
        let hits = search(
            &mut tx,
            embedder,
            embedding_version,
            &RetrievalRequest {
                query: q.query.clone(),
                k: 50, // deep k: a leak at ANY rank is a failure
                as_of: None,
                filters: Default::default(),
            },
        )
        .await?;
        drop(tx);
        for forbidden in &q.forbidden {
            let fid = stable_uuid(forbidden);
            if hits.iter().any(|h| h.memory.id == fid) {
                leaks.push(format!("{}:{forbidden}", q.id));
            }
        }
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

    Ok(RetrievalReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
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
    })
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
