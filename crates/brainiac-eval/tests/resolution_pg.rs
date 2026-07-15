//! End-to-end `resolution` profile against live Postgres (DATABASE_URL-gated):
//! seed the gold RAW entities (no canonical links), run the real resolve stage
//! over them with the oracle adjudicator, and score the predicted clustering.
//!
//! Score numbers here are PLUMBING floors for the deterministic embedder — the
//! load-bearing assertion is the HARD gate: `false_merges == 0`.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_eval::{resolution_profile, seed};
use brainiac_store::Store;

#[tokio::test]
async fn resolution_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — resolution eval test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");

    // Fresh tenant slate (admin connection; the store role can't TRUNCATE).
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();

    seed::seed_resolution(&store, &fx).await.expect("seed");
    let report = resolution_profile::run(&store, &fx, &embedder)
        .await
        .expect("resolution profile");

    println!(
        "resolution report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // Report shape mirrors the fixtures.
    assert_eq!(report.entities, fx.entities.entities.len());
    assert_eq!(report.negative_pairs, fx.merges.negative_pairs.len());
    assert_eq!(report.embedding_model, "deterministic-bow-v1");
    assert_eq!(report.fixture_version, "v1");

    // ── HARD GATE (EVAL.md §3.2): false merges are zero-tolerance ─────────
    assert_eq!(
        report.false_merges, 0,
        "HARD GATE: near-miss traps must never merge — offenders {:?}",
        report.false_merge_pairs
    );
    assert!(
        report.gate_failures().is_empty(),
        "gate_failures must agree with false_merges"
    );

    // ── metric plumbing floors ────────────────────────────────────────────
    // Precision is what the false-merge gate protects: the oracle never merges
    // a forbidden pair, so B³/pairwise precision must be perfect.
    assert!(
        report.b_cubed.precision > 0.99,
        "B³ precision should be ~1.0 with an oracle adjudicator, got {:.3}",
        report.b_cubed.precision
    );
    assert!(
        report.pairwise.precision > 0.99,
        "pairwise precision should be ~1.0, got {:.3}",
        report.pairwise.precision
    );
    // Recall is bounded by the deterministic embedder's blocking (weak on
    // cross-team paraphrase), but the F1 must be a real positive number.
    assert!(
        report.b_cubed.f1 > 0.0 && report.b_cubed.f1 <= 1.0,
        "B³ F1 in (0,1], got {:.3}",
        report.b_cubed.f1
    );
    assert!(
        report.gold_clusters > 0 && report.predicted_clusters > 0,
        "both clusterings are non-empty"
    );

    // ── baseline round-trips through the regression gate ──────────────────
    let baseline = resolution_profile::ResolutionBaseline::from_report(&report);
    assert!(
        resolution_profile::regression_failures(&report, &baseline).is_empty(),
        "a run must not regress against its own scores"
    );

    let _ = &admin;
}
