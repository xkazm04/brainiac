//! End-to-end `retrieval` profile against live Postgres: seed gold Meridian
//! fixtures, run the full QA + temporal + leak suites through the real
//! retrieval engine under real RLS, and enforce the hard gates.
//!
//! Score expectations here are PLUMBING floors for the deterministic
//! bag-of-tokens embedder — not quality claims. The bake-off (real models)
//! recalibrates them.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::rerank::LexicalOverlapReranker;
use brainiac_eval::gates::{regression_failures, Baseline};
use brainiac_eval::{retrieval_profile, seed};
use brainiac_store::Store;

async fn truncate(admin: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs CASCADE",
    )
    .execute(admin)
    .await
    .expect("truncate");
}

#[tokio::test]
async fn retrieval_profile_end_to_end() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — eval integration test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");

    // Fresh tenant slate (admin connection; the store role can't TRUNCATE).
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();

    let seeded = seed::seed_gold(&store, &fx, &embedder).await.expect("seed");
    let (report, diagnostics) =
        retrieval_profile::run(&store, &fx, &embedder, None, seeded.embedding_version)
            .await
            .expect("profile");

    println!(
        "retrieval report: {}",
        serde_json::to_string_pretty(&report).expect("json")
    );

    // Direction 1: no reranker tags "none" and round-trips through the gate.
    assert_eq!(report.reranker, "none");
    assert_eq!(diagnostics.reranker, "none");

    // ── diagnostics mirror the run ───────────────────────────────────────
    assert_eq!(
        diagnostics.queries.len(),
        report.queries_run,
        "one diagnostic per executed query"
    );
    // Failures sort first, and every hard-gate breach names its query.
    let first_pass_idx = diagnostics.queries.iter().position(|q| q.pass);
    if let Some(i) = first_pass_idx {
        assert!(
            diagnostics.queries[i..].iter().all(|q| q.pass),
            "diagnostics must be ordered failures-first"
        );
    }
    for q in &diagnostics.queries {
        assert_eq!(
            q.pass,
            q.violations.is_empty(),
            "pass flag must match violations for {}",
            q.id
        );
        if q.suite == "qa" {
            assert!(q.stratum.is_some(), "qa diagnostics carry a stratum");
        }
    }

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

    // ── Direction 2: latency captured (informational, never gated) ────────
    // Overall covers every retrieval call across the three suites.
    assert_eq!(
        report.latency.overall.count, report.queries_run,
        "one latency sample per retrieval call"
    );
    assert!(
        report.latency.overall.p95_ms >= report.latency.overall.p50_ms,
        "p95 must be >= p50: {:?}",
        report.latency.overall
    );
    assert!(
        report.latency.overall.mean_ms > 0.0,
        "real retrieval calls take measurable time"
    );
    // Per-suite sample counts partition the overall count.
    let suite_total: usize = report.latency.per_suite.values().map(|s| s.count).sum();
    assert_eq!(suite_total, report.latency.overall.count);
    assert_eq!(
        report.latency.per_suite["qa"].count,
        fx.qa.queries.len(),
        "qa suite latency covers every qa query"
    );
    // Per-stratum latency covers exactly the QA queries.
    let stratum_total: usize = report.latency.per_stratum.values().map(|s| s.count).sum();
    assert_eq!(stratum_total, fx.qa.queries.len());
    // Every diagnostic carries its own per-query ms.
    assert!(
        diagnostics.queries.iter().all(|q| q.latency_ms >= 0.0),
        "each diagnostic records a non-negative per-query latency"
    );
}

/// Direction 1: a lexical-reranker run produces a report TAGGED with the
/// reranker, its scores gate against a same-reranker baseline, and a
/// cross-reranker baseline comparison is REFUSED with a clear error.
#[tokio::test]
async fn reranker_axis_tags_and_gates() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — reranker eval test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();
    let reranker = LexicalOverlapReranker;

    let seeded = seed::seed_gold(&store, &fx, &embedder).await.expect("seed");
    let (report, diagnostics) = retrieval_profile::run(
        &store,
        &fx,
        &embedder,
        Some(&reranker),
        seeded.embedding_version,
    )
    .await
    .expect("profile");

    // The report is tagged with the reranker's model name.
    assert_eq!(report.reranker, "lexical-overlap-v1");
    assert_eq!(diagnostics.reranker, "lexical-overlap-v1");

    // Hard gates still evaluate (RLS is orthogonal to reranking).
    assert!(
        report.gate_failures().is_empty(),
        "hard gates failed under lexical reranker:\n{}",
        report.gate_failures().join("\n")
    );

    // Its scores gate cleanly against a baseline captured from the same
    // reranker (a run must not regress against itself).
    let lexical_baseline = Baseline::from_report(&report).expect("baseline");
    assert_eq!(lexical_baseline.reranker, "lexical-overlap-v1");
    assert!(
        regression_failures(&report, &lexical_baseline).is_empty(),
        "lexical run must not regress against its own baseline"
    );

    // A cross-reranker comparison is REFUSED: gating this lexical run against a
    // no-reranker baseline errors clearly instead of comparing incomparable
    // rankings.
    let none_baseline = Baseline {
        reranker: "none".into(),
        ..lexical_baseline
    };
    let fails = regression_failures(&report, &none_baseline);
    assert_eq!(
        fails.len(),
        1,
        "cross-reranker comparison yields one refusal"
    );
    assert!(
        fails[0].contains("reranker mismatch"),
        "the refusal names the reranker mismatch: {}",
        fails[0]
    );
}
