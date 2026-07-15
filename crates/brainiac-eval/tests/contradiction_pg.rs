//! End-to-end `contradiction` profile (EVAL.md §2.3) against live Postgres
//! (DATABASE_URL-gated): seed the gold contradiction pairs into isolated orgs,
//! run the REAL contradict stage with the gold-oracle verdict mock, and score
//! detection + verify the soft gate machinery evaluates.
//!
//! Score numbers are PLUMBING floors under the deterministic embedder + gold
//! oracle: the load-bearing assertions are that the real stage RAN (pairs were
//! compared, rows opened with directions), that the coexist/dismiss traps are
//! NOT flagged (precision), and that a gold supersede sharing no entity anchor
//! is honestly recorded as `not_compared` rather than silently dropped.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_eval::contradiction_profile::{self, regression_failures, ContradictionBaseline};
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
async fn contradiction_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — contradiction eval test needs Postgres");
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

    let report = contradiction_profile::run(&store, &fx, &embedder)
        .await
        .expect("contradiction profile");
    println!(
        "contradiction report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // ── tags ───────────────────────────────────────────────────────────────
    assert_eq!(report.fixture_version, "v1");
    assert_eq!(report.embedding_model, "deterministic-bow-v1");
    assert_eq!(report.provider, "mock:deterministic");

    // ── the profile scored every gold case ──────────────────────────────────
    assert_eq!(
        report.cases.len(),
        fx.contradictions.cases.len(),
        "one result per gold case"
    );
    let supersede = fx
        .contradictions
        .cases
        .iter()
        .filter(|c| c.expected == "resolved_supersede")
        .count();
    let non_contradiction = fx.contradictions.cases.len() - supersede;
    assert_eq!(report.gold_supersede, supersede);
    assert_eq!(report.gold_non_contradiction, non_contradiction);

    // ── the real stage detected supersessions and never flagged a trap ──────
    assert!(
        report.detected_supersede > 0,
        "the contradict stage opened rows for genuine supersessions"
    );
    assert_eq!(
        report.false_positives, 0,
        "no coexist/dismiss trap may be flagged (precision floor)"
    );
    assert_eq!(report.false_positive_rate, 0.0);
    assert_eq!(report.detection_precision, 1.0);

    // Every detected supersede recorded the correct b_over_a direction.
    assert_eq!(
        report.supersede_direction_correct, report.supersede_direction_scored,
        "detected supersedes carry the gold direction"
    );
    assert_eq!(report.direction_accuracy, 1.0);

    // The detected + not-compared supersedes account for the whole gold set —
    // a supersede is either flagged or honestly recorded as never compared.
    assert_eq!(
        report.detected_supersede + report.not_compared,
        report.gold_supersede,
        "every gold supersede is flagged or explicitly not_compared"
    );

    // Per-case outcomes are internally consistent.
    for c in &report.cases {
        match c.outcome.as_str() {
            "true_positive" => assert!(c.flagged && c.compared),
            "false_positive" => assert!(c.flagged),
            "true_negative" => assert!(!c.flagged),
            "not_compared" => assert!(!c.compared && !c.flagged),
            "missed" => assert!(c.compared && !c.flagged),
            other => panic!("unexpected outcome {other}"),
        }
    }

    // ── gate machinery: a from-report baseline passes; a raised bar fails ───
    let baseline = ContradictionBaseline::from_report(&report);
    assert!(
        regression_failures(&report, &baseline).is_empty(),
        "a run compared to its own baseline passes"
    );

    let mut cross = baseline.clone();
    cross.embedding_model = "qwen:text-embedding-v4".into();
    let fails = regression_failures(&report, &cross);
    assert_eq!(fails.len(), 1, "cross-embedder comparison is refused");
    assert!(fails[0].contains("embedder mismatch"));

    let mut stricter = baseline.clone();
    stricter.detection_recall = 1.0; // demand perfect recall
    assert!(
        regression_failures(&report, &stricter)
            .iter()
            .any(|f| f.contains("recall regressed")),
        "a raised recall bar trips the soft gate"
    );
}
