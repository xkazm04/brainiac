//! The `pipeline` (P5) eval profile (EVAL.md §2.1, §3): score the REAL worker
//! extraction chain against the Meridian gold.
//!
//! Unlike the retrieval/resolution profiles — which seed ground truth and
//! measure one stage in isolation — this profile runs raw seed transcripts
//! through the ACTUAL worker pipeline (extract → embed → resolve → contradict →
//! promote), driven by a deterministic gold [`MockProvider`], then scores the
//! extracted memories against the transcripts' `gold_memories`.
//!
//! The mock is the eval-crate analog of the pattern `pipeline_pg::perfect_mock`
//! uses: it emits the gold extraction per transcript, adjudicates entity
//! resolution straight from the gold clustering (reusing
//! [`crate::resolution_profile`]'s oracle so the two never drift), and takes the
//! conservative `dismiss` on contradictions. This slice is the deterministic
//! MockProvider→gold path; a per-provider nightly (real LLM extraction quality)
//! is out of scope.
//!
//! Matching is PRAGMATIC and documented: the mock emits each memory's content
//! verbatim as its gold `content_gist`, so a predicted memory counts as a true
//! positive iff its `content` exactly equals a gold `content_gist` (multiset —
//! duplicates are matched up to their gold multiplicity). This is the tightest
//! honest definition for the mock path; the real-provider nightly will need a
//! fuzzier semantic match.
//!
//! Two gates ride on this, mirroring the resolution profile:
//! - micro-F1 `>= baseline − delta` is a SOFT regression gate.
//! - a cross-config comparison (different embedder OR provider) is REFUSED,
//!   exactly like the retrieval/resolution embedder guard.
//!
//! The absolute numbers under the deterministic embedder + gold mock are
//! PLUMBING floors (the mock emits gold, so F1 is ~1.0 by construction), not a
//! quality claim about any real extractor.

use std::collections::HashMap;

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::{ChatProvider, ChatRequest, MockProvider, ProviderRouter};
use brainiac_pipeline::worker;
use brainiac_store::{governance, memories, orgs, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::resolution_profile::{
    gold_clustering, name_to_cluster, oracle_same, parse_adjudication_names,
};

#[derive(Debug, Clone, Serialize)]
pub struct PipelineReport {
    pub fixture_version: String,
    pub embedding_model: String,
    /// The extraction provider this run used (the gold mock's model ref). Tagged
    /// like the embedder because extraction quality is provider-specific — the
    /// regression gate refuses to compare across providers.
    pub provider: String,
    // ── content-level extraction quality (EVAL.md §2.1) ──────────────────
    pub precision: f64,
    pub recall: f64,
    pub micro_f1: f64,
    pub gold_memories: usize,
    pub extracted_memories: usize,
    /// Extracted memories whose content exactly matches a gold `content_gist`.
    pub matched_memories: usize,
    // ── per-stage counts (the chain actually ran) ────────────────────────
    pub entities_created: usize,
    pub entities_resolved: usize,
    pub contradictions_opened: usize,
    pub auto_promoted: usize,
    pub needs_review: usize,
}

impl PipelineReport {
    /// No hard gate here (the pipeline profile's zero-tolerance invariant —
    /// false merges — is owned by the resolution profile and by
    /// `pipeline_pg`). Present for symmetry with the other reports; always
    /// passes.
    pub fn gate_failures(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Committed baseline for the pipeline SOFT gate (`results/pipeline-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineBaseline {
    pub embedding_model: String,
    pub provider: String,
    pub fixture_version: String,
    pub micro_f1: f64,
}

/// micro-F1 may not regress below the committed baseline by more than this
/// (mirrors the resolution profile's `F1_DELTA`).
const F1_DELTA: f64 = 0.02;

impl PipelineBaseline {
    pub fn from_report(report: &PipelineReport) -> Self {
        Self {
            embedding_model: report.embedding_model.clone(),
            provider: report.provider.clone(),
            fixture_version: report.fixture_version.clone(),
            micro_f1: report.micro_f1,
        }
    }
}

/// Compare a run against the committed baseline (mirrors the resolution gate):
/// a cross-config comparison (different embedder OR provider) is refused, then
/// micro-F1 may not regress past `F1_DELTA`. Empty = pass.
pub fn regression_failures(report: &PipelineReport, baseline: &PipelineBaseline) -> Vec<String> {
    let mut fails = Vec::new();
    if report.embedding_model != baseline.embedding_model {
        fails.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate the baseline instead of comparing across embedders",
            report.embedding_model, baseline.embedding_model
        ));
        return fails;
    }
    if report.provider != baseline.provider {
        fails.push(format!(
            "provider mismatch: run={} baseline={} — extraction quality is provider-specific; recalibrate instead of comparing across providers",
            report.provider, baseline.provider
        ));
        return fails;
    }
    if report.micro_f1 < baseline.micro_f1 - F1_DELTA {
        fails.push(format!(
            "micro-F1 regressed: {:.3} < baseline {:.3} − {:.2}",
            report.micro_f1, baseline.micro_f1, F1_DELTA
        ));
    }
    fails
}

/// Gold extraction JSON for one transcript — the shape a perfect extractor
/// would emit. Mirrors `pipeline_pg::gold_extraction_json`; kept here so the
/// eval crate scores against the same ground truth the pipeline test asserts on.
fn gold_extraction_json(fx: &Fixtures, transcript_id: &str) -> String {
    let t = fx
        .transcripts
        .iter()
        .find(|t| t.id == transcript_id)
        .expect("transcript");
    let entity_name = |eid: &str| -> serde_json::Value {
        let e = fx
            .entities
            .entities
            .iter()
            .find(|e| e.id == eid)
            .expect("entity");
        json!({"name": e.name, "kind": e.kind})
    };
    let name_of = |eid: &str| -> String {
        fx.entities
            .entities
            .iter()
            .find(|e| e.id == eid)
            .expect("entity")
            .name
            .clone()
    };
    let memories: Vec<serde_json::Value> = t
        .gold_memories
        .iter()
        .map(|g| {
            json!({
                "kind": g.kind,
                "content": g.content_gist,
                "visibility": if g.visibility == "org" { "org" } else { "team" },
                "confidence": 0.95,
                "entities": g.entities.iter().map(|e| entity_name(e)).collect::<Vec<_>>(),
                "relations": g.relations.iter().map(|r| json!({
                    "src": name_of(&r.src), "rel": r.rel, "dst": name_of(&r.dst)
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "memories": memories }).to_string()
}

/// Deterministic gold provider: gold extraction per transcript (matched by the
/// transcript's first-turn marker), gold-clustering-derived adjudication (reuses
/// the resolution oracle), and a conservative `dismiss` on contradictions.
fn perfect_mock(fx: &Fixtures) -> MockProvider {
    let extraction: Vec<(String, String)> = fx
        .transcripts
        .iter()
        .map(|t| {
            let marker = t.turns.first().expect("turns").text.clone();
            (marker, gold_extraction_json(fx, &t.id))
        })
        .collect();
    let names = name_to_cluster(fx, &gold_clustering(fx));
    MockProvider::new(move |req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            for (marker, json) in &extraction {
                if req.user.contains(marker.as_str()) {
                    return json.clone();
                }
            }
            return r#"{"memories":[]}"#.to_string();
        }
        if req.system.contains("adjudicate") {
            let (a, b) = parse_adjudication_names(&req.user);
            let same = oracle_same(&names, &a, &b);
            return format!(r#"{{"same": {same}, "confidence": 0.9}}"#);
        }
        if req.system.contains("Decide their relationship") {
            return r#"{"relation":"dismiss","winner":null,"reason":"mock"}"#.to_string();
        }
        r#"{}"#.to_string()
    })
}

/// Run the pipeline profile end-to-end: seed identity + sources, drain them
/// through the real worker chain with the gold mock, and score the extracted
/// memories against gold. `admin` is a raw (RLS-bypassing) pool used only to
/// read back the cross-team results the pipeline wrote — the same trick
/// `pipeline_pg` uses for its assertions.
pub async fn run(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &Fixtures,
    embedder: &dyn Embedder,
) -> Result<PipelineReport> {
    let org_id = stable_uuid(&fx.org.org);
    let provider_mock = perfect_mock(fx);
    let provider_tag = provider_mock.model_ref();
    let providers = ProviderRouter::single(std::sync::Arc::new(provider_mock));
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // ── seed identity + sources (raw transcripts, NOT gold memories) ─────
    let mut tx = store.scoped_tx(&principal).await?;
    orgs::upsert_org(&mut tx, org_id, &fx.org.org).await?;
    for t in &fx.org.teams {
        orgs::upsert_team(&mut tx, stable_uuid(&t.id), org_id, &t.name).await?;
    }
    let mut source_ids: Vec<Uuid> = Vec::new();
    for t in &fx.transcripts {
        let sid = stable_uuid(&t.id);
        let text: String = t
            .turns
            .iter()
            .map(|turn| format!("{}: {}", turn.role, turn.text))
            .collect::<Vec<_>>()
            .join("\n");
        governance::insert_source(
            &mut tx,
            sid,
            org_id,
            Some(stable_uuid(&t.team)),
            "session_transcript",
            &text,
            None,
        )
        .await?;
        source_ids.push(sid);
    }
    let embedding_version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await?;
    tx.commit().await?;

    // ── enqueue + drain through the REAL worker chain ───────────────────
    for sid in &source_ids {
        worker::enqueue_source(store, org_id, *sid).await?;
    }
    let cfg = worker::WorkerConfig {
        batch: (fx.transcripts.len() as i64).max(1) + 4,
        ..Default::default()
    };
    let stats = worker::tick(store, &providers, embedder, embedding_version, &cfg).await?;

    // ── score extracted memories vs gold (admin pool bypasses RLS) ───────
    let gold_contents: Vec<String> = fx
        .transcripts
        .iter()
        .flat_map(|t| t.gold_memories.iter().map(|g| g.content_gist.clone()))
        .collect();
    let extracted_contents: Vec<String> =
        sqlx::query("SELECT content FROM memories WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(admin)
            .await
            .context("reading extracted memories")?
            .iter()
            .map(|r| r.get::<String, _>("content"))
            .collect();

    let matched = multiset_matched(&gold_contents, &extracted_contents);
    let gold_total = gold_contents.len();
    let extracted_total = extracted_contents.len();
    let precision = ratio(matched, extracted_total);
    let recall = ratio(matched, gold_total);
    let micro_f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };

    // Per-stage counts. memories/contradictions/promotions come from the tick;
    // entity counts are read back (the tick folds them into private run stats).
    let entities_created: i64 = scalar(
        admin,
        "SELECT count(*) FROM entities WHERE org_id = $1",
        org_id,
    )
    .await?;
    let entities_resolved: i64 = scalar(
        admin,
        "SELECT count(*) FROM entity_links el JOIN entities e ON e.id = el.entity_id WHERE e.org_id = $1",
        org_id,
    )
    .await?;

    Ok(PipelineReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        provider: provider_tag,
        precision,
        recall,
        micro_f1,
        gold_memories: gold_total,
        extracted_memories: extracted_total,
        matched_memories: matched,
        entities_created: entities_created as usize,
        entities_resolved: entities_resolved as usize,
        contradictions_opened: stats.contradictions_opened,
        auto_promoted: stats.auto_promoted,
        needs_review: stats.needs_review,
    })
}

/// Count of extracted items that match a gold item, honouring multiplicity:
/// for each distinct content, min(gold count, extracted count).
fn multiset_matched(gold: &[String], extracted: &[String]) -> usize {
    let mut gold_counts: HashMap<&str, usize> = HashMap::new();
    for g in gold {
        *gold_counts.entry(g.as_str()).or_default() += 1;
    }
    let mut extracted_counts: HashMap<&str, usize> = HashMap::new();
    for e in extracted {
        *extracted_counts.entry(e.as_str()).or_default() += 1;
    }
    gold_counts
        .iter()
        .map(|(content, &gc)| gc.min(extracted_counts.get(content).copied().unwrap_or(0)))
        .sum()
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

async fn scalar(admin: &sqlx::PgPool, sql: &str, org_id: Uuid) -> Result<i64> {
    Ok(sqlx::query(sql)
        .bind(org_id)
        .fetch_one(admin)
        .await?
        .get::<i64, _>("count"))
}
