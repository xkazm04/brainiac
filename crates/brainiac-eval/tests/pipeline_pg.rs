//! End-to-end `pipeline` (P5) profile against live Postgres (DATABASE_URL-
//! gated): drive the seed transcripts through the REAL worker chain with the
//! deterministic gold mock and score extracted memories vs gold.
//!
//! Score numbers here are PLUMBING: the mock emits gold, so micro-F1 is ~1.0 by
//! construction. The load-bearing assertions are that the chain RAN (entities
//! created/resolved, memories written) and that the gate machinery evaluates.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_eval::pipeline_profile::{self, PipelineBaseline};
use brainiac_store::Store;

async fn truncate(admin: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  queue.jobs, queue.archive CASCADE",
    )
    .execute(admin)
    .await
    .expect("truncate");
}

#[tokio::test]
async fn pipeline_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — pipeline eval test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");

    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();

    let report = pipeline_profile::run(&store, &admin, &fx, &embedder)
        .await
        .expect("pipeline profile");

    println!(
        "pipeline report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // ── tags ─────────────────────────────────────────────────────────────
    assert_eq!(report.fixture_version, "v1");
    assert_eq!(report.embedding_model, "deterministic-bow-v1");
    assert_eq!(report.provider, "mock:deterministic");

    // ── the chain actually ran end-to-end ─────────────────────────────────
    let gold_total: usize = fx.transcripts.iter().map(|t| t.gold_memories.len()).sum();
    assert_eq!(
        report.gold_memories, gold_total,
        "gold count matches the transcripts"
    );
    assert_eq!(
        report.extracted_memories, gold_total,
        "one memory per gold item — nothing dropped"
    );
    assert!(
        report.entities_created > 0,
        "extraction created raw entities"
    );
    assert!(
        report.entities_resolved > 0,
        "resolve linked raw entities to canonicals"
    );
    assert!(
        report.auto_promoted + report.needs_review >= 1,
        "promotion stage produced audit outcomes"
    );

    // ── content-level quality (gold mock ⇒ perfect extraction) ────────────
    assert_eq!(
        report.matched_memories, gold_total,
        "every gold item matched"
    );
    assert!(
        (report.precision - 1.0).abs() < 1e-9,
        "precision {:.3} should be 1.0 under the gold mock",
        report.precision
    );
    assert!(
        (report.recall - 1.0).abs() < 1e-9,
        "recall {:.3} should be 1.0 under the gold mock",
        report.recall
    );
    assert!(
        (report.micro_f1 - 1.0).abs() < 1e-9,
        "micro-F1 {:.3} should be 1.0 under the gold mock",
        report.micro_f1
    );
    assert!(
        report.gate_failures().is_empty(),
        "no hard gates on pipeline"
    );

    // ── baseline round-trips + cross-config refusal ───────────────────────
    let baseline = PipelineBaseline::from_report(&report);
    assert!(
        pipeline_profile::regression_failures(&report, &baseline).is_empty(),
        "a run must not regress against its own baseline"
    );

    // A different provider is refused (extraction quality is provider-specific).
    let cross_provider = PipelineBaseline {
        provider: "qwen:qwen-max".into(),
        ..baseline.clone()
    };
    let fails = pipeline_profile::regression_failures(&report, &cross_provider);
    assert_eq!(
        fails.len(),
        1,
        "cross-provider comparison yields one refusal"
    );
    assert!(
        fails[0].contains("provider mismatch"),
        "the refusal names the provider mismatch: {}",
        fails[0]
    );

    // A different embedder is refused too.
    let cross_embedder = PipelineBaseline {
        embedding_model: "qwen:text-embedding-v4".into(),
        ..baseline
    };
    let embed_fails = pipeline_profile::regression_failures(&report, &cross_embedder);
    assert!(
        embed_fails.iter().any(|f| f.contains("embedder mismatch")),
        "cross-embedder comparison refused: {embed_fails:?}"
    );

    let _ = &admin;
}
