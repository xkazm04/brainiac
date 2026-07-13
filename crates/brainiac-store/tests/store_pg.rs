//! Store integration tests — require a live Postgres (docker compose up).
//! Skipped (with a loud note) when DATABASE_URL is unset, so the pure crates
//! stay testable without Docker.
//!
//! What must hold here and nowhere less:
//! - RLS visibility matrix through the app role (org / team / private).
//! - The pgvector scan and FTS scan inherit RLS — no leak at the SQL layer.
//! - Queue: claim invisibility, crash-redelivery, dead-lettering.

use brainiac_core::{MemoryKind, MemoryStatus, Principal, Visibility};
use brainiac_store::{entities, feedback, memories, orgs, queue, Store};
use uuid::Uuid;

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

fn uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

// Tests share one database: serialize them so truncate/seed phases never
// interleave (cargo runs test fns in parallel by default).
static DB_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
}

async fn setup() -> Option<(Ctx, tokio::sync::MutexGuard<'static, ()>)> {
    let guard = DB_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let Some(url) = database_url() else {
        eprintln!("SKIP: DATABASE_URL not set — store integration tests need Postgres");
        return None;
    };
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin pool");
    // Idempotent replay: wipe tenant data (order-insensitive via CASCADE).
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
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

    // Exhaust attempts → dead-letter.
    let mut job = redelivered[0].clone();
    job.attempts = queue::MAX_ATTEMPTS;
    let retrying = queue::fail(pool, &job, 0).await.expect("dead");
    assert!(!retrying, "exhausted job must not retry");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 0);

    // Success path archives cleanly too.
    queue::send(pool, "extract", &payload).await.expect("send2");
    let claimed = queue::read(pool, "extract", 1, 30).await.expect("read4");
    queue::complete(pool, &claimed[0]).await.expect("complete");
    assert_eq!(queue::depth(pool, "extract").await.expect("depth"), 0);

    // Health reflects the story so far: nothing live, one ok, one dead.
    let h = queue::health(pool, "extract").await.expect("health");
    assert_eq!((h.ready, h.in_flight), (0, 0));
    assert_eq!(h.archived_ok, 1);
    assert_eq!(h.dead_letters, 1);

    // Dead-letter inspection + requeue round trip.
    let dead = queue::dead_letters(pool, "extract", 10).await.expect("dl");
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
    assert!(soon.iter().any(|m| m.id == uuid(102)), "5-days-out memory queued");
    let sooner = memories::expiring(&mut tx, 1, 50).await.expect("expiring");
    assert!(sooner.iter().all(|m| m.id != uuid(102)), "outside 1-day horizon");

    // Re-verify: window extends from NOW, not the old boundary.
    let new_to = memories::extend_validity(&mut tx, uuid(102), 365)
        .await
        .expect("extend")
        .expect("row updated");
    assert!(new_to > chrono::Utc::now() + chrono::Duration::days(300));
    let after = memories::expiring(&mut tx, 30, 50).await.expect("expiring");
    assert!(after.iter().all(|m| m.id != uuid(102)), "re-verified row left the queue");

    // RLS: another org's/team's caller can't extend what they can't see.
    drop(tx);
    let mut tx = ctx.store.scoped_tx(&data_analyst()).await.expect("tx");
    let denied = memories::extend_validity(&mut tx, uuid(103), 365)
        .await
        .expect("query ok");
    assert!(denied.is_none(), "invisible (private) memory must not extend");

    let _ = &ctx.admin;
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
        feedback::flagged(&mut tx, 50).await.expect("flagged").is_empty(),
        "a helpful verdict must not open a claim"
    );

    // Two negative verdicts against one memory = ONE queue row, counts merged.
    for (verdict, note) in [("wrong", Some("psp changed the endpoint")), ("outdated", None)] {
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
    let flagged = feedback::flagged(&mut tx, 50).await.expect("flagged");
    assert_eq!(flagged.len(), 1, "one row per disputed memory, not per verdict");
    assert_eq!(flagged[0].memory_id, uuid(102));
    assert_eq!((flagged[0].wrong, flagged[0].outdated), (1, 1));
    assert_eq!(flagged[0].notes, vec!["psp changed the endpoint".to_string()]);
    assert_eq!(feedback::flagged_count(&mut tx).await.expect("count"), 1);

    // Trust attaches to served memories in one batched lookup.
    let trust = feedback::trust_for(&mut tx, &[uuid(101), uuid(102), uuid(104)])
        .await
        .expect("trust");
    assert_eq!(trust[&uuid(101)].helpful, 1);
    assert!(!trust[&uuid(101)].disputed(), "helpful is not a dispute");
    assert!(trust[&uuid(102)].disputed(), "open claims mark a memory disputed");
    assert!(!trust.contains_key(&uuid(104)), "un-rated memories carry no trust row");

    // A maintainer answers: dismissed → claims close, memory untouched.
    let closed = feedback::resolve_claims(&mut tx, uuid(102), uuid(11), "dismissed")
        .await
        .expect("resolve");
    assert_eq!(closed, 2, "both open claims close together");
    assert!(
        feedback::flagged(&mut tx, 50).await.expect("flagged").is_empty(),
        "answered claims leave the queue"
    );
    let trust = feedback::trust_for(&mut tx, &[uuid(102)]).await.expect("trust");
    assert_eq!(trust[&uuid(102)].wrong, 1, "history is kept");
    assert!(!trust[&uuid(102)].disputed(), "but the dispute is settled");

    // Re-resolving an already-answered memory closes nothing (idempotent).
    let closed = feedback::resolve_claims(&mut tx, uuid(102), uuid(11), "dismissed")
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
