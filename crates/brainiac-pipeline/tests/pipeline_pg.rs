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
    let provider = perfect_mock(&fx);

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
    let stats = worker::tick(&store, &provider, &embedder, version, 10)
        .await
        .expect("tick");

    // ── plumbing assertions ──────────────────────────────────────────────
    let gold_total: usize = fx.transcripts.iter().map(|t| t.gold_memories.len()).sum();
    assert_eq!(stats.jobs, 3, "three sources processed");
    assert_eq!(
        stats.memories, gold_total,
        "one memory per gold item, none dropped"
    );

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    use sqlx::Row;

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

    // Queue drained.
    assert_eq!(
        brainiac_store::queue::depth(store.pool(), worker::INGEST_QUEUE)
            .await
            .expect("depth"),
        0
    );
}

#[tokio::test]
#[allow(clippy::assertions_on_constants)] // deliberate: tuning-coherence guard
async fn resolve_thresholds_are_ordered() {
    // Pure sanity: the band boundaries stay coherent if someone tunes them.
    assert!(resolve::ADJUDICATION_FLOOR < resolve::AUTO_LINK_SIMILARITY);
    assert!(resolve::ADJUDICATION_AUTO_CONFIDENCE > 0.5);
}
