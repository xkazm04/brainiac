//! Store integration tests — require a live Postgres (docker compose up).
//! Skipped (with a loud note) when DATABASE_URL is unset, so the pure crates
//! stay testable without Docker.
//!
//! What must hold here and nowhere less:
//! - RLS visibility matrix through the app role (org / team / private).
//! - The pgvector scan and FTS scan inherit RLS — no leak at the SQL layer.
//! - Queue: claim invisibility, crash-redelivery, dead-lettering.

use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::{
    Enforcement, LibraryArtifactKind, LibraryUsageEvent, MemoryKind, MemoryStatus, Principal,
    StandardLifecycle, StandardProvenanceKind, Visibility,
};
use brainiac_store::{entities, feedback, library, memories, orgs, queue, retrieval, Store};
use uuid::Uuid;

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

fn uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
}

async fn setup() -> Option<(Ctx, tokio::sync::MutexGuard<'static, ()>)> {
    let Some(url) = database_url() else {
        eprintln!("SKIP: DATABASE_URL not set — store integration tests need Postgres");
        return None;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin pool");
    // Idempotent replay: wipe tenant data (order-insensitive via CASCADE).
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive,
                  practice_divergences, standards, standard_versions, standard_provenance,
                  skills, skill_versions, library_usage_events
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");
    let store = Store::connect(&url).await.expect("store connect");
    Some((Ctx { store, admin }, guard))
}

fn org() -> Uuid {
    uuid(1)
}

fn pay_dev() -> Principal {
    Principal {
        org_id: org(),
        user_id: uuid(11),
        team_ids: vec![uuid(21)],
    }
}

fn data_analyst() -> Principal {
    Principal {
        org_id: org(),
        user_id: uuid(12),
        team_ids: vec![uuid(22)],
    }
}

async fn seed(ctx: &Ctx) {
    let admin_principal = pay_dev(); // any principal with the right org for writes
    let mut tx = ctx.store.scoped_tx(&admin_principal).await.expect("tx");
    let c = &mut *tx;

    orgs::upsert_org(c, org(), "meridian-test")
        .await
        .expect("org");
    orgs::upsert_team(c, uuid(21), org(), "payments")
        .await
        .expect("team");
    orgs::upsert_team(c, uuid(22), org(), "data")
        .await
        .expect("team");
    orgs::upsert_user(c, uuid(11), org(), "pay@x")
        .await
        .expect("user");
    orgs::upsert_user(c, uuid(12), org(), "data@x")
        .await
        .expect("user");
    orgs::upsert_member(c, uuid(21), uuid(11), "member")
        .await
        .expect("member");
    orgs::upsert_member(c, uuid(22), uuid(12), "member")
        .await
        .expect("member");

    let mk = |id: u8, team: u8, vis: Visibility, owner: Option<Uuid>, content: &str| {
        memories::NewMemory {
            id: uuid(id),
            org_id: org(),
            team_id: Some(uuid(team)),
            owner_user_id: owner,
            visibility: vis,
            status: MemoryStatus::Canonical,
            kind: MemoryKind::Fact,
            title: None,
            lifecycle: Default::default(),
            detail_md: None,
            content: content.to_string(),
            language: "en".into(),
            valid_from: None,
            valid_to: None,
            superseded_by: None,
            confidence: None,
            provenance_id: None,
        }
    };

    // org-visible, payments-team-visible, private-to-pay-dev, data-team-visible
    memories::insert(
        c,
        &mk(101, 21, Visibility::Org, None, "org wide payment standard"),
    )
    .await
    .expect("m101");
    memories::insert(
        c,
        &mk(
            102,
            21,
            Visibility::Team,
            None,
            "payments webhook signing secret runbook",
        ),
    )
    .await
    .expect("m102");
    memories::insert(
        c,
        &mk(
            103,
            21,
            Visibility::Private,
            Some(uuid(11)),
            "personal sandbox key note",
        ),
    )
    .await
    .expect("m103");
    memories::insert(
        c,
        &mk(
            104,
            22,
            Visibility::Team,
            None,
            "data feature store latency numbers",
        ),
    )
    .await
    .expect("m104");

    // Embeddings: orthogonal one-hot-ish vectors so ANN ordering is trivial.
    let version = memories::ensure_embedding_version(c, "test-model", 4)
        .await
        .expect("ver");
    memories::upsert_embedding(c, uuid(101), version, &[1.0, 0.0, 0.0, 0.0])
        .await
        .expect("e101");
    memories::upsert_embedding(c, uuid(102), version, &[0.9, 0.1, 0.0, 0.0])
        .await
        .expect("e102");
    memories::upsert_embedding(c, uuid(103), version, &[0.8, 0.2, 0.0, 0.0])
        .await
        .expect("e103");
    memories::upsert_embedding(c, uuid(104), version, &[0.7, 0.3, 0.0, 0.0])
        .await
        .expect("e104");

    tx.commit().await.expect("commit seed");
}

#[tokio::test]
async fn rls_visibility_matrix_and_search_leaks() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    // pay dev: sees org + own team + own private; NOT data team.
    let mut tx = ctx.store.scoped_tx(&pay_dev()).await.expect("tx");
    let ids: Vec<Uuid> = vec![uuid(101), uuid(102), uuid(103), uuid(104)];
    let visible = memories::get_by_ids(&mut tx, &ids).await.expect("get");
    let got: Vec<Uuid> = visible.iter().map(|m| m.id).collect();
    assert!(got.contains(&uuid(101)), "org visible");
    assert!(got.contains(&uuid(102)), "team visible");
    assert!(got.contains(&uuid(103)), "own private visible");
    assert!(!got.contains(&uuid(104)), "other team must be filtered");
    drop(tx);

    // data analyst: org only (plus their team's row).
    let mut tx = ctx.store.scoped_tx(&data_analyst()).await.expect("tx");
    let visible = memories::get_by_ids(&mut tx, &ids).await.expect("get");
    let got: Vec<Uuid> = visible.iter().map(|m| m.id).collect();
    assert_eq!(got.len(), 2, "org row + data team row only, got {got:?}");
    assert!(got.contains(&uuid(101)));
    assert!(got.contains(&uuid(104)));

    // Vector search as data analyst: the payments-team and private rows must
    // never surface even though their vectors are the nearest neighbors.
    let version = memories::ensure_embedding_version(&mut tx, "test-model", 4)
        .await
        .expect("ver");
    let hits = memories::search_vector(
        &mut tx,
        version,
        &[1.0, 0.0, 0.0, 0.0],
        10,
        &Default::default(),
    )
    .await
    .expect("ann");
    let hit_ids: Vec<Uuid> = hits.iter().map(|(id, _)| *id).collect();
    assert!(hit_ids.contains(&uuid(101)));
    assert!(
        !hit_ids.contains(&uuid(102)),
        "ANN leaked a team-private row"
    );
    assert!(!hit_ids.contains(&uuid(103)), "ANN leaked a private row");

    // FTS as data analyst: same invariant.
    let hits = memories::search_fts(&mut tx, "webhook signing secret", 10, &Default::default())
        .await
        .expect("fts");
    assert!(
        hits.iter().all(|(id, _)| *id != uuid(102)),
        "FTS leaked a team-private row"
    );
    drop(tx);

    // Owner isolation: a DIFFERENT payments user must not see 103. Simulate
    // with a principal in the same team but another user id.
    let teammate = Principal {
        org_id: org(),
        user_id: uuid(13),
        team_ids: vec![uuid(21)],
    };
    let mut tx = ctx.store.scoped_tx(&teammate).await.expect("tx");
    let visible = memories::get_by_ids(&mut tx, &[uuid(103)])
        .await
        .expect("get");
    assert!(
        visible.is_empty(),
        "private row visible to non-owner teammate"
    );
}

#[tokio::test]
async fn identifier_query_ranks_exact_match_first() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await; // org / teams / users
    let p = pay_dev();
    let embedder = DeterministicEmbedder::default();

    // Two org-visible memories: one carries the exact identifier, the other is
    // about the same topic (parsing/validation) without the literal token.
    let exact = "the zeta-parser-9000 service validates inbound payment schemas";
    let distractor = "our parser service validates inbound payment message shapes";

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;
    let mk = |id: u8, content: &str| memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(21)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: None,
        provenance_id: None,
    };
    memories::insert(c, &mk(160, exact)).await.expect("m160");
    memories::insert(c, &mk(161, distractor))
        .await
        .expect("m161");
    let ver = memories::ensure_embedding_version(c, embedder.model_name(), embedder.dim() as i32)
        .await
        .expect("ver");
    memories::upsert_embedding(c, uuid(160), ver, &embedder.embed_sync(exact))
        .await
        .expect("e160");
    memories::upsert_embedding(c, uuid(161), ver, &embedder.embed_sync(distractor))
        .await
        .expect("e161");
    // Must commit before search: the candidate retrievers run on separate
    // pooled connections and would not see this tx's uncommitted rows.
    tx.commit().await.expect("commit");

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let hits = retrieval::search(
        &mut tx,
        ctx.store.pool(),
        &embedder,
        ver,
        &retrieval::RetrievalRequest {
            query: "zeta-parser-9000".into(),
            k: 10,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await
    .expect("search");
    assert!(
        !hits.is_empty() && hits[0].memory.id == uuid(160),
        "exact identifier match must rank first, got {:?}",
        hits.iter().map(|h| h.memory.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn stage5_reranker_reorders_survivors() {
    // Stage 5 (ARCHITECTURE.md §4): the reranker rescores the assembled
    // survivors BEFORE the blend. Two org-visible memories are shaped so the
    // baseline (bi-encoder) and a query-token-overlap reranker DISAGREE.
    // Neither memory contains ALL of the query's tokens, so the AND-semantics
    // FTS query matches neither — the baseline is pure bi-encoder cosine:
    //   A — "alpha": a single query token, no dilution ⇒ high cosine (1/√3) ⇒
    //       baseline #1, but only 1 of 3 query tokens (reranker 1/3).
    //   B — two query tokens ("beta gamma") buried in filler ⇒ the filler
    //       dilutes its cosine below A's, but it covers 2 of 3 query tokens
    //       (reranker 2/3). The reranker is query-normalized, so B's filler
    //       doesn't lower its rerank score.
    // Baseline top must be A; flip the reranker on and B (more query tokens
    // covered) must take the top — proving reordering flows through stage 5.
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;
    let p = pay_dev();
    let embedder = DeterministicEmbedder::default();

    let query = "alpha beta gamma";
    let a = "alpha";
    let b = "beta gamma delta epsilon zeta eta theta iota";

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;
    let mk = |id: u8, content: &str| memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(21)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: None,
        provenance_id: None,
    };
    memories::insert(c, &mk(170, a)).await.expect("m170");
    memories::insert(c, &mk(171, b)).await.expect("m171");
    let ver = memories::ensure_embedding_version(c, embedder.model_name(), embedder.dim() as i32)
        .await
        .expect("ver");
    memories::upsert_embedding(c, uuid(170), ver, &embedder.embed_sync(a))
        .await
        .expect("e170");
    memories::upsert_embedding(c, uuid(171), ver, &embedder.embed_sync(b))
        .await
        .expect("e171");
    tx.commit().await.expect("commit");

    let req = || retrieval::RetrievalRequest {
        query: query.into(),
        k: 10,
        as_of: None,
        filters: Default::default(),
    };

    // Baseline (reranker off): A ranks first.
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let base = retrieval::search(&mut tx, ctx.store.pool(), &embedder, ver, &req())
        .await
        .expect("baseline search");
    tx.commit().await.expect("commit");
    assert_eq!(
        base.first().map(|h| h.memory.id),
        Some(uuid(170)),
        "baseline (no reranker) ranks the tight subset match first, got {:?}",
        base.iter().map(|h| h.memory.id).collect::<Vec<_>>()
    );

    // Reranker on: the full-overlap memory B is promoted to the top.
    let reranker = brainiac_core::rerank::LexicalOverlapReranker;
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let reranked = retrieval::search_reranked(
        &mut tx,
        ctx.store.pool(),
        &embedder,
        Some(&reranker),
        ver,
        &req(),
    )
    .await
    .expect("reranked search");
    tx.commit().await.expect("commit");
    assert_eq!(
        reranked.first().map(|h| h.memory.id),
        Some(uuid(171)),
        "stage-5 reranker promotes the full-overlap memory, got {:?}",
        reranked.iter().map(|h| h.memory.id).collect::<Vec<_>>()
    );
    // Both memories are still present — rerank reorders, it does not drop.
    assert!(
        reranked.iter().any(|h| h.memory.id == uuid(170)),
        "reranker reorders without dropping candidates"
    );
}

#[tokio::test]
async fn ensure_version_auto_creates_hnsw_index_for_new_dim() {
    // Dim-agnostic ANN (0012): a bake-off model at a dimension 0006 never
    // hard-coded (here 768) must get its partial HNSW index the moment its
    // embedding version is ensured — no more silent seq-scan. And search over
    // that dimension must return the right hit through the new index.
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;
    let p = pay_dev();
    let embedder = DeterministicEmbedder::new(768);
    assert_eq!(embedder.dim(), 768);

    // Clean precondition: indexes survive TRUNCATE (and a prior run of this
    // test would have created this one), so drop it as the owner to prove the
    // ensure path recreates it. DROP INDEX needs owner rights — the admin pool.
    sqlx::query("DROP INDEX IF EXISTS idx_memory_embeddings_hnsw_768")
        .execute(&ctx.admin)
        .await
        .expect("drop 768 index");

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;

    // Precondition: no 768-d index yet (dropped above; 0006's 256/1024 and
    // 0012's re-assertion of those two are the only HNSW indexes).
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_indexes WHERE indexname = $1")
        .bind("idx_memory_embeddings_hnsw_768")
        .fetch_one(&mut *c)
        .await
        .expect("count before");
    assert_eq!(before, 0, "768-d index must not pre-exist");

    let content = "the omega-indexer-768 service reconciles ledger snapshots nightly";
    memories::insert(
        c,
        &memories::NewMemory {
            id: uuid(180),
            org_id: org(),
            team_id: Some(uuid(21)),
            owner_user_id: None,
            visibility: Visibility::Org,
            status: MemoryStatus::Canonical,
            kind: MemoryKind::Fact,
            title: None,
            lifecycle: Default::default(),
            detail_md: None,
            content: content.to_string(),
            language: "en".into(),
            valid_from: None,
            valid_to: None,
            superseded_by: None,
            confidence: None,
            provenance_id: None,
        },
    )
    .await
    .expect("m180");
    // Ensuring the 768-d version must auto-create the matching partial index
    // (through the SECURITY DEFINER function — brainiac_app itself can't DDL).
    let ver = memories::ensure_embedding_version(c, embedder.model_name(), 768)
        .await
        .expect("ver768");
    memories::upsert_embedding(c, uuid(180), ver, &embedder.embed_sync(content))
        .await
        .expect("e180");

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_indexes WHERE indexname = $1")
        .bind("idx_memory_embeddings_hnsw_768")
        .fetch_one(&mut *c)
        .await
        .expect("count after");
    assert_eq!(
        after, 1,
        "ensure_embedding_version(768) must create the index"
    );
    tx.commit().await.expect("commit");

    // Search over the 768-d version returns the seeded memory (index or not,
    // the scan is correct — this proves the dimension is fully wired).
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let hits = retrieval::search(
        &mut tx,
        ctx.store.pool(),
        &embedder,
        ver,
        &retrieval::RetrievalRequest {
            query: "omega-indexer-768 ledger reconcile".into(),
            k: 10,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await
    .expect("search 768");
    assert!(
        hits.iter().any(|h| h.memory.id == uuid(180)),
        "768-d search must surface the seeded memory, got {:?}",
        hits.iter().map(|h| h.memory.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn czech_fts_honors_language_config() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    // A Czech, org-visible memory under the payments team. Under the old
    // english-only index its distinctive Czech words would be english-stemmed
    // (and any that collide with english stopwords dropped); the language-aware
    // index (0007) builds it under the 'simple' config instead.
    let p = pay_dev();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let cs = memories::NewMemory {
        id: uuid(150),
        org_id: org(),
        team_id: Some(uuid(21)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: "nasazení nové platební služby do produkčního prostředí vyžaduje schválení".into(),
        language: "cs".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: None,
        provenance_id: None,
    };
    memories::insert(&mut tx, &cs).await.expect("insert cs");
    tx.commit().await.expect("commit");

    // Search a Czech phrase: the cs memory must surface via the 'simple' query.
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let hits = memories::search_fts(&mut tx, "nasazení produkčního", 10, &Default::default())
        .await
        .expect("fts cs");
    assert!(
        hits.iter().any(|(id, _)| *id == uuid(150)),
        "Czech memory must be retrievable by a Czech phrase, got {hits:?}"
    );

    // English retrieval still works through the same call path.
    let hits = memories::search_fts(&mut tx, "webhook signing secret", 10, &Default::default())
        .await
        .expect("fts en");
    assert!(
        hits.iter().any(|(id, _)| *id == uuid(102)),
        "English memory still retrievable, got {hits:?}"
    );
}

#[tokio::test]
async fn graph_neighbors_cross_canonical_bridge() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    let p = pay_dev();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;

    // Two raw entities in different teams linked to one canonical.
    entities::insert_entity(
        c,
        uuid(31),
        org(),
        Some(uuid(21)),
        "Kafka",
        "tech",
        &[],
        None,
    )
    .await
    .expect("e31");
    entities::insert_entity(
        c,
        uuid(32),
        org(),
        Some(uuid(22)),
        "the event bus",
        "tech",
        &[],
        None,
    )
    .await
    .expect("e32");
    entities::insert_canonical(c, uuid(41), org(), "kafka", "tech")
        .await
        .expect("c41");
    entities::link(c, uuid(31), uuid(41), 0.95, "human", None)
        .await
        .expect("l1");
    entities::link(c, uuid(32), uuid(41), 0.95, "human", None)
        .await
        .expect("l2");
    // And one edge hop from the sibling.
    entities::insert_entity(
        c,
        uuid(33),
        org(),
        Some(uuid(22)),
        "event-lake",
        "repo",
        &[],
        None,
    )
    .await
    .expect("e33");
    entities::insert_edge(c, uuid(51), org(), uuid(32), uuid(33), "uses", None)
        .await
        .expect("edge");

    let one_hop = entities::neighbors(c, &[uuid(31)], 1, 50)
        .await
        .expect("hop1");
    assert!(
        one_hop.contains(&uuid(32)),
        "canonical sibling reachable in 1 hop"
    );
    assert!(
        !one_hop.contains(&uuid(33)),
        "edge neighbor of sibling needs 2 hops"
    );

    let two_hop = entities::neighbors(c, &[uuid(31)], 2, 50)
        .await
        .expect("hop2");
    assert!(two_hop.contains(&uuid(32)));
    assert!(
        two_hop.contains(&uuid(33)),
        "2-hop reaches the sibling's edge neighbor"
    );

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn graph_surfaced_hit_scores_and_carries_anchors() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await; // org / teams / users
    let p = pay_dev();
    let embedder = DeterministicEmbedder::default();
    let query = "kafka retry semantics for payment events";

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;

    // Two raw entities in different teams bridged by one canonical: payments'
    // "Kafka" and data's "the event bus".
    entities::insert_entity(
        c,
        uuid(31),
        org(),
        Some(uuid(21)),
        "Kafka",
        "tech",
        &[],
        None,
    )
    .await
    .expect("e31");
    entities::insert_entity(
        c,
        uuid(32),
        org(),
        Some(uuid(22)),
        "the event bus",
        "tech",
        &[],
        None,
    )
    .await
    .expect("e32");
    entities::insert_canonical(c, uuid(41), org(), "kafka", "tech")
        .await
        .expect("c41");
    entities::link(c, uuid(31), uuid(41), 0.95, "human", None)
        .await
        .expect("l31");
    entities::link(c, uuid(32), uuid(41), 0.95, "human", None)
        .await
        .expect("l32");

    let mk = |id: u8, team: u8, content: &str| memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(team)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: None,
        provenance_id: None,
    };
    // Direct hit (payments): matches the query, anchored to Kafka, embedded so
    // it leads the candidate lists and seeds graph expansion from entity 31.
    let direct = "kafka retry semantics for payment events and idempotent consumers";
    memories::insert(c, &mk(180, 21, direct))
        .await
        .expect("m180");
    memories::link_entity(c, uuid(180), uuid(31))
        .await
        .expect("le180");
    // Graph-only hit (data team): anchored to the sibling entity, but with NO
    // embedding and content that shares no query terms — so it is absent from
    // BOTH candidate retrievers and can only surface by walking the canonical
    // bridge Kafka↔event-bus. This is the cross-team hit that used to sink at
    // score 0.0.
    let cross = "the observability runbook for consumer lag dashboards and paging";
    memories::insert(c, &mk(181, 22, cross))
        .await
        .expect("m181");
    memories::link_entity(c, uuid(181), uuid(32))
        .await
        .expect("le181");

    let ver = memories::ensure_embedding_version(c, embedder.model_name(), embedder.dim() as i32)
        .await
        .expect("ver");
    // Only the direct hit gets an embedding (the exact query vector).
    memories::upsert_embedding(c, uuid(180), ver, &embedder.embed_sync(query))
        .await
        .expect("e180");
    tx.commit().await.expect("commit");

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let hits = retrieval::search(
        &mut tx,
        ctx.store.pool(),
        &embedder,
        ver,
        &retrieval::RetrievalRequest {
            query: query.into(),
            k: 10,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await
    .expect("search");

    let direct_hit = hits
        .iter()
        .find(|h| h.memory.id == uuid(180))
        .expect("direct hit present");
    assert!(!direct_hit.via_graph, "180 is a direct hit");
    assert!(
        direct_hit
            .anchors
            .iter()
            .any(|a| a.id == uuid(41) && a.name == "kafka"),
        "direct hit carries its canonical anchor, got {:?}",
        direct_hit.anchors
    );

    let graph_hit = hits
        .iter()
        .find(|h| h.memory.id == uuid(181))
        .expect("cross-team memory surfaced via graph");
    assert!(graph_hit.via_graph, "181 surfaced only through the graph");
    assert!(
        graph_hit.score > 0.0,
        "graph-surfaced hit must carry a real (non-zero) score, got {}",
        graph_hit.score
    );
    assert!(
        graph_hit.score < direct_hit.score,
        "one hop removed: graph hit sits below its anchoring direct hit"
    );
    assert!(
        graph_hit
            .anchors
            .iter()
            .any(|a| a.id == uuid(41) && a.name == "kafka"),
        "graph hit carries the bridging canonical anchor, got {:?}",
        graph_hit.anchors
    );
}

#[tokio::test]
async fn queue_claim_redeliver_and_dead_letter() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    let pool = ctx.store.pool();

    let payload = serde_json::json!({"task": "extract", "source": "src-1"});
    queue::send(pool, "extract", &payload).await.expect("send");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 1);

    // Claim makes it invisible to a second reader.
    let claimed = queue::read(pool, "extract", 5, 30).await.expect("read");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].payload["task"], "extract");
    let second = queue::read(pool, "extract", 5, 30).await.expect("read2");
    assert!(second.is_empty(), "claimed job must be invisible");

    // Zero-second visibility = immediate redelivery (crash simulation).
    queue::fail(pool, &claimed[0], 0).await.expect("fail");
    let redelivered = queue::read(pool, "extract", 5, 30).await.expect("read3");
    assert_eq!(redelivered.len(), 1, "failed job redelivers");
    assert!(redelivered[0].attempts >= 2);

    // Exhaust attempts via fail() → 'failed' (adjudicated: the worker reported
    // the error every time until the budget ran out; distinct from crash-poison
    // 'dead', which claim-time reaping produces — see queue_reaps_crash_poison).
    let mut job = redelivered[0].clone();
    job.attempts = queue::MAX_ATTEMPTS;
    let retrying = queue::fail(pool, &job, 0).await.expect("failed");
    assert!(!retrying, "exhausted job must not retry");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 0);

    // Success path archives cleanly too.
    queue::send(pool, "extract", &payload).await.expect("send2");
    let claimed = queue::read(pool, "extract", 1, 30).await.expect("read4");
    queue::complete(pool, &claimed[0]).await.expect("complete");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 0);

    // Health reflects the story so far: nothing live, one ok, one adjudicated
    // failure, zero crash-poison (no job ever went unacked past the ceiling).
    let h = queue::health(pool, "extract").await.expect("health");
    assert_eq!((h.ready, h.in_flight), (0, 0));
    assert_eq!(h.archived_ok, 1);
    assert_eq!(h.archived_failed, 1);
    assert_eq!(h.dead_letters, 0);

    // Dead-letter inspection + requeue round trip. dead_letters() is the
    // operator recovery surface: it lists BOTH terminal outcomes, so the
    // 'failed' job above appears here.
    let dead = queue::dead_letters(pool, "extract", 10, 0)
        .await
        .expect("dl");
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].payload["task"], "extract");
    // Archive rows carry the DB-recorded claim count (the dead-letter above
    // was forced by forging attempts locally, so only the real claims show).
    assert!(dead[0].attempts >= 1);
    let requeued = queue::requeue_dead(pool, dead[0].id)
        .await
        .expect("requeue");
    assert!(requeued, "dead letter must requeue");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 1);
    let back = queue::read(pool, "extract", 1, 30).await.expect("read5");
    assert_eq!(back[0].id, dead[0].id, "requeue preserves the job id");
    assert_eq!(back[0].attempts, 1, "attempt budget resets");
    assert!(
        !queue::requeue_dead(pool, dead[0].id)
            .await
            .expect("requeue2"),
        "second requeue of the same id is a no-op"
    );
    queue::complete(pool, &back[0]).await.expect("complete2");

    // Keep admin pool alive till the end (unused otherwise).
    let _ = &ctx.admin;
}

/// Direction 1: a crash-poison job — one that crashes the worker before it can
/// ever call fail() — is reaped to the dead-letter archive at claim time
/// instead of being redelivered forever. Simulated by claiming repeatedly with
/// zero visibility and never acking, past the attempt budget.
#[tokio::test]
async fn queue_reaps_crash_poison() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    let pool = ctx.store.pool();

    queue::send(pool, "reap", &serde_json::json!({"task": "crasher"}))
        .await
        .expect("send");

    // Claim over and over WITHOUT ever completing or failing — this is exactly
    // what a worker that panics/crashes mid-job looks like to the queue: each
    // claim bumps attempts, nothing acks, the visibility window lapses (0s),
    // and it comes right back. Reaping must terminate this.
    let mut reaped = false;
    let mut last_seen_attempts = 0;
    for _ in 0..(queue::MAX_ATTEMPTS + 3) {
        let jobs = queue::read(pool, "reap", 1, 0).await.expect("read");
        match jobs.into_iter().next() {
            Some(job) => last_seen_attempts = job.attempts,
            None => {
                reaped = true;
                break;
            }
        }
    }
    assert!(reaped, "crash-poison job must stop being redelivered");
    assert!(
        last_seen_attempts >= queue::MAX_ATTEMPTS,
        "job was delivered its full budget before reaping, got {last_seen_attempts}"
    );
    assert_eq!(
        queue::depth(pool, "reap").await.expect("depth"),
        0,
        "no live job remains after reaping"
    );

    // It landed in dead-letter as crash-poison ('dead'), NOT as an adjudicated
    // 'failed' — fail() was never called.
    let h = queue::health(pool, "reap").await.expect("health");
    assert_eq!(h.dead_letters, 1, "reaped as crash-poison");
    assert_eq!(h.archived_failed, 0, "never adjudicated via fail()");
    assert_eq!(h.archived_ok, 0);

    // And it's recoverable through the same operator surface.
    let dl = queue::dead_letters(pool, "reap", 10, 0).await.expect("dl");
    assert_eq!(dl.len(), 1);
    assert_eq!(dl[0].payload["task"], "crasher");

    let _ = &ctx.admin;
}

/// Direction 2: per-org fair claiming. Org A floods the queue with 100 jobs,
/// then org B enqueues a single job afterwards. Under the old strict-FIFO
/// (ORDER BY id) claim, org B would wait behind all 100 of org A's jobs. Fair
/// claiming ranks per org, so org B's one job rides in the FIRST claimed batch —
/// while FIFO WITHIN org A is still preserved.
#[tokio::test]
async fn queue_fair_claiming_interleaves_orgs() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    let pool = ctx.store.pool();

    let org_a = "11111111-1111-1111-1111-111111111111";
    let org_b = "22222222-2222-2222-2222-222222222222";

    // Org A floods 100 jobs first, then org B enqueues exactly one.
    let mut a_ids = Vec::new();
    for i in 0..100 {
        let id = queue::send(
            pool,
            "fair",
            &serde_json::json!({"org_id": org_a, "seq": i}),
        )
        .await
        .expect("send a");
        a_ids.push(id);
    }
    let b_id = queue::send(pool, "fair", &serde_json::json!({"org_id": org_b}))
        .await
        .expect("send b");

    // First claimed batch of 10.
    let batch = queue::read(pool, "fair", 10, 30).await.expect("read");
    let claimed_ids: Vec<i64> = batch.iter().map(|j| j.id).collect();
    assert!(
        claimed_ids.contains(&b_id),
        "org B's job must ride in the FIRST batch, not queue behind org A's flood: {claimed_ids:?}"
    );

    // Within-org FIFO preserved: fairness only reorders ACROSS orgs — the org A
    // jobs that got claimed are exactly its OLDEST (lowest-id) ones, never a
    // later job jumping ahead of an earlier same-org one. (RETURNING order from
    // an UPDATE is unspecified, so compare the claimed SET, sorted.)
    let mut claimed_a: Vec<i64> = batch
        .iter()
        .filter(|j| j.payload["org_id"] == org_a)
        .map(|j| j.id)
        .collect();
    claimed_a.sort();
    let mut oldest_a = a_ids.clone();
    oldest_a.sort();
    oldest_a.truncate(claimed_a.len());
    assert_eq!(
        claimed_a, oldest_a,
        "org A's claimed jobs are its oldest N, in FIFO id order (no later job jumps ahead)"
    );
    let oldest_a_id = a_ids.iter().min().copied().expect("org A has jobs");
    assert!(
        claimed_a.contains(&oldest_a_id),
        "org A's very oldest job is among the first claimed"
    );

    let _ = &ctx.admin;
}

/// Direction 2: the fair-claiming rewrite keeps FOR UPDATE SKIP LOCKED correct —
/// two workers reading the same queue at the same instant must never both claim
/// the same job. Fire many concurrent readers at a small pool of jobs and assert
/// each job is claimed by exactly one reader.
#[tokio::test]
async fn queue_concurrent_readers_never_double_claim() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    let pool = ctx.store.pool();

    // A mix of orgs so the ranking subquery is exercised, not just one bucket.
    let mut ids = std::collections::HashSet::new();
    for i in 0..40 {
        let org = format!("org-{}", i % 4);
        let id = queue::send(pool, "race", &serde_json::json!({"org_id": org, "n": i}))
            .await
            .expect("send");
        ids.insert(id);
    }

    // Eight readers claim concurrently; SKIP LOCKED must partition the jobs.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut claimed = Vec::new();
            loop {
                let jobs = queue::read(&pool, "race", 5, 30).await.expect("read");
                if jobs.is_empty() {
                    break;
                }
                claimed.extend(jobs.into_iter().map(|j| j.id));
            }
            claimed
        }));
    }

    let mut all_claimed = Vec::new();
    for h in handles {
        all_claimed.extend(h.await.expect("join"));
    }

    // Every job claimed exactly once: no duplicates, none lost.
    let unique: std::collections::HashSet<i64> = all_claimed.iter().copied().collect();
    assert_eq!(
        all_claimed.len(),
        unique.len(),
        "no job claimed twice under concurrent readers (double-claim)"
    );
    assert_eq!(unique, ids, "every job claimed exactly once, none lost");

    let _ = &ctx.admin;
}

#[tokio::test]
async fn ttl_expiring_queue_and_reverification() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;
    let p = pay_dev();

    // Age one canonical memory to the edge of its window: expires in 5 days.
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    sqlx::query(
        "UPDATE memories SET valid_from = now() - interval '360 days',
                             valid_to = now() + interval '5 days'
         WHERE id = $1",
    )
    .bind(uuid(102))
    .execute(&mut *tx)
    .await
    .expect("age");
    tx.commit().await.expect("commit");

    // Within a 30-day horizon it shows; within 1 day it doesn't.
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let soon = memories::expiring(&mut tx, 30, 50).await.expect("expiring");
    assert!(
        soon.iter().any(|m| m.id == uuid(102)),
        "5-days-out memory queued"
    );
    let sooner = memories::expiring(&mut tx, 1, 50).await.expect("expiring");
    assert!(
        sooner.iter().all(|m| m.id != uuid(102)),
        "outside 1-day horizon"
    );

    // Re-verify: window extends from NOW, not the old boundary.
    let memories::ExtendOutcome::Extended(new_to) =
        memories::extend_validity(&mut tx, uuid(102), 365)
            .await
            .expect("extend")
    else {
        panic!("row updated");
    };
    assert!(new_to > chrono::Utc::now() + chrono::Duration::days(300));
    let after = memories::expiring(&mut tx, 30, 50).await.expect("expiring");
    assert!(
        after.iter().all(|m| m.id != uuid(102)),
        "re-verified row left the queue"
    );

    // RLS: another org's/team's caller can't extend what they can't see.
    drop(tx);
    let mut tx = ctx.store.scoped_tx(&data_analyst()).await.expect("tx");
    let denied = memories::extend_validity(&mut tx, uuid(103), 365)
        .await
        .expect("query ok");
    assert_eq!(
        denied,
        memories::ExtendOutcome::NotFound,
        "invisible (private) memory must not extend, and must be indistinguishable \
         from a nonexistent one"
    );

    let _ = &ctx.admin;
}

#[tokio::test]
async fn fresh_memory_outranks_stale_near_duplicate() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await; // org / teams / users
    let p = pay_dev();
    let embedder = DeterministicEmbedder::default();
    let query = "zephyr reconciliation cadence";

    // Engineer two memories with EQUAL fused relevance, differing only in age,
    // so the recency nudge is the sole differentiator. Equal RRF can't come
    // from identical rows (SQL tie-break makes them adjacent, not equal); it
    // comes from SWAPPED cross-retriever ranks — each memory wins one list:
    //   STALE: FTS #1 (denser lexical match) + vector #2  → 1/61 + 1/62
    //   FRESH: vector #1 (exact query embedding) + FTS #2 → 1/61 + 1/62
    // Identical fused scores; only created_at (hence recency) differs.
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;
    let mk = |id: u8, content: &str| memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(21)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: None,
        provenance_id: None,
    };
    // STALE wins FTS: repeats every query term (higher term frequency).
    let stale_content = "zephyr reconciliation cadence zephyr reconciliation cadence";
    // FRESH matches the query terms once, diluted with filler → lower ts_rank.
    let fresh_content = "zephyr reconciliation cadence noted with assorted unrelated filler prose";
    memories::insert(c, &mk(170, stale_content))
        .await
        .expect("m170 stale");
    memories::insert(c, &mk(171, fresh_content))
        .await
        .expect("m171 fresh");
    sqlx::query("UPDATE memories SET created_at = now() - interval '400 days' WHERE id = $1")
        .bind(uuid(170))
        .execute(&mut *tx)
        .await
        .expect("age stale");
    sqlx::query("UPDATE memories SET created_at = now() - interval '2 days' WHERE id = $1")
        .bind(uuid(171))
        .execute(&mut *tx)
        .await
        .expect("age fresh");
    let ver =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    // FRESH gets the exact query embedding → cosine distance 0 → vector #1.
    // STALE gets an unrelated embedding → vector #2.
    memories::upsert_embedding(&mut tx, uuid(171), ver, &embedder.embed_sync(query))
        .await
        .expect("e171");
    memories::upsert_embedding(
        &mut tx,
        uuid(170),
        ver,
        &embedder.embed_sync("orthogonal ledger housekeeping unrelated"),
    )
    .await
    .expect("e170");
    tx.commit().await.expect("commit");

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let hits = retrieval::search(
        &mut tx,
        ctx.store.pool(),
        &embedder,
        ver,
        &retrieval::RetrievalRequest {
            query: query.into(),
            k: 10,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await
    .expect("search");
    let order: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
    let pos_fresh = order.iter().position(|id| *id == uuid(171));
    let pos_stale = order.iter().position(|id| *id == uuid(170));
    let (Some(pos_fresh), Some(pos_stale)) = (pos_fresh, pos_stale) else {
        panic!("both near-duplicates should surface, got {order:?}");
    };
    assert!(
        pos_fresh < pos_stale,
        "fresh memory must outrank the stale near-duplicate at equal relevance: \
         fresh@{pos_fresh:?} stale@{pos_stale:?}"
    );
    // The recency edge stays tiebreak-scale: the blended scores are a hair apart.
    let s_fresh = hits[pos_fresh].score;
    let s_stale = hits[pos_stale].score;
    assert!(
        s_fresh > s_stale && (s_fresh - s_stale) < 0.01,
        "recency is a nudge, not a landslide: {s_fresh} vs {s_stale}"
    );
}

#[tokio::test]
async fn feedback_claims_queue_and_resolution() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;
    let p = pay_dev();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");

    // A helpful verdict asserts nothing to fix: it must NOT open a claim.
    feedback::insert(
        &mut tx,
        Uuid::new_v4(),
        org(),
        uuid(101),
        uuid(11),
        "helpful",
        None,
    )
    .await
    .expect("helpful");
    assert!(
        feedback::flagged(&mut tx, 50, 0)
            .await
            .expect("flagged")
            .is_empty(),
        "a helpful verdict must not open a claim"
    );

    // Two negative verdicts against one memory = ONE queue row, counts merged.
    for (verdict, note) in [
        ("wrong", Some("psp changed the endpoint")),
        ("outdated", None),
    ] {
        feedback::insert(
            &mut tx,
            Uuid::new_v4(),
            org(),
            uuid(102),
            uuid(11),
            verdict,
            note,
        )
        .await
        .expect("negative verdict");
    }
    let flagged = feedback::flagged(&mut tx, 50, 0).await.expect("flagged");
    assert_eq!(
        flagged.len(),
        1,
        "one row per disputed memory, not per verdict"
    );
    assert_eq!(flagged[0].memory_id, uuid(102));
    assert_eq!((flagged[0].wrong, flagged[0].outdated), (1, 1));
    assert_eq!(
        flagged[0].notes,
        vec!["psp changed the endpoint".to_string()]
    );
    assert_eq!(feedback::flagged_count(&mut tx).await.expect("count"), 1);

    // Trust attaches to served memories in one batched lookup.
    let trust = feedback::trust_for(&mut tx, &[uuid(101), uuid(102), uuid(104)])
        .await
        .expect("trust");
    assert_eq!(trust[&uuid(101)].helpful, 1);
    assert!(!trust[&uuid(101)].disputed(), "helpful is not a dispute");
    assert!(
        trust[&uuid(102)].disputed(),
        "open claims mark a memory disputed"
    );
    assert!(
        !trust.contains_key(&uuid(104)),
        "un-rated memories carry no trust row"
    );

    // A maintainer answers: dismissed → claims close, memory untouched.
    let closed = feedback::resolve_claims(&mut tx, uuid(102), uuid(11), "dismissed", None)
        .await
        .expect("resolve");
    assert_eq!(closed, 2, "both open claims close together");
    assert!(
        feedback::flagged(&mut tx, 50, 0)
            .await
            .expect("flagged")
            .is_empty(),
        "answered claims leave the queue"
    );
    let trust = feedback::trust_for(&mut tx, &[uuid(102)])
        .await
        .expect("trust");
    assert_eq!(trust[&uuid(102)].wrong, 1, "history is kept");
    assert!(!trust[&uuid(102)].disputed(), "but the dispute is settled");

    // Re-resolving an already-answered memory closes nothing (idempotent).
    let closed = feedback::resolve_claims(&mut tx, uuid(102), uuid(11), "dismissed", None)
        .await
        .expect("resolve again");
    assert_eq!(closed, 0);

    // RLS: the data analyst cannot see claims against a payments-team memory.
    drop(tx);
    let mut tx = ctx.store.scoped_tx(&pay_dev()).await.expect("tx");
    feedback::insert(
        &mut tx,
        Uuid::new_v4(),
        org(),
        uuid(103),
        uuid(11),
        "wrong",
        None,
    )
    .await
    .expect("claim on a private memory");
    tx.commit().await.expect("commit");
    let mut tx = ctx.store.scoped_tx(&data_analyst()).await.expect("tx");
    assert_eq!(
        feedback::flagged_count(&mut tx).await.expect("count"),
        0,
        "claims against invisible memories must not leak into another principal's queue"
    );

    let _ = &ctx.admin;
}

/// The raw-TTL sweep (migration 0024): a raw memory past the TTL flips to
/// `rejected` — dropped from every retrieval path but preserved — and leaves a
/// `promotions` audit row naming the sweep as the actor. Younger raw memories
/// and old-but-reviewed memories are untouched, and a second pass finds nothing
/// (idempotent).
#[tokio::test]
async fn raw_ttl_sweep_expires_only_neglected_raw_and_leaves_an_audit_trail() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    let p = pay_dev();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let c = &mut *tx;
    let mk = |id: u8, status: MemoryStatus, content: &str| memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(21)),
        owner_user_id: None,
        visibility: Visibility::Org,
        status,
        kind: MemoryKind::Fact,
        title: None,
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: Some(0.9),
        provenance_id: None,
    };
    memories::insert(
        c,
        &mk(230, MemoryStatus::Raw, "old raw — declined by neglect"),
    )
    .await
    .expect("old raw");
    memories::insert(
        c,
        &mk(231, MemoryStatus::Raw, "young raw — genuinely pending"),
    )
    .await
    .expect("young raw");
    memories::insert(
        c,
        &mk(
            232,
            MemoryStatus::Canonical,
            "old canonical — age is not neglect once reviewed",
        ),
    )
    .await
    .expect("old canonical");
    tx.commit().await.expect("commit");

    // Age the old pair past the TTL. created_at is not caller-writable, so this
    // goes through the admin pool — the same pool the sweep scheduler runs on.
    sqlx::query("UPDATE memories SET created_at = now() - interval '40 days' WHERE id = ANY($1)")
        .bind(vec![uuid(230), uuid(232)])
        .execute(&ctx.admin)
        .await
        .expect("age");

    let (expired, orgs_touched) = memories::expire_stale_raw(&ctx.admin, 30)
        .await
        .expect("sweep");
    assert_eq!(
        (expired, orgs_touched),
        (1, 1),
        "exactly the old RAW memory expires — not the young raw, not the old canonical"
    );

    let statuses: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status::text FROM memories WHERE id = ANY($1) ORDER BY id")
            .bind(vec![uuid(230), uuid(231), uuid(232)])
            .fetch_all(&ctx.admin)
            .await
            .expect("statuses");
    let status_of = |id: Uuid| {
        statuses
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, s)| s.as_str())
            .unwrap_or("missing")
    };
    assert_eq!(status_of(uuid(230)), "rejected");
    assert_eq!(status_of(uuid(231)), "raw");
    assert_eq!(status_of(uuid(232)), "canonical");

    // "Who decided this belief goes away" must have a named answer.
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM promotions
         WHERE memory_id = $1 AND from_status = 'raw' AND to_status = 'rejected'
           AND policy_decision = 'auto_rejected' AND policy_rule = 'raw_ttl_sweep'",
    )
    .bind(uuid(230))
    .fetch_one(&ctx.admin)
    .await
    .expect("audit");
    assert_eq!(audit, 1, "the expiry must leave exactly one audit row");

    let (again, _) = memories::expire_stale_raw(&ctx.admin, 30)
        .await
        .expect("second sweep");
    assert_eq!(
        again, 0,
        "a second pass finds nothing — rejected is terminal here"
    );
}

// ── LB0: the Library substrate (migration 0028) ─────────────────────────────

/// A principal from a DIFFERENT org. No seeding needed: the point is that the
/// GUC scope alone must wall them off from org 1's library.
fn intruder() -> Principal {
    Principal {
        org_id: uuid(2),
        user_id: uuid(13),
        team_ids: vec![],
    }
}

#[tokio::test]
async fn library_rls_isolates_orgs_on_every_new_table() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    // Org 1 fills its library: a standard (with memory provenance), a skill
    // with a published version, and a usage event.
    let p = pay_dev();
    let std_id = uuid(70);
    let skill_id = uuid(71);
    let ver_id = uuid(72);
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        library::insert_standard(
            c,
            &library::NewStandard {
                id: std_id,
                org_id: org(),
                origin: Default::default(),
                stack: "rust".into(),
                category: "errors".into(),
                slug: "no-unwrap-in-handlers".into(),
                statement: "Request handlers never unwrap; they map errors to typed responses."
                    .into(),
                rationale: Some("Learned from the org-wide payment standard incident.".into()),
                detail_md: None,
                enforcement: Enforcement::Mandatory,
                provenance: vec![(StandardProvenanceKind::Memory, uuid(101))],
                author: Some(p.user_id),
            },
        )
        .await
        .expect("standard");
        library::insert_skill(
            c,
            &library::NewSkill {
                id: skill_id,
                org_id: org(),
                slug: "review-migrations".into(),
                name: "Review migrations".into(),
                description: None,
                domain: Some("database".into()),
            },
        )
        .await
        .expect("skill");
        library::add_skill_version(
            c,
            &library::NewSkillVersion {
                id: ver_id,
                skill_id,
                org_id: org(),
                semver: "1.0.0".into(),
                manifest: serde_json::json!({"name": "review-migrations"}),
                content_md: "# Review migrations\ncheck RLS on every new table".into(),
                resources: serde_json::json!([]),
            },
        )
        .await
        .expect("version");
        assert!(library::publish_skill_version(c, ver_id, p.user_id)
            .await
            .expect("publish"));
        library::record_usage(
            c,
            org(),
            LibraryArtifactKind::Skill,
            skill_id,
            Some("1.0.0"),
            LibraryUsageEvent::Fetch,
            Some(uuid(21)),
        )
        .await
        .expect("usage");
        tx.commit().await.expect("commit");
    }

    // The owner org sees everything it wrote.
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        assert!(library::get_standard(c, std_id)
            .await
            .expect("get")
            .is_some());
        assert_eq!(library::list_skills(c).await.expect("skills").len(), 1);
        assert_eq!(
            library::usage_by_team(c, LibraryArtifactKind::Skill, skill_id)
                .await
                .expect("usage")
                .len(),
            1
        );
    }

    // Another org's principal sees NOTHING — and "nothing" must be indistinguishable
    // from "does not exist" on every table, reads and aggregates alike.
    {
        let mut tx = ctx.store.scoped_tx(&intruder()).await.expect("tx");
        let c = &mut *tx;
        assert!(library::get_standard(c, std_id)
            .await
            .expect("get")
            .is_none());
        assert!(library::get_standard_by_slug(c, "no-unwrap-in-handlers")
            .await
            .expect("slug")
            .is_none());
        assert!(library::list_standards(c, None, None)
            .await
            .expect("list")
            .is_empty());
        assert!(library::provenance(c, std_id)
            .await
            .expect("prov")
            .is_empty());
        assert!(library::get_skill_by_slug(c, "review-migrations")
            .await
            .expect("skill")
            .is_none());
        assert!(library::list_skills(c).await.expect("skills").is_empty());
        assert!(library::current_published_version(c, skill_id)
            .await
            .expect("ver")
            .is_none());
        assert!(
            library::usage_by_team(c, LibraryArtifactKind::Skill, skill_id)
                .await
                .expect("usage")
                .is_empty()
        );
        // Writing INTO the other org is refused by WITH CHECK, not by convention.
        let smuggle = library::insert_skill(
            c,
            &library::NewSkill {
                id: uuid(73),
                org_id: org(), // org 1, but the session is scoped to org 2
                slug: "smuggled".into(),
                name: "smuggled".into(),
                description: None,
                domain: None,
            },
        )
        .await;
        assert!(smuggle.is_err(), "cross-org insert must be a policy error");
    }

    // The leaderboard invariant is a SHAPE, not a query discipline: the events
    // table must not even have a user column to misuse.
    let has_user_col: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_name = 'library_usage_events' AND column_name LIKE '%user%'",
    )
    .fetch_one(&ctx.admin)
    .await
    .expect("schema check");
    assert_eq!(
        has_user_col, 0,
        "usage events must be unattributable to a person"
    );
}

#[tokio::test]
async fn divergence_ratification_bridges_to_exactly_one_candidate() {
    let Some((ctx, _guard)) = setup().await else {
        return;
    };
    seed(&ctx).await;

    let p = pay_dev();
    let d1 = uuid(61);

    // A detected divergence, as the sweep writes it (migration 0016).
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        sqlx::query(
            "INSERT INTO practice_divergences
                (id, org_id, practice, summary, recommended_standard, impact, positions, model_ref)
             VALUES ($1, $2, 'Service retry policy',
                     'payments retries 3x fixed, data retries with full jitter',
                     'Exponential backoff with full jitter, max 30s',
                     'high', $3, 'test-model')",
        )
        .bind(d1)
        .bind(org())
        .bind(serde_json::json!([
            {"team": "payments", "approach": "3x fixed"},
            {"team": "data", "approach": "full jitter"}
        ]))
        .execute(&mut *tx)
        .await
        .expect("divergence");
        tx.commit().await.expect("commit");
    }

    // Ratify: one divergence in, one PROPOSED candidate out, carrying the
    // divergence as provenance and the recommendation as its statement.
    let std_id = {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        let id = library::ratify_divergence(c, d1, p.user_id)
            .await
            .expect("ratify")
            .expect("divergence exists");
        tx.commit().await.expect("commit");
        id
    };
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        let s = library::get_standard(c, std_id)
            .await
            .expect("get")
            .expect("bridged standard");
        assert_eq!(s.lifecycle, StandardLifecycle::Proposed);
        assert_eq!(s.slug, "service-retry-policy");
        assert_eq!(s.statement, "Exponential backoff with full jitter, max 30s");
        let prov = library::provenance(c, std_id).await.expect("prov");
        assert_eq!(prov.len(), 1, "exactly one provenance row");
        assert_eq!(prov[0].kind, StandardProvenanceKind::Divergence);
        assert_eq!(prov[0].ref_id, d1);

        // Ratifying the SAME divergence again returns the same candidate —
        // never a second one. This is the LB0 gate.
        let again = library::ratify_divergence(c, d1, p.user_id)
            .await
            .expect("ratify again")
            .expect("still resolves");
        assert_eq!(again, std_id);
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM standard_provenance WHERE kind = 'divergence' AND ref_id = $1",
        )
        .bind(d1)
        .fetch_one(&mut *tx)
        .await
        .expect("count");
        assert_eq!(total, 1, "exactly one candidate per divergence");
    }

    // Another org cannot ratify org 1's divergence — same answer as "no such
    // divergence", because existence is itself information.
    {
        let mut tx = ctx.store.scoped_tx(&intruder()).await.expect("tx");
        let c = &mut *tx;
        assert!(library::ratify_divergence(c, d1, intruder().user_id)
            .await
            .expect("ratify")
            .is_none());
    }

    // Adoption: the bridged candidate carries evidence, so a named human can
    // adopt it plainly.
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        assert!(library::adopt_standard(c, std_id, p.user_id, false)
            .await
            .expect("adopt"));
        tx.commit().await.expect("commit");
    }

    // An evidence-free rule CANNOT be adopted without a decree — the database
    // refuses, not the API. The transaction is poisoned by the refusal, so it
    // is dropped and the decree path gets a fresh one.
    let bare = uuid(62);
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        library::insert_standard(
            c,
            &library::NewStandard {
                id: bare,
                org_id: org(),
                origin: Default::default(),
                stack: "general".into(),
                category: "style".into(),
                slug: "tabs-vs-spaces".into(),
                statement: "Spaces.".into(),
                rationale: None,
                detail_md: None,
                enforcement: Enforcement::Recommended,
                provenance: vec![],
                author: Some(p.user_id),
            },
        )
        .await
        .expect("bare standard");
        tx.commit().await.expect("commit");
    }
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        let refused = library::adopt_standard(c, bare, p.user_id, false).await;
        assert!(
            refused.is_err(),
            "no provenance + no decree must be refused by the schema"
        );
    }
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        assert!(library::adopt_standard(c, bare, p.user_id, true)
            .await
            .expect("decreed adoption"));
        tx.commit().await.expect("commit");
    }
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let c = &mut *tx;
        let s = library::get_standard(c, bare)
            .await
            .expect("get")
            .expect("decreed standard");
        assert_eq!(s.lifecycle, StandardLifecycle::Adopted);
        assert_eq!(
            s.decreed_by,
            Some(p.user_id),
            "an evidence-free rule must name the human who signed for it"
        );

        // Retirement is one-way and idempotence is a caller bug surfaced as `false`.
        assert!(library::deprecate_standard(c, std_id, p.user_id)
            .await
            .expect("deprecate"));
        assert!(!library::deprecate_standard(c, std_id, p.user_id)
            .await
            .expect("second deprecate"));
        // The adopted-only serve filter no longer returns it.
        let served = library::list_standards(c, None, Some(StandardLifecycle::Adopted))
            .await
            .expect("list adopted");
        assert!(served.iter().all(|s| s.id != std_id));
    }
}
