//! LB3 mining-sweep tests (DATABASE_URL-gated) — the LIBRARY-PLAN gate:
//!
//! - Seeded signals yield exactly the expected candidates, each carrying its
//!   signal as provenance (a candidate that cannot say where it came from is
//!   noise with a UI).
//! - The sweep is idempotent: a second run over the same corpus creates
//!   nothing — every signal is already spoken for.
//! - **A rejected candidate never reappears within the dedup window** (the
//!   hard gate): rejection is knowledge, and a maintainer who said no is not
//!   asked again next week. Past the window, the signal may return — dated
//!   and attributed like the first time.
//! - An org with no signal is a no-op.

use brainiac_core::{
    Lifecycle, MemoryKind, MemoryStatus, StandardLifecycle, StandardProvenanceKind, Visibility,
};
use brainiac_pipeline::library_sweep::{mine_all, DEFAULT_DEDUP_WINDOW_DAYS};
use brainiac_store::{library, memories, orgs};
use serde_json::json;
use uuid::Uuid;

fn mem(id: Uuid, org: Uuid, kind: MemoryKind, content: &str) -> memories::NewMemory {
    memories::NewMemory {
        id,
        org_id: org,
        team_id: None,
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
    }
}

async fn feedback(pool: &sqlx::PgPool, org: Uuid, memory: Uuid, user: Uuid, verdict: &str) {
    sqlx::query(
        "INSERT INTO memory_feedback (id, org_id, memory_id, user_id, verdict)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .bind(memory)
    .bind(user)
    .bind(verdict)
    .execute(pool)
    .await
    .expect("feedback");
}

#[tokio::test]
async fn mining_dedups_rejections_and_leaves_quiet_orgs_alone() {
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

    let (org_a, org_quiet) = (Uuid::new_v4(), Uuid::new_v4());
    let maintainer = Uuid::new_v4();
    let mut conn = admin.acquire().await.expect("conn");
    orgs::upsert_org(&mut conn, org_a, "meridian")
        .await
        .expect("org a");
    orgs::upsert_org(&mut conn, org_quiet, "quiet-co")
        .await
        .expect("org b");

    // ── the signals ──────────────────────────────────────────────────────
    // (a) one unclaimed drift + one already bridged by a human.
    let (d_unclaimed, d_claimed) = (Uuid::new_v4(), Uuid::new_v4());
    for (id, practice, rec) in [
        (
            d_unclaimed,
            "Service retry policy",
            "Exponential backoff with full jitter, max 30s",
        ),
        (
            d_claimed,
            "Migration review",
            "Every migration reviewed by a second person",
        ),
    ] {
        sqlx::query(
            "INSERT INTO practice_divergences
                (id, org_id, practice, summary, recommended_standard, impact, positions, model_ref)
             VALUES ($1,$2,$3,'two teams disagree',$4,'high',$5,'test-model')",
        )
        .bind(id)
        .bind(org_a)
        .bind(practice)
        .bind(rec)
        .bind(json!([{"team": "payments", "approach": "a"}, {"team": "data", "approach": "b"}]))
        .execute(&admin)
        .await
        .expect("divergence");
    }
    library::propose_from_divergence(&mut conn, d_claimed, Some(maintainer))
        .await
        .expect("bridge")
        .expect("claimed");

    // (b) one reinforced pattern (two independent confirmations), one pattern
    // confirmed only once (below threshold), one reinforced FACT (wrong kind).
    let (m_reinforced, m_once, m_fact) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    memories::insert(
        &mut conn,
        &mem(
            m_reinforced,
            org_a,
            MemoryKind::Pattern,
            "Idempotency keys on every webhook write.",
        ),
    )
    .await
    .expect("m1");
    memories::insert(
        &mut conn,
        &mem(m_once, org_a, MemoryKind::Pattern, "Cache warms on deploy."),
    )
    .await
    .expect("m2");
    memories::insert(
        &mut conn,
        &mem(m_fact, org_a, MemoryKind::Fact, "The queue is Postgres."),
    )
    .await
    .expect("m3");
    let (u1, u2) = (Uuid::new_v4(), Uuid::new_v4());
    feedback(&admin, org_a, m_reinforced, u1, "helpful").await;
    feedback(&admin, org_a, m_reinforced, u2, "helpful").await;
    feedback(&admin, org_a, m_once, u1, "helpful").await;
    feedback(&admin, org_a, m_fact, u1, "helpful").await;
    feedback(&admin, org_a, m_fact, u2, "helpful").await;

    // (c) a settled contradiction whose winner states a convention.
    let (m_win, m_lose) = (Uuid::new_v4(), Uuid::new_v4());
    memories::insert(
        &mut conn,
        &mem(
            m_win,
            org_a,
            MemoryKind::Decision,
            "Feature flags retire within two releases.",
        ),
    )
    .await
    .expect("win");
    let mut loser = mem(
        m_lose,
        org_a,
        MemoryKind::Decision,
        "Feature flags live forever.",
    );
    loser.status = MemoryStatus::Deprecated;
    loser.superseded_by = Some(m_win);
    memories::insert(&mut conn, &loser).await.expect("lose");
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
         VALUES ($1,$2,$3,$4,'pipeline','resolved_supersede','flags rot; the org chose a retirement window')",
    )
    .bind(Uuid::new_v4())
    .bind(org_a)
    .bind(m_win)
    .bind(m_lose)
    .execute(&admin)
    .await
    .expect("contradiction");

    // ── run 1: exactly the expected candidates ───────────────────────────
    let s1 = mine_all(&admin, DEFAULT_DEDUP_WINDOW_DAYS)
        .await
        .expect("run 1");
    assert_eq!(s1.orgs, 2);
    assert_eq!(
        (s1.from_divergence, s1.from_feedback, s1.from_contradiction),
        (1, 1, 1),
        "one candidate per signal class: {s1:?}"
    );
    assert!(s1.deduped >= 1, "the human-bridged drift must be skipped");

    // Every candidate is PROPOSED and carries its signal as provenance.
    let drift_candidate = library::get_standard_by_slug(&mut conn, "service-retry-policy")
        .await
        .expect("get")
        .expect("drift candidate");
    assert_eq!(drift_candidate.lifecycle, StandardLifecycle::Proposed);
    let prov = library::provenance(&mut conn, drift_candidate.id)
        .await
        .expect("prov");
    assert_eq!(prov[0].kind, StandardProvenanceKind::Divergence);
    assert_eq!(prov[0].ref_id, d_unclaimed);

    let reinforced =
        library::get_standard_by_slug(&mut conn, "idempotency-keys-on-every-webhook-write")
            .await
            .expect("get")
            .expect("reinforced candidate");
    assert_eq!(
        reinforced.statement,
        "Idempotency keys on every webhook write."
    );
    assert_eq!(reinforced.category, "pattern");

    // The below-threshold and wrong-kind signals produced nothing.
    assert!(
        library::get_standard_by_slug(&mut conn, "cache-warms-on-deploy")
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        library::get_standard_by_slug(&mut conn, "the-queue-is-postgres")
            .await
            .expect("get")
            .is_none()
    );

    // Quiet org: untouched.
    let quiet_count: i64 = sqlx::query_scalar("SELECT count(*) FROM standards WHERE org_id = $1")
        .bind(org_quiet)
        .fetch_one(&admin)
        .await
        .expect("count");
    assert_eq!(quiet_count, 0, "no signal → no candidates → no-op");

    // ── run 2: idempotent — everything is spoken for ─────────────────────
    let s2 = mine_all(&admin, DEFAULT_DEDUP_WINDOW_DAYS)
        .await
        .expect("run 2");
    assert_eq!(s2.candidates(), 0, "a re-run must create nothing: {s2:?}");
    assert_eq!(s2.deduped, 4, "all four claimed signals skip: {s2:?}");

    // ── the hard gate: a rejection stays rejected within the window ─────
    assert!(
        library::reject_standard(&mut conn, drift_candidate.id, maintainer)
            .await
            .expect("reject")
    );
    let s3 = mine_all(&admin, DEFAULT_DEDUP_WINDOW_DAYS)
        .await
        .expect("run 3");
    assert_eq!(
        s3.candidates(),
        0,
        "a rejected candidate must NOT reappear within the dedup window: {s3:?}"
    );

    // …and past the window, the signal may return — as a NEW dated candidate,
    // never by resurrecting the rejected row.
    sqlx::query("UPDATE standards SET updated_at = now() - interval '1 day' * $2 WHERE id = $1")
        .bind(drift_candidate.id)
        .bind(DEFAULT_DEDUP_WINDOW_DAYS + 1)
        .execute(&admin)
        .await
        .expect("age rejection");
    let s4 = mine_all(&admin, DEFAULT_DEDUP_WINDOW_DAYS)
        .await
        .expect("run 4");
    assert_eq!(
        s4.from_divergence, 1,
        "past the window the drift is minable again: {s4:?}"
    );
    let rejected_still = library::get_standard(&mut conn, drift_candidate.id)
        .await
        .expect("get")
        .expect("rejected row kept");
    assert_eq!(
        rejected_still.lifecycle,
        StandardLifecycle::Rejected,
        "the rejection is history, not a draft to reopen"
    );
    // The new candidate is a distinct row wearing a suffixed slug (the base
    // slug belongs to the rejected row forever).
    let fresh: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM standards WHERE org_id = $1 AND slug LIKE 'service-retry-policy-%' AND lifecycle = 'proposed'",
    )
    .bind(org_a)
    .fetch_one(&admin)
    .await
    .expect("fresh");
    assert_eq!(fresh, 1);
}
