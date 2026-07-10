//! End-to-end `retrieval` profile against live Postgres: seed gold Meridian
//! fixtures, run the full QA + temporal + leak suites through the real
//! retrieval engine under real RLS, and enforce the hard gates.
//!
//! Score expectations here are PLUMBING floors for the deterministic
//! bag-of-tokens embedder — not quality claims. The bake-off (real models)
//! recalibrates them.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_eval::{retrieval_profile, seed};
use brainiac_store::Store;

#[tokio::test]
async fn retrieval_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — eval integration test needs Postgres");
        return;
    };
    brainiac_store::migrate(&url).await.expect("migrate");

    // Fresh tenant slate (admin connection; the store role can't TRUNCATE).
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();

    let seeded = seed::seed_gold(&store, &fx, &embedder).await.expect("seed");
    let report = retrieval_profile::run(&store, &fx, &embedder, seeded.embedding_version)
        .await
        .expect("profile");

    println!(
        "retrieval report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // ── HARD GATES (EVAL.md §3.2) ────────────────────────────────────────
    let gate_failures = report.gate_failures();
    assert!(
        gate_failures.is_empty(),
        "hard gates failed:\n{}",
        gate_failures.join("\n")
    );

    // ── plumbing floors for the deterministic embedder ───────────────────
    assert_eq!(
        report.queries_run,
        fx.qa.queries.len() + fx.temporal.cases.len() + fx.leak.queries.len()
    );
    let overall_ndcg = report.overall.ndcg_at_10.expect("graded queries exist");
    assert!(
        overall_ndcg > 0.35,
        "overall NDCG@10 {overall_ndcg:.3} below plumbing floor — retrieval wiring regressed"
    );
    assert!(
        report.temporal_rank1_accuracy >= 0.7,
        "temporal rank-1 accuracy {:.2} below floor — as-of assembly regressed",
        report.temporal_rank1_accuracy
    );
    let exact = &report.per_stratum["exact_identifier"];
    assert!(
        exact.ndcg_at_10.unwrap_or(0.0) > 0.5,
        "exact-identifier stratum should be strong under hybrid FTS"
    );
}
