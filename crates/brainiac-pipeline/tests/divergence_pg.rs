//! Divergence identity across sweeps (DATABASE_URL-gated).
//!
//! The regression this pins: `scan_org` used to DELETE every row for the org and
//! re-INSERT the verdicts, so a divergence that persists across two sweeps came
//! back with a new `id`. Two things cite that id —
//! `standard_provenance.ref_id` (a rule's evidence trail) and
//! `library_sweep::blocked()` (candidate dedup) — so the churn silently produced
//! a duplicate standard candidate per practice per sweep and left every ratified
//! standard's provenance pointing at a row that no longer existed.
//!
//! Asserted here: (1) a still-present divergence keeps its id and gets refreshed
//! content, (2) its bridged standard candidate is still reachable through
//! `standard_provenance`, (3) `propose_from_divergence` still returns the SAME
//! candidate after a re-scan rather than minting a second one, and (4) a
//! divergence the adjudicator no longer confirms is retired.

use brainiac_core::{Lifecycle, MemoryKind, MemoryStatus, Visibility};
use brainiac_gateway::MockProvider;
use brainiac_pipeline::divergence::scan_org;
use brainiac_store::{entities, library, memories, orgs};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
fn mem(id: Uuid, org: Uuid, team: Uuid, kind: MemoryKind, content: &str) -> memories::NewMemory {
    memories::NewMemory {
        id,
        org_id: org,
        team_id: Some(team),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind,
        title: None,
        lifecycle: Lifecycle::Shipped,
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: Some(0.9),
        provenance_id: None,
        project_id: None,
    }
}

/// A verdict for whichever canonical entity the prompt is about. `confirm_retry`
/// lets the second sweep drop one of the two divergences.
fn verdict_for(user: &str, confirm_retry: bool) -> String {
    let about_retry = user.contains("Retry policy");
    let divergence = if about_retry { confirm_retry } else { true };
    format!(
        r#"{{"divergence": {divergence}, "practice": "{}", "summary": "{}",
             "recommended_standard": "one way", "impact": "high",
             "approaches": [{{"team": "payments", "approach": "a"}},
                            {{"team": "data", "approach": "b"}}]}}"#,
        if about_retry {
            "Service retry policy"
        } else {
            "Migration review"
        },
        if about_retry {
            "sweep-2 summary"
        } else {
            "two teams disagree"
        },
    )
}

#[tokio::test]
async fn rescan_preserves_divergence_identity_and_provenance() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences, memory_feedback,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let org = Uuid::new_v4();
    let (team_pay, team_data) = (Uuid::new_v4(), Uuid::new_v4());
    let mut conn = admin.acquire().await.expect("conn");
    orgs::upsert_org(&mut conn, org, "meridian")
        .await
        .expect("org");
    orgs::upsert_team(&mut conn, team_pay, org, "payments")
        .await
        .expect("t1");
    orgs::upsert_team(&mut conn, team_data, org, "data")
        .await
        .expect("t2");

    // Two clusters, each anchored on a canonical entity two teams both speak about.
    for (canon_name, statements) in [
        ("Retry policy", ["retry 3x fast", "retry with backoff"]),
        ("Migration review", ["one reviewer", "two reviewers"]),
    ] {
        let canonical = Uuid::new_v4();
        entities::insert_canonical(&mut conn, canonical, org, canon_name, "concept")
            .await
            .expect("canonical");
        for (team, text) in [team_pay, team_data].iter().zip(statements) {
            let entity = Uuid::new_v4();
            entities::insert_entity(
                &mut conn,
                entity,
                org,
                Some(*team),
                canon_name,
                "concept",
                &[],
                None,
            )
            .await
            .expect("entity");
            entities::link(&mut conn, entity, canonical, 0.9, "test", None)
                .await
                .expect("link");
            let m = Uuid::new_v4();
            memories::insert(&mut conn, &mem(m, org, *team, MemoryKind::Decision, text))
                .await
                .expect("memory");
            memories::link_entity(&mut conn, m, entity)
                .await
                .expect("me");
        }
    }

    // ── sweep 1: both clusters confirmed ────────────────────────────────
    let provider = MockProvider::new(|req| verdict_for(&req.user, true));
    let stats = scan_org(&admin, &provider, org).await.expect("sweep 1");
    assert_eq!(stats.divergences, 2, "both clusters confirmed");

    let before: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, practice, summary FROM practice_divergences WHERE org_id = $1 ORDER BY practice",
    )
    .bind(org)
    .fetch_all(&admin)
    .await
    .expect("read 1");
    assert_eq!(before.len(), 2);
    let retry_id = before
        .iter()
        .find(|(_, p, _)| p == "Service retry policy")
        .expect("retry row")
        .0;
    let review_id = before
        .iter()
        .find(|(_, p, _)| p == "Migration review")
        .expect("review row")
        .0;

    // A maintainer bridges the review divergence into a standard candidate; its
    // provenance now cites `review_id`.
    let maintainer = Uuid::new_v4();
    orgs::upsert_user(&mut conn, maintainer, org, "m@example.com")
        .await
        .expect("user");
    let candidate = library::propose_from_divergence(&mut conn, review_id, Some(maintainer))
        .await
        .expect("bridge")
        .expect("candidate");

    // ── sweep 2: the review divergence persists, retry is no longer confirmed ──
    let provider = MockProvider::new(|req| verdict_for(&req.user, false));
    let stats = scan_org(&admin, &provider, org).await.expect("sweep 2");
    assert_eq!(
        stats.divergences, 1,
        "only the review cluster still diverges"
    );

    let after: Vec<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT id, practice, summary FROM practice_divergences WHERE org_id = $1")
            .bind(org)
            .fetch_all(&admin)
            .await
            .expect("read 2");

    // (1) identity preserved, content refreshed.
    assert_eq!(after.len(), 1, "the unconfirmed divergence was retired");
    assert_eq!(
        after[0].0, review_id,
        "a persisting divergence MUST keep its id — standards cite it"
    );

    // (4) the retired one is gone.
    assert!(
        !after.iter().any(|(id, ..)| *id == retry_id),
        "a divergence the adjudicator no longer confirms is retired"
    );

    // (2) the standard's evidence trail still resolves.
    let provenance_alive: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM standard_provenance sp
          JOIN practice_divergences pd ON pd.id = sp.ref_id
         WHERE sp.standard_id = $1",
    )
    .bind(candidate)
    .fetch_one(&admin)
    .await
    .expect("provenance join");
    assert_eq!(
        provenance_alive, 1,
        "the ratified standard's provenance must still point at a live divergence"
    );

    // (3) bridging again returns the SAME candidate, not a duplicate.
    let again = library::propose_from_divergence(&mut conn, review_id, Some(maintainer))
        .await
        .expect("re-bridge")
        .expect("candidate");
    assert_eq!(
        again, candidate,
        "re-bridging must not mint a second candidate"
    );
    let candidates: i64 = sqlx::query_scalar("SELECT count(*) FROM standards WHERE org_id = $1")
        .bind(org)
        .fetch_one(&admin)
        .await
        .expect("count standards");
    assert_eq!(
        candidates, 1,
        "exactly one standard candidate for one practice"
    );
}
