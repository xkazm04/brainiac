//! End-to-end `divergence` profile against live Postgres (DATABASE_URL-gated):
//! seed the gold divergence clusters into one isolated org, run the REAL
//! practice-divergence sweep with the gold-oracle verdict mock, and score
//! detection + verify the soft-gate machinery evaluates.
//!
//! Score numbers are PLUMBING floors under the gold oracle: the load-bearing
//! assertions are that the real sweep RAN (every cross-team cluster gathered and
//! adjudicated on the team axis), that the divergent clusters opened rows
//! (recall), and — the point of the sweep — that the CONSISTENT clusters (the
//! agreement + different-aspect traps) did NOT (precision / false-positive rate).

use brainiac_eval::divergence_profile::{self, regression_failures, DivergenceBaseline};
use brainiac_store::Store;

async fn truncate(admin: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, practice_divergences, memories,
                  canonical_entities, entities, provenance, sources, team_members, users, teams,
                  orgs, pipeline_runs, queue.jobs, queue.archive CASCADE",
    )
    .execute(admin)
    .await
    .expect("truncate");
}

#[tokio::test]
async fn divergence_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — divergence eval test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");

    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");

    let report = divergence_profile::run(&store, &admin, &fx)
        .await
        .expect("divergence profile");
    println!(
        "divergence report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // ── tags ────────────────────────────────────────────────────────────────
    assert_eq!(report.fixture_version, "v1");
    assert_eq!(report.provider, "mock:deterministic");

    // ── the profile scored every gold cluster ────────────────────────────────
    assert_eq!(
        report.cases.len(),
        fx.divergence.clusters.len(),
        "one result per gold cluster"
    );
    let divergent = fx
        .divergence
        .clusters
        .iter()
        .filter(|c| c.expected_divergence)
        .count();
    let consistent = fx.divergence.clusters.len() - divergent;
    assert_eq!(report.gold_divergent, divergent);
    assert_eq!(report.gold_consistent, consistent);
    assert!(
        divergent > 0 && consistent > 0,
        "gold must exercise both verdicts"
    );

    // ── the real sweep gathered and adjudicated every cross-team cluster ──────
    // All gold clusters are team-axis only (no project stamping), so the number
    // of adjudications equals the cluster count. A shortfall would mean the sweep
    // never saw a cluster — a graph-gathering regression this profile exists to
    // catch.
    assert_eq!(
        report.clusters_scanned,
        fx.divergence.clusters.len(),
        "the sweep gathered and adjudicated every cluster on the team axis"
    );

    // ── detection: divergent flagged, consistent traps never flagged ─────────
    assert_eq!(
        report.detected_divergent, divergent,
        "every divergent cluster opened a row (recall floor)"
    );
    assert_eq!(report.missed, 0);
    assert_eq!(report.detection_recall, 1.0);
    assert_eq!(
        report.false_positives, 0,
        "no consistent cluster may be flagged (precision floor / queue hygiene)"
    );
    assert_eq!(report.false_positive_rate, 0.0);
    assert_eq!(report.detection_precision, 1.0);
    // The sweep didn't just detect the divergences — it named the right practice.
    assert_eq!(
        report.practice_correct, divergent,
        "each detected divergence stored the gold practice name"
    );

    // Per-cluster outcomes are internally consistent.
    for c in &report.cases {
        match c.outcome.as_str() {
            "true_positive" => assert!(c.detected && c.expected_divergence),
            "false_positive" => assert!(c.detected && !c.expected_divergence),
            "true_negative" => assert!(!c.detected && !c.expected_divergence),
            "missed" => assert!(!c.detected && c.expected_divergence),
            other => panic!("unexpected outcome {other}"),
        }
    }

    // ── gate machinery: a from-report baseline passes; changes trip it ───────
    let baseline = DivergenceBaseline::from_report(&report);
    assert!(
        regression_failures(&report, &baseline).is_empty(),
        "a run compared to its own baseline passes"
    );

    let mut cross = baseline.clone();
    cross.provider = "qwen:qwen-max".into();
    let fails = regression_failures(&report, &cross);
    assert_eq!(fails.len(), 1, "cross-provider comparison is refused");
    assert!(fails[0].contains("provider mismatch"));

    let mut stricter = baseline.clone();
    stricter.detection_recall = 1.0 + 2.0 * 0.02; // demand above the tolerance band
    assert!(
        regression_failures(&report, &stricter)
            .iter()
            .any(|f| f.contains("recall regressed")),
        "a raised recall bar trips the soft gate"
    );
}
