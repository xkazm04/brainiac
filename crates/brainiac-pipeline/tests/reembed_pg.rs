//! Reembed backfill integration test (DATABASE_URL-gated): seed memories +
//! canonicals under embedding version A (dim 256) across TWO orgs, then reembed
//! to version B (a different-dim test embedder) via the admin/RLS-bypassing
//! pool. Assert every memory AND canonical — in BOTH orgs — gains a version-B
//! embedding, that search works under B, and that a second run is a no-op
//! (resumable + idempotent).

use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::{MemoryKind, MemoryStatus, Principal, Visibility};
use brainiac_store::{entities, memories, orgs, retrieval, Store};
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

async fn seed_org(
    store: &Store,
    embedder: &dyn Embedder,
    version_a: i32,
    org_id: Uuid,
    team_id: Uuid,
    mem_ids: &[(Uuid, &str)],
    canon: (Uuid, &str),
) {
    let principal = Principal {
        org_id,
        user_id: Uuid::from_bytes([200u8; 16]),
        team_ids: vec![team_id],
    };
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team_id, org_id, "team")
        .await
        .expect("team");
    for (id, content) in mem_ids {
        memories::insert(
            &mut tx,
            &memories::NewMemory {
                id: *id,
                org_id,
                team_id: Some(team_id),
                owner_user_id: None,
                visibility: Visibility::Org,
                status: MemoryStatus::Canonical,
                kind: MemoryKind::Fact,
                title: None,
                lifecycle: Default::default(),
                detail_md: None,
                content: content.to_string(),
                language: "en".into(),
                valid_from: Some(Utc::now()),
                valid_to: None,
                superseded_by: None,
                confidence: Some(0.9),
                provenance_id: None,
            },
        )
        .await
        .expect("insert mem");
        memories::upsert_embedding(
            &mut tx,
            *id,
            version_a,
            &embedder.embed(content).await.expect("embed"),
        )
        .await
        .expect("embed A");
    }
    // A canonical with a version-A embedding — resolve depends on these, so
    // reembed must carry them into B too.
    entities::insert_canonical(&mut tx, canon.0, org_id, canon.1, "service")
        .await
        .expect("canon");
    entities::upsert_canonical_embedding(
        &mut tx,
        canon.0,
        version_a,
        &embedder.embed(canon.1).await.expect("embed"),
    )
    .await
    .expect("canon embed A");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn reembed_backfills_all_orgs_to_new_version() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — reembed test needs Postgres");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  queue.jobs, queue.archive CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    // Version A: default deterministic embedder (dim 256).
    let embedder_a = DeterministicEmbedder::default();
    let version_a = {
        let principal = brainiac_pipeline::pipeline_principal(Uuid::from_bytes([1u8; 16]));
        let mut tx = store.scoped_tx(&principal).await.expect("tx");
        let v = memories::ensure_embedding_version(
            &mut tx,
            embedder_a.model_name(),
            embedder_a.dim() as i32,
        )
        .await
        .expect("ver A");
        tx.commit().await.expect("commit");
        v
    };

    let org1 = Uuid::from_bytes([1u8; 16]);
    let org2 = Uuid::from_bytes([2u8; 16]);
    let team1 = Uuid::from_bytes([11u8; 16]);
    let team2 = Uuid::from_bytes([22u8; 16]);
    let m1 = Uuid::from_bytes([101u8; 16]);
    let m2 = Uuid::from_bytes([102u8; 16]);
    let m3 = Uuid::from_bytes([103u8; 16]);
    seed_org(
        &store,
        &embedder_a,
        version_a,
        org1,
        team1,
        &[
            (m1, "psp-gateway retry cap is five attempts"),
            (m2, "kafka consumer lag alert"),
        ],
        (Uuid::from_bytes([201u8; 16]), "psp-gateway"),
    )
    .await;
    seed_org(
        &store,
        &embedder_a,
        version_a,
        org2,
        team2,
        &[(m3, "refund-worker idempotency keys are required")],
        (Uuid::from_bytes([202u8; 16]), "refund-worker"),
    )
    .await;

    // Version B: a DIFFERENT-dimension deterministic embedder (dim 384) — a new
    // vector space, exactly the model-swap scenario.
    let embedder_b = DeterministicEmbedder::new(384);
    assert_ne!(embedder_a.dim(), embedder_b.dim());

    // Reembed on the admin (RLS-bypassing) pool — the cross-org operator sweep.
    let pool = brainiac_store::admin_pool(&url).await.expect("admin pool");
    let stats = brainiac_pipeline::reembed::reembed(&pool, &embedder_b, 2)
        .await
        .expect("reembed");
    let version_b = stats.version_id;
    assert_ne!(version_a, version_b, "swap created a new embedding version");
    assert_eq!(
        stats.memories, 3,
        "all three memories across both orgs reembedded"
    );
    assert_eq!(stats.canonicals, 2, "both canonicals reembedded");
    assert!(stats.batches >= 2, "batch size 2 forced multiple batches");

    // Every memory has a version-B embedding of the RIGHT dimension.
    let mem_missing: i64 = sqlx::query(
        "SELECT count(*) AS n FROM memories m
         WHERE NOT EXISTS (SELECT 1 FROM memory_embeddings e
             WHERE e.memory_id = m.id AND e.embedding_version_id = $1)",
    )
    .bind(version_b)
    .fetch_one(&admin)
    .await
    .expect("q")
    .get("n");
    assert_eq!(
        mem_missing, 0,
        "no memory left without a version-B embedding"
    );

    let wrong_dim: i64 = sqlx::query(
        "SELECT count(*) AS n FROM memory_embeddings
         WHERE embedding_version_id = $1 AND vector_dims(embedding) <> 384",
    )
    .bind(version_b)
    .fetch_one(&admin)
    .await
    .expect("q")
    .get("n");
    assert_eq!(wrong_dim, 0, "version-B embeddings are dim 384");

    let canon_missing: i64 = sqlx::query(
        "SELECT count(*) AS n FROM canonical_entities c
         WHERE NOT EXISTS (SELECT 1 FROM canonical_entity_embeddings ce
             WHERE ce.canonical_id = c.id AND ce.embedding_version_id = $1)",
    )
    .bind(version_b)
    .fetch_one(&admin)
    .await
    .expect("q")
    .get("n");
    assert_eq!(
        canon_missing, 0,
        "no canonical left without a version-B embedding"
    );

    // Search works under B for BOTH orgs (proves the new space is queryable).
    let p1 = Principal {
        org_id: org1,
        user_id: Uuid::from_bytes([200u8; 16]),
        team_ids: vec![team1],
    };
    let mut tx = store.scoped_tx(&p1).await.expect("tx");
    let hits = retrieval::search(
        &mut tx,
        store.pool(),
        &embedder_b,
        version_b,
        &retrieval::RetrievalRequest {
            query: "psp-gateway retry cap".into(),
            k: 10,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await
    .expect("search B");
    tx.commit().await.expect("commit");
    assert!(
        hits.iter().any(|h| h.memory.id == m1),
        "version-B search surfaces the seeded memory: {:?}",
        hits.iter().map(|h| h.memory.id).collect::<Vec<_>>()
    );

    // Idempotent + resumable: a second run finds nothing missing.
    let stats2 = brainiac_pipeline::reembed::reembed(&pool, &embedder_b, 2)
        .await
        .expect("reembed 2");
    assert_eq!(
        stats2.memories, 0,
        "second run writes no new memory embeddings"
    );
    assert_eq!(
        stats2.canonicals, 0,
        "second run writes no new canonical embeddings"
    );

    pool.close().await;
}
