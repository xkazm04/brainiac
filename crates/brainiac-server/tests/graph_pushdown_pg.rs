//! Graph-expansion filter pushdown (DATABASE_URL-gated).
//!
//! The graph-extra fetch used to skip the filtered candidate stage entirely:
//! no kind/status/team/confidence/project filter and no temporal bound reached
//! the SQL, so a bounded fetch over a high-fanout entity's neighborhood paid DB
//! and network cost proportional to the corpus and could then discard 100% of
//! what it fetched in Rust.
//!
//! The risk of pushing those predicates down is that the SQL and the Rust
//! predicate drift apart — a silent recall change in the one stratum the eval
//! exists to defend. So this suite checks the two properties that matter:
//!
//!   * **Soundness** — everything returned satisfies the filter (and the
//!     temporal window), including the project lens' deliberate inclusion of
//!     org-shared rows.
//!   * **Completeness** — nothing that satisfies the filter is missing:
//!     with an unbounded cap the SQL result is EXACTLY the unfiltered result
//!     narrowed by an independently written predicate over public fields.
//!
//! Plus the point of the change: under a bounded cap the filtered fetch spends
//! its whole budget on keepable rows instead of discarding part of it.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::temporal::valid_at;
use brainiac_core::{Memory, MemoryKind, MemoryStatus, Principal};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::memories;
use brainiac_store::retrieval::RetrievalFilters;
use brainiac_store::Store;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Independently written twin of `RetrievalFilters::admits` (which is private)
/// plus `valid_at`. Deliberately NOT a call into the production predicate: the
/// whole point is to check the SQL against a second opinion.
fn keeps(m: &Memory, f: &RetrievalFilters, as_of: Option<DateTime<Utc>>) -> bool {
    if !f.kinds.is_empty() && !f.kinds.contains(&m.kind) {
        return false;
    }
    if let Some(floor) = f.min_status {
        let rank = |s: MemoryStatus| match s {
            MemoryStatus::Raw => 0,
            MemoryStatus::Candidate => 1,
            MemoryStatus::Canonical => 2,
            _ => -1,
        };
        if rank(m.status) < rank(floor) || rank(m.status) < 0 {
            return false;
        }
    }
    if let Some(t) = f.team_id {
        if m.team_id != Some(t) {
            return false;
        }
    }
    if let Some(c) = f.min_confidence {
        match m.confidence {
            Some(mc) if mc >= c => {}
            _ => return false,
        }
    }
    if let Some(p) = f.project_id {
        // The project lens is inclusive by design: the project's rows PLUS the
        // org-shared ones.
        if m.project_id != Some(p) && m.project_id.is_some() {
            return false;
        }
    }
    if let Some(t) = as_of {
        if !valid_at(m, t) {
            return false;
        }
    }
    true
}

const BIG: i64 = 10_000;

#[tokio::test]
async fn graph_extra_filters_push_down_without_changing_the_result_set() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE embedding_versions, memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();
    brainiac_eval::seed::seed_gold(&store, &fx, &embedder)
        .await
        .expect("seed");

    // Every entity in the corpus — the widest possible fan-out, which is
    // exactly the case the pushdown exists for.
    let entity_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT entity_id FROM memory_entities")
            .fetch_all(&admin)
            .await
            .expect("entities");
    assert!(!entity_ids.is_empty(), "fixture has no entity links");

    // An org-wide principal: the point here is the filter predicate, not RLS
    // (which the leak suite owns) — but the reads still go through scoped_tx.
    let principal = Principal {
        org_id: stable_uuid(&fx.org.org),
        user_id: stable_uuid("user-eng-lead"),
        team_ids: vec![
            stable_uuid("team-payments"),
            stable_uuid("team-data"),
            stable_uuid("team-platform"),
        ],
        project_id: None,
    };
    let mut tx = store.scoped_tx(&principal).await.expect("tx");

    // The unfiltered baseline the Rust stage used to narrow after the fact.
    let all = memories::for_entities(&mut tx, &entity_ids, BIG)
        .await
        .expect("unfiltered");
    assert!(all.len() > 10, "need a real corpus, got {}", all.len());

    let now = Utc::now();
    let cases: Vec<(&str, RetrievalFilters, Option<DateTime<Utc>>)> = vec![
        ("unfiltered", RetrievalFilters::default(), None),
        ("temporal only", RetrievalFilters::default(), Some(now)),
        (
            "kind",
            RetrievalFilters {
                kinds: vec![MemoryKind::Decision],
                ..Default::default()
            },
            Some(now),
        ),
        (
            "kind (multi)",
            RetrievalFilters {
                kinds: vec![MemoryKind::Decision, MemoryKind::Fact],
                ..Default::default()
            },
            Some(now),
        ),
        (
            "status floor",
            RetrievalFilters {
                min_status: Some(MemoryStatus::Canonical),
                ..Default::default()
            },
            Some(now),
        ),
        (
            "team",
            RetrievalFilters {
                team_id: Some(stable_uuid("team-payments")),
                ..Default::default()
            },
            Some(now),
        ),
        (
            "confidence",
            RetrievalFilters {
                min_confidence: Some(0.5),
                ..Default::default()
            },
            Some(now),
        ),
        (
            "project lens",
            RetrievalFilters {
                project_id: Some(Uuid::new_v4()),
                ..Default::default()
            },
            Some(now),
        ),
        (
            "everything at once",
            RetrievalFilters {
                kinds: vec![MemoryKind::Decision, MemoryKind::Fact],
                min_status: Some(MemoryStatus::Canonical),
                team_id: Some(stable_uuid("team-payments")),
                min_confidence: Some(0.1),
                project_id: None,
            },
            Some(now),
        ),
    ];

    let mut narrowing_cases = 0usize;
    for (name, filters, as_of) in &cases {
        let got = memories::for_entities_filtered(&mut tx, &entity_ids, BIG, filters, *as_of)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));

        // Soundness: nothing the filter forbids came back.
        for m in &got {
            assert!(
                keeps(m, filters, *as_of),
                "{name}: SQL returned a row the predicate rejects ({}, kind={:?}, status={:?}, team={:?}, conf={:?}, project={:?})",
                m.id, m.kind, m.status, m.team_id, m.confidence, m.project_id
            );
        }

        // Completeness: the SQL result IS the Rust-narrowed unfiltered result.
        // Compared as sets of ids — `for_entities` orders by created_at, and
        // ties in the fixture are not a stable ordering contract.
        let expect: std::collections::BTreeSet<Uuid> = all
            .iter()
            .filter(|m| keeps(m, filters, *as_of))
            .map(|m| m.id)
            .collect();
        let actual: std::collections::BTreeSet<Uuid> = got.iter().map(|m| m.id).collect();
        assert_eq!(
            actual, expect,
            "{name}: pushed-down SQL disagrees with the Rust predicate"
        );

        if expect.len() < all.len() {
            narrowing_cases += 1;
        }
    }
    assert!(
        narrowing_cases >= 4,
        "fixture no longer exercises real narrowing ({narrowing_cases} cases)"
    );

    // ── And the point of the change: a bounded fetch under a narrow filter now
    //    spends its whole budget on KEEPABLE rows. Pre-change the cap was taken
    //    before the narrowing, so the surviving count was whatever happened to
    //    pass — strictly fewer whenever the filter rejects part of the head.
    // Mirrors `retrieval::GRAPH_EXTRA_MEMORIES` — the real graph budget.
    let cap = 10i64;
    // What the old shape produced: cap first, narrow afterwards.
    let head = memories::for_entities(&mut tx, &entity_ids, cap)
        .await
        .expect("unfiltered capped");
    let mut demonstrated = 0usize;
    for (name, filters, as_of) in &cases {
        let keepable = all.iter().filter(|m| keeps(m, filters, *as_of)).count();
        let old_shape = head.iter().filter(|m| keeps(m, filters, *as_of)).count();
        if keepable < cap as usize || old_shape >= cap as usize {
            // Either the corpus can't fill the budget under this filter, or the
            // filter happens to reject nothing in the head — no waste to show.
            continue;
        }
        let filtered_capped =
            memories::for_entities_filtered(&mut tx, &entity_ids, cap, filters, *as_of)
                .await
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            filtered_capped.len(),
            cap as usize,
            "{name}: the filtered fetch must fill its whole budget with keepable rows \
             ({keepable} qualify corpus-wide)"
        );
        assert!(
            old_shape < filtered_capped.len(),
            "{name}: expected the pre-filter cap to waste budget (old={old_shape})"
        );
        demonstrated += 1;
    }
    assert!(
        demonstrated > 0,
        "fixture no longer demonstrates a filter that wasted the graph budget"
    );

    tx.commit().await.expect("commit");
}
