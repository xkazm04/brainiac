//! Bake-off grid driver (EVAL.md §3.1): run the `retrieval` profile across the
//! cross-product of AVAILABLE backends and emit ONE decision-table artifact so a
//! §3.1 comparison is a single command instead of N hand invocations with no way
//! to line the numbers up side by side.
//!
//! The grid is EXPLORATORY. Unlike a single-config `eval` run — which enforces
//! the §3.2 hard + soft gates against a committed baseline — the grid deliberately
//! evaluates NO baselines and NO gates: it exists to SURFACE the trade-offs
//! between configs (does the lexical reranker earn its latency? does qwen move
//! cross-team NDCG enough to justify the API cost?), and gating each cell against
//! a baseline that was calibrated for one specific config would be meaningless.
//! Recalibration and gate enforcement stay the job of the single-config path.
//!
//! Availability: the deterministic embedder is always present; qwen requires an
//! API key. An unavailable backend is never a silent gap — every config it would
//! have produced is emitted as a [`SkippedCell`] carrying the reason.
//!
//! Each config runs on a FRESHLY truncated + re-seeded tenant (the eval re-seeds
//! per run anyway; the grid tightens that to per-config isolation), executed
//! sequentially — the configs share one database, so parallelism would cross the
//! streams.

use std::sync::Arc;

use anyhow::{Context, Result};
use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::rerank::{LexicalOverlapReranker, Reranker};
use brainiac_fixtures::Fixtures;
use brainiac_gateway::QwenEmbedder;
use brainiac_store::Store;
use serde::Serialize;

use crate::report::RetrievalReport;

/// Fresh-tenant truncate, identical to the single-config `eval` path (queue
/// tables included so the grid can share a database with a `pipeline` run).
const TRUNCATE_SQL: &str =
    "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings,
              entity_links, edges, contradictions, promotions, memories, canonical_entities,
              entities, provenance, sources, team_members, users, teams, orgs, pipeline_runs,
              queue.jobs, queue.archive CASCADE";

/// One decision-table artifact: every backend config that ran (keyed by config),
/// plus every config that was skipped with its reason. Serialized to JSON; the
/// same data renders to the markdown table via [`GridArtifact::to_markdown`].
#[derive(Debug, Clone, Serialize)]
pub struct GridArtifact {
    /// UTC date the grid ran (YYYY-MM-DD).
    pub generated: String,
    pub fixture_version: String,
    /// The embedder axis the grid attempted (available or not).
    pub embedder_axis: Vec<String>,
    /// The reranker axis the grid attempted.
    pub reranker_axis: Vec<String>,
    /// Configs that ran, each carrying its full tagged retrieval report.
    pub cells: Vec<GridCell>,
    /// Configs skipped because a backend was unavailable — never a silent gap.
    pub skipped: Vec<SkippedCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GridCell {
    /// Human key, e.g. `deterministic-bow-v1 × lexical-overlap-v1`.
    pub config: String,
    /// The report's `embedding_model` tag (the real model name, not the CLI alias).
    pub embedder: String,
    /// The report's `reranker` tag (`none` or the reranker's model name).
    pub reranker: String,
    pub report: RetrievalReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedCell {
    pub config: String,
    pub embedder: String,
    pub reranker: String,
    /// Why this config could not run (e.g. `qwen: no API key`).
    pub reason: String,
}

/// Default artifact stem (extension-less): `results/grid/<date>-grid`. The CLI
/// writes `<stem>.json` and `<stem>.md`.
pub fn default_out_stem() -> String {
    format!("results/grid/{}-grid", today())
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// An embedder candidate: either a live instance or the reason it is unavailable.
type EmbedderCandidate = (String, std::result::Result<Arc<dyn Embedder>, String>);

/// The embedder axis: deterministic is always present; qwen is attempted from
/// the environment and, when its key is absent, carried as an unavailable
/// candidate so the grid can state the skip reason rather than drop the row.
fn embedder_candidates() -> Vec<EmbedderCandidate> {
    let deterministic: Arc<dyn Embedder> = Arc::new(DeterministicEmbedder::default());
    let qwen: std::result::Result<Arc<dyn Embedder>, String> = match QwenEmbedder::from_env() {
        Some(e) => Ok(Arc::new(e)),
        None => Err("qwen: no API key (set QWEN_API_KEY or DASHSCOPE_API_KEY)".to_string()),
    };
    vec![
        ("deterministic".into(), Ok(deterministic)),
        ("qwen".into(), qwen),
    ]
}

/// The reranker axis: no reranker (byte-identical pre-stage-5 path) and the
/// deterministic lexical-overlap scorer (the bake-off seam).
fn reranker_candidates() -> Vec<(String, Option<Arc<dyn Reranker>>)> {
    vec![
        ("none".into(), None),
        ("lexical".into(), Some(Arc::new(LexicalOverlapReranker))),
    ]
}

/// Run the full grid: for every AVAILABLE embedder × reranker, truncate +
/// re-seed the gold fixtures and run the retrieval profile; collect one tagged
/// report per config. Unavailable configs are recorded as skips. `admin` is the
/// RLS-bypassing pool used only to truncate between configs.
pub async fn run(store: &Store, admin: &sqlx::PgPool, fx: &Fixtures) -> Result<GridArtifact> {
    let embedders = embedder_candidates();
    let rerankers = reranker_candidates();
    let embedder_axis: Vec<String> = embedders.iter().map(|(n, _)| n.clone()).collect();
    let reranker_axis: Vec<String> = rerankers.iter().map(|(n, _)| n.clone()).collect();

    let mut cells: Vec<GridCell> = Vec::new();
    let mut skipped: Vec<SkippedCell> = Vec::new();

    for (embedder_name, embedder) in &embedders {
        match embedder {
            Err(reason) => {
                // Backend unavailable: emit a skip for the whole reranker row so
                // the artifact's cross-product stays complete (no silent gaps).
                for (reranker_name, _) in &rerankers {
                    skipped.push(SkippedCell {
                        config: format!("{embedder_name} × {reranker_name}"),
                        embedder: embedder_name.clone(),
                        reranker: reranker_name.clone(),
                        reason: reason.clone(),
                    });
                }
            }
            Ok(embedder) => {
                for (reranker_name, reranker) in &rerankers {
                    // Fresh, isolated tenant per config.
                    sqlx::query(TRUNCATE_SQL)
                        .execute(admin)
                        .await
                        .context("truncate between grid configs")?;
                    let seeded = crate::seed::seed_gold(store, fx, embedder.as_ref())
                        .await
                        .with_context(|| {
                            format!("seeding for {embedder_name} × {reranker_name}")
                        })?;
                    let (report, _diag) = crate::retrieval_profile::run(
                        store,
                        fx,
                        embedder.as_ref(),
                        reranker.as_deref(),
                        seeded.embedding_version,
                    )
                    .await
                    .with_context(|| {
                        format!("retrieval profile {embedder_name} × {reranker_name}")
                    })?;
                    cells.push(GridCell {
                        config: format!("{} × {}", report.embedding_model, report.reranker),
                        embedder: report.embedding_model.clone(),
                        reranker: report.reranker.clone(),
                        report,
                    });
                }
            }
        }
    }

    Ok(GridArtifact {
        generated: today(),
        // The fixture tree is v1; the reports carry the same tag (see
        // `RetrievalReport::fixture_version`). Kept here for the artifact header.
        fixture_version: cells
            .first()
            .map(|c| c.report.fixture_version.clone())
            .unwrap_or_else(|| "v1".to_string()),
        embedder_axis,
        reranker_axis,
        cells,
        skipped,
    })
}

impl GridArtifact {
    /// Render the decision table as markdown: one row per config, columns for
    /// overall + per-stratum NDCG@10, temporal rank-1, RLS leaks, and p50/p95
    /// retrieval latency. Skipped configs follow in their own list.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# Retrieval bake-off grid — {} (fixtures {})\n\n",
            self.generated, self.fixture_version
        ));
        out.push_str(
            "Exploratory (§3.1): no baselines or regression gates are evaluated — the grid \
             surfaces cross-config trade-offs. Latency is INFORMATIONAL (host-dependent).\n\n",
        );

        // Union of strata across every cell, sorted for stable columns.
        let mut strata: Vec<String> = self
            .cells
            .iter()
            .flat_map(|c| c.report.per_stratum.keys().cloned())
            .collect();
        strata.sort();
        strata.dedup();

        // Header.
        let mut header = String::from("| Config | Overall NDCG@10 |");
        for s in &strata {
            header.push_str(&format!(" {s} NDCG@10 |"));
        }
        header.push_str(" Temporal R@1 | Leaks | p50 ms | p95 ms |\n");
        out.push_str(&header);

        let mut sep = String::from("|---|---|");
        for _ in &strata {
            sep.push_str("---|");
        }
        sep.push_str("---|---|---|---|\n");
        out.push_str(&sep);

        let fmt_opt = |v: Option<f64>| match v {
            Some(n) => format!("{n:.3}"),
            None => "—".to_string(),
        };

        for c in &self.cells {
            let r = &c.report;
            let mut row = format!("| {} | {} |", c.config, fmt_opt(r.overall.ndcg_at_10));
            for s in &strata {
                let v = r.per_stratum.get(s).and_then(|st| st.ndcg_at_10);
                row.push_str(&format!(" {} |", fmt_opt(v)));
            }
            row.push_str(&format!(
                " {:.3} | {} | {:.1} | {:.1} |\n",
                r.temporal_rank1_accuracy,
                r.rls_leaks.len(),
                r.latency.overall.p50_ms,
                r.latency.overall.p95_ms,
            ));
            out.push_str(&row);
        }

        if !self.skipped.is_empty() {
            out.push_str("\n## Skipped configs\n\n");
            for s in &self.skipped {
                out.push_str(&format!("- `{}` — {}\n", s.config, s.reason));
            }
        }
        out
    }
}
