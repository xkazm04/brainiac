//! End-to-end bake-off grid (EVAL.md §3.1) against live Postgres (DATABASE_URL-
//! gated): run the retrieval profile across the available backend cross-product
//! and assert the decision-table artifact is complete — a tagged report per
//! runnable config plus a stated-reason skip for every unavailable one.
//!
//! Deterministic × {none, lexical} always runs (2 cells). The qwen row runs only
//! when an API key is present; otherwise it must appear as skipped-with-reason —
//! never a silent gap.

use brainiac_eval::grid;
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
async fn grid_produces_decision_table() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — grid eval test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");

    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");

    let artifact = grid::run(&store, &admin, &fx).await.expect("grid run");
    println!(
        "grid artifact: {}",
        serde_json::to_string_pretty(&artifact).expect("json")
    );

    // ── the deterministic row always ran: both rerankers, correctly tagged ──
    let det_cells: Vec<_> = artifact
        .cells
        .iter()
        .filter(|c| c.embedder == "deterministic-bow-v1")
        .collect();
    assert_eq!(
        det_cells.len(),
        2,
        "deterministic × {{none, lexical}} = 2 cells"
    );
    let rerankers: std::collections::HashSet<&str> =
        det_cells.iter().map(|c| c.reranker.as_str()).collect();
    assert!(
        rerankers.contains("none"),
        "the no-reranker cell is present"
    );
    assert!(
        rerankers.contains("lexical-overlap-v1"),
        "the lexical reranker cell is present"
    );

    // Each cell carries its full tagged report.
    for c in &det_cells {
        assert_eq!(c.report.embedding_model, "deterministic-bow-v1");
        assert_eq!(c.report.fixture_version, "v1");
        assert_eq!(c.report.reranker, c.reranker);
        // Retrieval actually ran — the hard invariant holds in every cell.
        assert!(
            c.report.rls_leaks.is_empty(),
            "no RLS leak in any grid cell: {:?}",
            c.report.rls_leaks
        );
        assert!(c.report.queries_run > 0, "queries ran in cell {}", c.config);
    }

    // ── qwen: either ran (key present) or is skipped WITH A REASON ─────────
    let qwen_ran = artifact
        .cells
        .iter()
        .any(|c| c.embedder.starts_with("qwen"));
    if !qwen_ran {
        let qwen_skips: Vec<_> = artifact
            .skipped
            .iter()
            .filter(|s| s.embedder == "qwen")
            .collect();
        assert_eq!(
            qwen_skips.len(),
            2,
            "qwen × {{none, lexical}} = 2 skipped rows when no key"
        );
        for s in &qwen_skips {
            assert!(
                s.reason.to_lowercase().contains("key"),
                "skip states the reason: {}",
                s.reason
            );
        }
    }

    // ── the markdown table renders both deterministic configs ──────────────
    let md = artifact.to_markdown();
    assert!(md.contains("| Config |"), "markdown has a table header");
    assert!(md.contains("NDCG@10"), "markdown has the NDCG columns");
    assert!(
        md.contains("deterministic-bow-v1 × none"),
        "markdown lists the no-reranker config"
    );
    assert!(
        md.contains("deterministic-bow-v1 × lexical-overlap-v1"),
        "markdown lists the lexical config"
    );
    if !qwen_ran {
        assert!(
            md.contains("Skipped configs"),
            "markdown states skipped configs"
        );
    }
}
