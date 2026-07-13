//! The `resolution` eval profile (EVAL.md §2.2, §3.2): score the entity-
//! resolution stage against the Meridian gold clustering.
//!
//! The gold raw entities are seeded WITHOUT their canonical links (see
//! [`crate::seed::seed_resolution`]); this profile runs the real
//! [`brainiac_pipeline::resolve::resolve_entity`] over every raw entity — with
//! a deterministic ORACLE adjudicator derived from the fixtures' gold clusters,
//! mirroring the `MockProvider` pattern the pipeline pg tests use — then scores
//! the PREDICTED clustering (raw entity → canonical) against the gold with the
//! three §2.2 metrics: B³ P/R/F1, pairwise P/R/F1, and false-merge count.
//!
//! Two gates ride on this (EVAL.md §3.2):
//! - `false_merges == 0` is a HARD gate: a wrong merge silently corrupts the
//!   graph, so it is zero-tolerance (see [`ResolutionReport::gate_failures`]).
//! - `B³ F1 >= baseline` is a SOFT regression gate against a committed baseline
//!   (see [`regression_failures`]), exactly like the retrieval NDCG gate.
//!
//! Like the retrieval profile, the absolute numbers under the deterministic
//! bag-of-tokens embedder are PLUMBING floors, not quality claims — the real
//! adjudicator/embedder bake-off recalibrates the baseline.

use std::collections::HashMap;

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::metrics::{
    b_cubed, false_merge_count, pairwise_prf, Clustering, PrecisionRecallF1,
};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::{ChatRequest, MockProvider};
use brainiac_store::{entities, memories, Store};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::seed::seeding_principal;

/// Serializable precision/recall/F1 (the core type is `Copy` but not `Serde`).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Prf {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

impl From<PrecisionRecallF1> for Prf {
    fn from(p: PrecisionRecallF1) -> Self {
        Self {
            precision: p.precision,
            recall: p.recall,
            f1: p.f1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolutionReport {
    pub fixture_version: String,
    pub embedding_model: String,
    /// Raw entities scored (the clustering universe).
    pub entities: usize,
    /// Distinct gold clusters (merge sets + singletons).
    pub gold_clusters: usize,
    /// Distinct predicted clusters (canonicals + unlinked singletons).
    pub predicted_clusters: usize,
    pub b_cubed: Prf,
    pub pairwise: Prf,
    /// HARD GATE: gold-forbidden pairs the resolver merged anyway. Must be zero.
    pub false_merges: usize,
    /// Total negative (must-not-merge) pairs checked.
    pub negative_pairs: usize,
    /// The offending pairs when `false_merges > 0` (fixture ids), for triage.
    pub false_merge_pairs: Vec<[String; 2]>,
}

impl ResolutionReport {
    /// Evaluate the hard gate (EVAL.md §3.2): false merges are zero-tolerance.
    /// Returns human-readable failures; empty = pass.
    pub fn gate_failures(&self) -> Vec<String> {
        let mut fails = Vec::new();
        if self.false_merges > 0 {
            fails.push(format!(
                "false merges detected ({}): {}",
                self.false_merges,
                self.false_merge_pairs
                    .iter()
                    .map(|p| format!("{}~{}", p[0], p[1]))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        fails
    }
}

/// Committed baseline for the resolution SOFT gates (`results/resolution-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionBaseline {
    pub embedding_model: String,
    pub fixture_version: String,
    pub b_cubed_f1: f64,
    pub pairwise_f1: f64,
}

/// F1 may not regress below the committed baseline by more than this.
const F1_DELTA: f64 = 0.02;

impl ResolutionBaseline {
    pub fn from_report(report: &ResolutionReport) -> Self {
        Self {
            embedding_model: report.embedding_model.clone(),
            fixture_version: report.fixture_version.clone(),
            b_cubed_f1: report.b_cubed.f1,
            pairwise_f1: report.pairwise.f1,
        }
    }
}

/// Compare a run against the committed baseline (mirrors the retrieval gate):
/// B³ and pairwise F1 may not regress past `F1_DELTA`. Empty = pass.
pub fn regression_failures(
    report: &ResolutionReport,
    baseline: &ResolutionBaseline,
) -> Vec<String> {
    let mut fails = Vec::new();
    if report.embedding_model != baseline.embedding_model {
        fails.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate the baseline instead of comparing across embedders",
            report.embedding_model, baseline.embedding_model
        ));
        return fails;
    }
    if report.b_cubed.f1 < baseline.b_cubed_f1 - F1_DELTA {
        fails.push(format!(
            "B³ F1 regressed: {:.3} < baseline {:.3} − {:.2}",
            report.b_cubed.f1, baseline.b_cubed_f1, F1_DELTA
        ));
    }
    if report.pairwise.f1 < baseline.pairwise_f1 - F1_DELTA {
        fails.push(format!(
            "pairwise F1 regressed: {:.3} < baseline {:.3} − {:.2}",
            report.pairwise.f1, baseline.pairwise_f1, F1_DELTA
        ));
    }
    fails
}

/// Gold clustering over ALL raw entities: merge-set members share a cluster;
/// every entity absent from a merge set is its own singleton. Keyed by fixture
/// entity id so it lines up with the predicted clustering and the negative
/// pairs (which are also fixture ids).
pub(crate) fn gold_clustering(fx: &Fixtures) -> Clustering<String> {
    let mut gold: Clustering<String> = HashMap::new();
    let mut next = 0usize;
    for set in &fx.merges.merge_sets {
        let cid = next;
        next += 1;
        for m in &set.members {
            gold.insert(m.clone(), cid);
        }
    }
    for e in &fx.entities.entities {
        if !gold.contains_key(&e.id) {
            gold.insert(e.id.clone(), next);
            next += 1;
        }
    }
    gold
}

/// An ORACLE adjudicator: answers "same real-world thing?" straight from the
/// gold clustering. Deterministic and fixture-derived — the resolution analog
/// of the pipeline tests' `perfect_mock`. Two surface forms are the same iff
/// they belong to the same gold cluster; every negative pair is in a different
/// cluster by construction, so this never green-lights a forbidden merge.
/// Lowercased surface form → gold cluster id. Names are the resolver's handle
/// on an entity (it passes `Name A: <name>` / `Name B: <canonical name>`, and a
/// bootstrapped canonical's name IS a raw surface form). Shared with the
/// pipeline profile's combined mock so both adjudicate from the same ground
/// truth.
pub(crate) fn name_to_cluster(fx: &Fixtures, gold: &Clustering<String>) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for e in &fx.entities.entities {
        if let Some(&c) = gold.get(&e.id) {
            map.entry(e.name.trim().to_lowercase()).or_insert(c);
        }
    }
    map
}

/// Ground-truth answer to "are these two surface forms the same real-world
/// thing?" from the gold clustering — reused by the resolution oracle and the
/// pipeline mock. Unknown names (not in any gold cluster) are treated as
/// distinct, so this never green-lights a forbidden merge.
pub(crate) fn oracle_same(name_to_cluster: &HashMap<String, usize>, a: &str, b: &str) -> bool {
    match (
        name_to_cluster.get(&a.trim().to_lowercase()),
        name_to_cluster.get(&b.trim().to_lowercase()),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn oracle_adjudicator(fx: &Fixtures, gold: &Clustering<String>) -> MockProvider {
    let name_to_cluster = name_to_cluster(fx, gold);
    MockProvider::new(move |req: &ChatRequest| {
        if req.system.contains("adjudicate") {
            let (a, b) = parse_adjudication_names(&req.user);
            let same = oracle_same(&name_to_cluster, &a, &b);
            return format!(r#"{{"same": {same}, "confidence": 0.95}}"#);
        }
        "{}".to_string()
    })
}

/// Pull the two names out of the resolve stage's adjudication prompt body
/// (`Name A: <a>\nName B: <b>`).
pub(crate) fn parse_adjudication_names(user: &str) -> (String, String) {
    let mut a = String::new();
    let mut b = String::new();
    for line in user.lines() {
        if let Some(rest) = line.strip_prefix("Name A:") {
            a = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Name B:") {
            b = rest.trim().to_string();
        }
    }
    (a, b)
}

/// Run the resolution profile end-to-end against a store already seeded by
/// [`crate::seed::seed_resolution`].
pub async fn run(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
) -> Result<ResolutionReport> {
    let org_id = stable_uuid(&fx.org.org);
    let gold = gold_clustering(fx);
    let adjudicator = oracle_adjudicator(fx, &gold);

    let principal = seeding_principal(fx);
    let mut tx = store.scoped_tx(&principal).await?;
    let embedding_version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await?;

    // Resolve every raw entity in fixture order (bootstrapping canonicals as it
    // goes) — the exact stage-4 path a real ingest walks.
    for e in &fx.entities.entities {
        brainiac_pipeline::resolve::resolve_entity(
            &mut tx,
            &adjudicator,
            embedder,
            embedding_version,
            org_id,
            stable_uuid(&e.id),
            &e.name,
            &e.kind,
            &e.aliases,
        )
        .await?;
    }

    // Reconstruct the predicted clustering from the persisted links.
    let uuid_to_fx: HashMap<Uuid, String> = fx
        .entities
        .entities
        .iter()
        .map(|e| (stable_uuid(&e.id), e.id.clone()))
        .collect();
    let links = entities::links_in_org(&mut tx, org_id).await?;
    tx.commit().await?;

    let mut predicted: Clustering<String> = HashMap::new();
    let mut canon_idx: HashMap<Uuid, usize> = HashMap::new();
    let mut next = 0usize;
    for (entity_id, canonical_id) in links {
        let idx = *canon_idx.entry(canonical_id).or_insert_with(|| {
            let i = next;
            next += 1;
            i
        });
        if let Some(fx_id) = uuid_to_fx.get(&entity_id) {
            predicted.insert(fx_id.clone(), idx);
        }
    }
    // Any raw entity the resolver left unlinked (NeedsReview) is a singleton.
    for e in &fx.entities.entities {
        if !predicted.contains_key(&e.id) {
            predicted.insert(e.id.clone(), next);
            next += 1;
        }
    }

    let negative_pairs: Vec<(String, String)> = fx
        .merges
        .negative_pairs
        .iter()
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect();
    let false_merge_pairs: Vec<[String; 2]> = negative_pairs
        .iter()
        .filter(|(a, b)| match (predicted.get(a), predicted.get(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        })
        .map(|(a, b)| [a.clone(), b.clone()])
        .collect();

    let gold_clusters = gold
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len();
    let predicted_clusters = predicted
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len();

    Ok(ResolutionReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        entities: fx.entities.entities.len(),
        gold_clusters,
        predicted_clusters,
        b_cubed: b_cubed(&predicted, &gold).into(),
        pairwise: pairwise_prf(&predicted, &gold).into(),
        false_merges: false_merge_count(&predicted, &negative_pairs),
        negative_pairs: negative_pairs.len(),
        false_merge_pairs,
    })
}
