//! Pipeline integration test (DATABASE_URL-gated): ingest the three seed
//! transcripts through the FULL chain — extract → embed → resolve →
//! contradict → promote — with a deterministic mock provider that emits the
//! fixtures' gold extraction. This proves the PLUMBING end-to-end (parsing,
//! validation firewall, provenance, entity get-or-create, resolve outcomes,
//! governance audit rows); extraction QUALITY is a per-provider nightly
//! concern (EVAL.md §3).

use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_fixtures::Fixtures;
use brainiac_gateway::{ChatRequest, MockProvider};
use brainiac_pipeline::{resolve, worker};
use brainiac_store::Store;
use serde_json::json;
use uuid::Uuid;

/// These tests share one database (truncate + seed), so serialize them —
/// cargo runs test fns in parallel by default, which would let one test's
/// TRUNCATE tear down another's seed mid-run.
static DB_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

async fn db_guard() -> tokio::sync::MutexGuard<'static, ()> {
    DB_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

fn gold_extraction_json(fx: &Fixtures, transcript_id: &str) -> String {
    let t = fx
        .transcripts
        .iter()
        .find(|t| t.id == transcript_id)
        .expect("transcript");
    let entity_name = |eid: &str| -> serde_json::Value {
        let e = fx
            .entities
            .entities
            .iter()
            .find(|e| e.id == eid)
            .expect("entity");
        json!({"name": e.name, "kind": e.kind})
    };
    let name_of = |eid: &str| -> String {
        fx.entities
            .entities
            .iter()
            .find(|e| e.id == eid)
            .expect("entity")
            .name
            .clone()
    };
    let memories: Vec<serde_json::Value> = t
        .gold_memories
        .iter()
        .map(|g| {
            json!({
                "kind": g.kind,
                "content": g.content_gist,
                "visibility": if g.visibility == "org" { "org" } else { "team" },
                "confidence": 0.95,
                "entities": g.entities.iter().map(|e| entity_name(e)).collect::<Vec<_>>(),
                "relations": g.relations.iter().map(|r| json!({
                    "src": name_of(&r.src), "rel": r.rel, "dst": name_of(&r.dst)
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "memories": memories }).to_string()
}

/// Mock provider wired to ground truth: gold extraction per transcript,
/// negative-pair-aware adjudication, supersede-aware contradiction verdicts.
fn perfect_mock(fx: &Fixtures) -> MockProvider {
    let extraction: Vec<(String, String)> = fx
        .transcripts
        .iter()
        .map(|t| {
            let marker = t.turns.first().expect("turns").text.clone();
            (marker, gold_extraction_json(fx, &t.id))
        })
        .collect();
    MockProvider::new(move |req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            for (marker, json) in &extraction {
                if req.user.contains(marker.as_str()) {
                    return json.clone();
                }
            }
            return r#"{"memories":[]}"#.to_string();
        }
        if req.system.contains("adjudicate") {
            // Ground truth: name pairs from negative sets are NOT the same.
            let negative = [
                "fraud",
                "checkout v1",
                "payments team",
                "Streams",
                "OPA retry",
            ];
            let same = !negative.iter().any(|n| req.user.contains(n));
            return format!(r#"{{"same": {same}, "confidence": 0.9}}"#);
        }
        if req.system.contains("Decide their relationship") {
            // Conservative default: dismiss (contradiction quality is tested
            // against gold cases in the nightly pipeline profile).
            return r#"{"relation":"dismiss","winner":null,"reason":"mock"}"#.to_string();
        }
        r#"{}"#.to_string()
    })
}

#[tokio::test]
async fn full_pipeline_over_seed_transcripts() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
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
    let provider = brainiac_gateway::ProviderRouter::single(std::sync::Arc::new(perfect_mock(&fx)));

    // Identity + sources.
    let org_id = brainiac_fixtures::ids::stable_uuid(&fx.org.org);
    let principal = brainiac_pipeline::pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_id, &fx.org.org)
        .await
        .expect("org");
    for t in &fx.org.teams {
        brainiac_store::orgs::upsert_team(
            &mut tx,
            brainiac_fixtures::ids::stable_uuid(&t.id),
            org_id,
            &t.name,
        )
        .await
        .expect("team");
    }
    let mut source_ids: Vec<Uuid> = Vec::new();
    for t in &fx.transcripts {
        let sid = brainiac_fixtures::ids::stable_uuid(&t.id);
        let text: String = t
            .turns
            .iter()
            .map(|turn| format!("{}: {}", turn.role, turn.text))
            .collect::<Vec<_>>()
            .join("\n");
        brainiac_store::governance::insert_source(
            &mut tx,
            sid,
            org_id,
            Some(brainiac_fixtures::ids::stable_uuid(&t.team)),
            "session_transcript",
            &text,
            None,
        )
        .await
        .expect("source");
        source_ids.push(sid);
    }
    tx.commit().await.expect("commit");

    // Enqueue + drain.
    let version = {
        let mut tx = store.scoped_tx(&principal).await.expect("tx");
        let v = brainiac_store::memories::ensure_embedding_version(
            &mut tx,
            embedder.model_name(),
            embedder.dim() as i32,
        )
        .await
        .expect("ver");
        tx.commit().await.expect("commit");
        v
    };
    for sid in &source_ids {
        worker::enqueue_source(&store, org_id, *sid)
            .await
            .expect("enqueue");
    }
    let stats = worker::tick(&store, &provider, &embedder, version, 32)
        .await
        .expect("tick");

    // ── plumbing assertions ──────────────────────────────────────────────
    let gold_total: usize = fx.transcripts.iter().map(|t| t.gold_memories.len()).sum();
    assert_eq!(
        stats.jobs,
        fx.transcripts.len(),
        "every seed source processed"
    );
    assert_eq!(
        stats.memories, gold_total,
        "one memory per gold item, none dropped"
    );

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    use sqlx::Row;

    // Freshness lifecycle: extraction stamps a validity window on every
    // memory (valid_from = now, valid_to = now + kind TTL).
    let unstamped: i64 = sqlx::query(
        "SELECT count(*) AS n FROM memories
         WHERE valid_from IS NULL OR valid_to IS NULL OR valid_to <= now()",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("q")
    .get("n");
    assert_eq!(
        unstamped, 0,
        "every extracted memory carries a future validity window"
    );

    // Every memory carries provenance pointing at its source + the mock model.
    let orphan: i64 = sqlx::query(
        "SELECT count(*) AS n FROM memories m
         LEFT JOIN provenance p ON p.id = m.provenance_id
         WHERE p.id IS NULL OR p.model_ref IS NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("q")
    .get("n");
    assert_eq!(
        orphan, 0,
        "every extracted memory must be provenance-stamped"
    );

    // Promotion audit rows exist for every memory; high-confidence pitfalls
    // and explicit decisions auto-promoted, the rest await review.
    let promotions: i64 = sqlx::query("SELECT count(*) AS n FROM promotions")
        .fetch_one(&mut *tx)
        .await
        .expect("q")
        .get("n");
    assert_eq!(promotions as usize, gold_total, "one audit row per memory");
    assert!(
        stats.auto_promoted >= 1,
        "gold corpus contains auto-promotable items"
    );
    assert_eq!(stats.auto_promoted + stats.needs_review, gold_total);

    // Resolve: near-miss traps must NOT auto-merge. For each negative pair,
    // check via SQL whether the two surface forms ended up sharing a
    // canonical entity.
    let mut false_merges = 0;
    for pair in &fx.merges.negative_pairs {
        let a = fx
            .entities
            .entities
            .iter()
            .find(|e| e.id == pair[0])
            .expect("fx a");
        let b = fx
            .entities
            .entities
            .iter()
            .find(|e| e.id == pair[1])
            .expect("fx b");
        let shared: i64 = sqlx::query(
            "SELECT count(*) AS n
             FROM entities ea
             JOIN entity_links la ON la.entity_id = ea.id
             JOIN entity_links lb ON lb.canonical_id = la.canonical_id
             JOIN entities eb ON eb.id = lb.entity_id
             WHERE lower(ea.name) = lower($1) AND lower(eb.name) = lower($2)",
        )
        .bind(&a.name)
        .bind(&b.name)
        .fetch_one(&mut *tx)
        .await
        .expect("q")
        .get("n");
        if shared > 0 {
            false_merges += 1;
        }
    }
    assert_eq!(
        false_merges, 0,
        "HARD GATE: near-miss traps must never auto-merge"
    );

    // Direction 2: every canonical born through resolve carries a persisted
    // name embedding for the active version — so resolution reads one SQL
    // similarity query instead of re-embedding all canonicals live.
    let (canon_total, embedded): (i64, i64) = {
        let c: i64 = sqlx::query("SELECT count(*) AS n FROM canonical_entities")
            .fetch_one(&mut *tx)
            .await
            .expect("q")
            .get("n");
        let e: i64 = sqlx::query(
            "SELECT count(*) AS n FROM canonical_entity_embeddings WHERE embedding_version_id = $1",
        )
        .bind(version)
        .fetch_one(&mut *tx)
        .await
        .expect("q")
        .get("n");
        (c, e)
    };
    assert!(canon_total > 0, "corpus bootstrapped canonicals");
    assert_eq!(
        canon_total, embedded,
        "every canonical has a persisted embedding (no live re-embedding path)"
    );

    // Queue drained.
    assert_eq!(
        brainiac_store::queue::depth(store.pool(), worker::INGEST_QUEUE)
            .await
            .expect("depth"),
        0
    );
}

/// Direction 1: a governance supersession must feed the temporal engine —
/// after `apply_supersession`, retrieval serves the winner and never the
/// deprecated loser, and the transition lands in the promotions audit log.
#[tokio::test]
async fn supersession_serves_only_the_winner() {
    use brainiac_core::{MemoryKind, MemoryStatus, Principal, Visibility};
    use brainiac_store::{memories, orgs, retrieval};
    use chrono::Utc;
    use sqlx::Row;

    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let org_id = Uuid::from_bytes([7u8; 16]);
    let team = Uuid::from_bytes([8u8; 16]);
    let user = Uuid::from_bytes([9u8; 16]);
    let principal = Principal {
        org_id,
        user_id: user,
        team_ids: vec![team],
    };
    let winner = Uuid::from_bytes([1u8; 16]);
    let loser = Uuid::from_bytes([2u8; 16]);
    let win_txt = "psp-gateway retry cap is five attempts";
    let lose_txt = "psp-gateway retry cap is three attempts";

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team, org_id, "payments")
        .await
        .expect("team");
    orgs::upsert_user(&mut tx, user, org_id, "u@x")
        .await
        .expect("user");
    orgs::upsert_member(&mut tx, team, user, "maintainer")
        .await
        .expect("member");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    let mk = |id: Uuid, content: &str| memories::NewMemory {
        id,
        org_id,
        team_id: Some(team),
        owner_user_id: None,
        visibility: Visibility::Org,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        content: content.to_string(),
        language: "en".into(),
        valid_from: Some(Utc::now()),
        valid_to: None,
        superseded_by: None,
        confidence: Some(0.9),
        provenance_id: None,
    };
    for (id, txt) in [(winner, win_txt), (loser, lose_txt)] {
        memories::insert(&mut tx, &mk(id, txt))
            .await
            .expect("insert");
        memories::upsert_embedding(
            &mut tx,
            id,
            version,
            &embedder.embed(txt).await.expect("embed"),
        )
        .await
        .expect("embed row");
    }
    tx.commit().await.expect("commit");

    let req = retrieval::RetrievalRequest {
        query: "psp-gateway retry cap".into(),
        k: 10,
        as_of: None,
        filters: Default::default(),
    };

    // Before: the conflict is live — both are served.
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    let before: Vec<Uuid> = retrieval::search(&mut tx, store.pool(), &embedder, version, &req)
        .await
        .expect("search")
        .iter()
        .map(|h| h.memory.id)
        .collect();
    assert!(
        before.contains(&winner) && before.contains(&loser),
        "both memories live before supersede: {before:?}"
    );

    // Apply the supersession as the maintainer.
    let applied = brainiac_store::governance::apply_supersession(
        &mut tx,
        org_id,
        loser,
        winner,
        Some(user),
        "contradiction_supersede",
    )
    .await
    .expect("apply");
    assert!(applied, "supersession applied");
    let again = brainiac_store::governance::apply_supersession(
        &mut tx,
        org_id,
        loser,
        winner,
        Some(user),
        "contradiction_supersede",
    )
    .await
    .expect("apply again");
    assert!(!again, "already-superseded memory is a no-op (idempotent)");
    tx.commit().await.expect("commit");

    // After: retrieval serves ONLY the winner (the temporal chain is real now).
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    let after: Vec<Uuid> = retrieval::search(&mut tx, store.pool(), &embedder, version, &req)
        .await
        .expect("search")
        .iter()
        .map(|h| h.memory.id)
        .collect();
    assert!(after.contains(&winner), "winner still served: {after:?}");
    assert!(
        !after.contains(&loser),
        "deprecated loser must not be served: {after:?}"
    );

    // Loser row carries the applied supersession.
    let row = sqlx::query(
        "SELECT status::text AS s, superseded_by, valid_to FROM memories WHERE id = $1",
    )
    .bind(loser)
    .fetch_one(&mut *tx)
    .await
    .expect("row");
    assert_eq!(row.get::<String, _>("s"), "deprecated");
    assert_eq!(row.get::<Option<Uuid>, _>("superseded_by"), Some(winner));
    assert!(row
        .get::<Option<chrono::DateTime<Utc>>, _>("valid_to")
        .is_some());

    // Audit: the deprecation is in the promotions log, naming who applied it.
    let audit = sqlx::query(
        "SELECT reviewer_id, to_status::text AS t, policy_decision
         FROM promotions WHERE memory_id = $1 AND policy_rule = 'contradiction_supersede'",
    )
    .bind(loser)
    .fetch_one(&mut *tx)
    .await
    .expect("audit row");
    assert_eq!(audit.get::<Option<Uuid>, _>("reviewer_id"), Some(user));
    assert_eq!(audit.get::<String, _>("t"), "deprecated");
    assert_eq!(audit.get::<String, _>("policy_decision"), "approved");

    let _ = &admin;
}

/// Direction 3: extraction captures surface-form aliases, and resolution
/// matches them lexically across teams — proven WITHOUT any hand-seeded
/// alias (the whole point). One team's transcript names "psp-gateway (PSP)";
/// another team's bare "PSP" then resolves to the same canonical.
#[tokio::test]
async fn alias_capture_and_resolution() {
    use brainiac_gateway::{ChatRequest, MockProvider};
    use brainiac_pipeline::{extract, resolve};
    use brainiac_store::{entities, memories, orgs};
    use sqlx::Row;

    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let org_id = Uuid::from_bytes([5u8; 16]);
    let team_a = Uuid::from_bytes([6u8; 16]);
    let team_b = Uuid::from_bytes([7u8; 16]);
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // Mock extractor: one memory whose entity declares aliases.
    let mock = MockProvider::new(|req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            return r#"{"memories":[{"kind":"fact",
                "content":"psp-gateway owns retry backoff for refunds",
                "visibility":"team","confidence":0.9,
                "entities":[{"name":"psp-gateway","kind":"service",
                    "aliases":["PSP","payment service provider"]}],
                "relations":[]}]}"#
                .to_string();
        }
        "{}".to_string()
    });

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team_a, org_id, "payments")
        .await
        .expect("team_a");
    orgs::upsert_team(&mut tx, team_b, org_id, "data")
        .await
        .expect("team_b");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    let source_id = Uuid::from_bytes([10u8; 16]);
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        org_id,
        Some(team_a),
        "session_transcript",
        "psp-gateway (PSP) owns retry backoff",
        None,
    )
    .await
    .expect("source");

    // Extraction persists the captured aliases on the raw entity.
    let stats = extract::run_extract(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        Some(team_a),
        source_id,
        "psp-gateway (PSP) owns retry backoff",
    )
    .await
    .expect("extract");
    let ent_a = *stats.entities_created.first().expect("entity created");
    let row = sqlx::query("SELECT name, kind, aliases FROM entities WHERE id = $1")
        .bind(ent_a)
        .fetch_one(&mut *tx)
        .await
        .expect("entity row");
    let name_a: String = row.get("name");
    let kind_a: String = row.get("kind");
    let aliases_a: Vec<String> = row.get("aliases");
    assert!(
        aliases_a.iter().any(|a| a == "PSP"),
        "extraction persisted surface-form aliases: {aliases_a:?}"
    );

    // Resolve it → bootstraps a canonical that accumulates the aliases.
    let outcome = resolve::resolve_entity(
        &mut tx, &mock, &embedder, version, org_id, ent_a, &name_a, &kind_a, &aliases_a,
    )
    .await
    .expect("resolve a");
    let canonical_a = match outcome {
        resolve::ResolveOutcome::NewCanonical { canonical_id } => canonical_id,
        other => panic!("expected NewCanonical, got {other:?}"),
    };
    let canon_aliases: Vec<String> =
        sqlx::query("SELECT aliases FROM canonical_entities WHERE id = $1")
            .bind(canonical_a)
            .fetch_one(&mut *tx)
            .await
            .expect("canon")
            .get("aliases");
    assert!(
        canon_aliases.iter().any(|a| a == "PSP"),
        "canonical accumulated the alias: {canon_aliases:?}"
    );

    // A DIFFERENT team later mentions bare "PSP" — no shared embedding needed,
    // no fixture seeding: it resolves to the same canonical via the alias.
    let ent_b = Uuid::from_bytes([20u8; 16]);
    entities::insert_entity(
        &mut tx,
        ent_b,
        org_id,
        Some(team_b),
        "PSP",
        "service",
        &[],
        None,
    )
    .await
    .expect("insert b");
    let outcome_b = resolve::resolve_entity(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        ent_b,
        "PSP",
        "service",
        &[],
    )
    .await
    .expect("resolve b");
    match outcome_b {
        resolve::ResolveOutcome::Linked {
            canonical_id,
            method,
        } => {
            assert_eq!(canonical_id, canonical_a, "linked to the same canonical");
            assert_eq!(method, "alias_lexical", "resolved by alias, not embedding");
        }
        other => panic!("expected alias-linked, got {other:?}"),
    }

    // Both raw forms share one canonical — the cross-team bridge is built.
    let shared: i64 = sqlx::query(
        "SELECT count(DISTINCT canonical_id) AS n FROM entity_links WHERE entity_id = ANY($1)",
    )
    .bind(vec![ent_a, ent_b])
    .fetch_one(&mut *tx)
    .await
    .expect("q")
    .get("n");
    assert_eq!(shared, 1, "both forms linked into exactly one canonical");

    tx.commit().await.expect("commit");
    let _ = &admin;
}

#[tokio::test]
#[allow(clippy::assertions_on_constants)] // deliberate: tuning-coherence guard
async fn resolve_thresholds_are_ordered() {
    // Pure sanity: the band boundaries stay coherent if someone tunes them.
    assert!(resolve::ADJUDICATION_FLOOR < resolve::AUTO_LINK_SIMILARITY);
    assert!(resolve::ADJUDICATION_AUTO_CONFIDENCE > 0.5);
}

/// Shared teardown for the resilience/chunking tests below.
async fn truncate_all(admin: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(admin)
    .await
    .expect("truncate");
}

/// Direction 1: re-processing the same source must not duplicate memories.
/// The (org, source, content) dedup skips every memory the source already
/// produced — proven by extracting the same source twice.
#[tokio::test]
async fn reprocessing_source_is_idempotent() {
    use brainiac_gateway::{ChatRequest, MockProvider};
    use brainiac_pipeline::extract;
    use brainiac_store::{memories, orgs};
    use sqlx::Row;

    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate_all(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let org_id = Uuid::from_bytes([31u8; 16]);
    let team = Uuid::from_bytes([32u8; 16]);
    let principal = brainiac_pipeline::pipeline_principal(org_id);
    let source_text = "psp-gateway retries five times; refunds use idempotency keys";

    let mock = MockProvider::new(|req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            return r#"{"memories":[
                {"kind":"fact","content":"psp-gateway retries five times",
                 "visibility":"org","confidence":0.9,"entities":[],"relations":[]},
                {"kind":"decision","content":"refunds use idempotency keys",
                 "visibility":"org","confidence":0.9,"entities":[],"relations":[]}
            ]}"#
            .to_string();
        }
        "{}".to_string()
    });

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team, org_id, "payments")
        .await
        .expect("team");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    let source_id = Uuid::from_bytes([33u8; 16]);
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        org_id,
        Some(team),
        "session_transcript",
        source_text,
        None,
    )
    .await
    .expect("source");
    tx.commit().await.expect("commit");

    // Dedup reads the just-written rows via the worker read scope (org+team).
    let mut tx = store.worker_tx(&principal).await.expect("tx");
    let first = extract::run_extract(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        Some(team),
        source_id,
        source_text,
    )
    .await
    .expect("extract 1");
    tx.commit().await.expect("commit");
    assert_eq!(first.memories_written, 2, "first pass writes both memories");
    assert_eq!(first.deduped, 0);

    let mut tx = store.worker_tx(&principal).await.expect("tx");
    let second = extract::run_extract(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        Some(team),
        source_id,
        source_text,
    )
    .await
    .expect("extract 2");
    tx.commit().await.expect("commit");
    assert_eq!(second.memories_written, 0, "reprocess writes nothing new");
    assert_eq!(second.deduped, 2, "both memories recognized as duplicates");

    let n: i64 = sqlx::query("SELECT count(*) AS n FROM memories WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&admin)
        .await
        .expect("q")
        .get("n");
    assert_eq!(n, 2, "reprocessing did not duplicate memories");
}

/// Direction 1: a malformed first response is repaired with exactly one
/// re-prompt, and the corrected JSON is written — the parse-failure/repair
/// counters reflect it.
#[tokio::test]
async fn malformed_extraction_repairs_once() {
    use brainiac_gateway::{ChatRequest, MockProvider};
    use brainiac_pipeline::extract;
    use brainiac_store::{memories, orgs};

    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate_all(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let org_id = Uuid::from_bytes([41u8; 16]);
    let team = Uuid::from_bytes([42u8; 16]);
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // First extract call returns garbage; the repair re-prompt (which echoes
    // "could not be parsed") gets valid JSON.
    let mock = MockProvider::new(|req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            if req.user.contains("could not be parsed") {
                return r#"{"memories":[{"kind":"fact","content":"repaired memory",
                    "visibility":"org","confidence":0.9,"entities":[],"relations":[]}]}"#
                    .to_string();
            }
            return "SORRY not valid json {{{".to_string();
        }
        "{}".to_string()
    });

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team, org_id, "payments")
        .await
        .expect("team");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    let source_id = Uuid::from_bytes([43u8; 16]);
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        org_id,
        Some(team),
        "session_transcript",
        "a transcript",
        None,
    )
    .await
    .expect("source");
    tx.commit().await.expect("commit");

    let mut tx = store.worker_tx(&principal).await.expect("tx");
    let stats = extract::run_extract(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        Some(team),
        source_id,
        "a transcript",
    )
    .await
    .expect("extract recovers via repair");
    tx.commit().await.expect("commit");
    assert_eq!(stats.parse_failures, 1, "first parse failed");
    assert_eq!(stats.repairs, 1, "one repair recovered it");
    assert_eq!(stats.memories_written, 1, "repaired memory written");
}

/// Direction 1: a persistently-malformed source fails the job even after the
/// one repair, and the queue's attempt-aware fail() dead-letters it after
/// MAX_ATTEMPTS instead of retrying forever. The worker calls this exact
/// fail() (worker::tick, backoff 30s); here we drive it with zero backoff so
/// the redelivery loop doesn't wait on the visibility timeout.
#[tokio::test]
async fn persistently_malformed_source_fails_then_dead_letters() {
    use brainiac_gateway::{ChatRequest, MockProvider};
    use brainiac_pipeline::extract;
    use brainiac_store::{memories, orgs, queue};

    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = db_guard().await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    truncate_all(&admin).await;

    let store = Store::connect(&url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let org_id = Uuid::from_bytes([51u8; 16]);
    let team = Uuid::from_bytes([52u8; 16]);
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // Never returns parseable JSON — even the repair fails.
    let mock = MockProvider::new(|req: &ChatRequest| {
        if req.system.contains("distill organizational knowledge") {
            return "NEVER valid {{{".to_string();
        }
        "{}".to_string()
    });

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    orgs::upsert_org(&mut tx, org_id, "org").await.expect("org");
    orgs::upsert_team(&mut tx, team, org_id, "payments")
        .await
        .expect("team");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("ver");
    let source_id = Uuid::from_bytes([53u8; 16]);
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        org_id,
        Some(team),
        "session_transcript",
        "garbage",
        None,
    )
    .await
    .expect("source");
    tx.commit().await.expect("commit");

    // The extract stage itself hard-fails after the bounded repair.
    let mut tx = store.worker_tx(&principal).await.expect("tx");
    let result = extract::run_extract(
        &mut tx,
        &mock,
        &embedder,
        version,
        org_id,
        Some(team),
        source_id,
        "garbage",
    )
    .await;
    assert!(result.is_err(), "malformed source hard-fails the extract");
    drop(tx);

    // The queue's fail() (what the worker calls) dead-letters after MAX_ATTEMPTS.
    queue::send(
        store.pool(),
        worker::INGEST_QUEUE,
        &json!({ "org_id": org_id, "source_id": source_id }),
    )
    .await
    .expect("enqueue");
    let mut dead = false;
    for _ in 0..(queue::MAX_ATTEMPTS + 3) {
        let jobs = queue::read(store.pool(), worker::INGEST_QUEUE, 1, 0)
            .await
            .expect("read");
        let Some(job) = jobs.into_iter().next() else {
            break;
        };
        // simulate the worker's failure handling with zero backoff
        let retried = queue::fail(store.pool(), &job, 0).await.expect("fail");
        if !retried {
            dead = true;
            break;
        }
    }
    assert!(dead, "job dead-letters instead of retrying forever");
    assert_eq!(
        queue::depth(store.pool(), worker::INGEST_QUEUE)
            .await
            .expect("depth"),
        0,
        "no live job left"
    );
    let dl = queue::dead_letters(store.pool(), worker::INGEST_QUEUE, 10)
        .await
        .expect("dl");
    assert_eq!(dl.len(), 1, "exactly one dead letter recorded");
}
