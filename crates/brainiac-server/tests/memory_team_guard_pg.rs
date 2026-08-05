//! A memory write with no team is REFUSED, and one with a team still round-trips
//! (DATABASE_URL-gated).
//!
//! The defect this pins: `.env.deploy.example` ships `"teams": []`, so the
//! default first-run principal has no team. A memory defaults to
//! `visibility='team'` and takes its team from the writer's first team, so its
//! `team_id` was NULL — and `memories_read` (migrations/0001_init.sql) matches
//! team-visible rows by `team_id IN (SELECT … FROM team_members …)`, which NULL
//! satisfies for nobody, including the writer. The write returned 202, the
//! pipeline reported `processed`, and the row was unreachable forever with no
//! error anywhere: silent, permanent data loss on the DEFAULT path.
//!
//! Both halves matter. The reject half proves the loss is now loud; the
//! round-trip half is the guard against over-rejecting — it drives the real
//! router, the real queue, the real worker, and reads the memory back through
//! RLS as the writing principal.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::Principal;
use brainiac_gateway::{ChatRequest, MockProvider, ProviderRouter};
use brainiac_store::Store;
use uuid::Uuid;

const ORG: &str = "00000000-0000-0000-0000-0000000000a1";
const USER: &str = "00000000-0000-0000-0000-0000000000a2";
const TEAM: &str = "00000000-0000-0000-0000-0000000000a3";
/// A marker no fixture corpus could produce by accident, so the read-back is a
/// real assertion rather than "some row came back".
const MARKER: &str = "teamguard-roundtrip-marker-9f31";

#[tokio::test]
async fn teamless_write_is_refused_and_a_teamed_write_reads_back() {
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

    let org_id = Uuid::parse_str(ORG).expect("org uuid");
    let user_id = Uuid::parse_str(USER).expect("user uuid");
    let team_id = Uuid::parse_str(TEAM).expect("team uuid");

    let store = Store::connect(&url).await.expect("connect");
    // Identity. The `team_members` row is the load-bearing part: RLS reads the
    // TABLE, not the token's `teams` array, so a token that merely NAMES a team
    // is not enough for the row to be readable.
    {
        let bootstrap = Principal {
            org_id,
            user_id,
            team_ids: vec![team_id],
            project_id: None,
        };
        let mut tx = store.scoped_tx(&bootstrap).await.expect("tx");
        brainiac_store::orgs::upsert_org(&mut tx, org_id, "team-guard-org")
            .await
            .expect("org");
        brainiac_store::orgs::upsert_team(&mut tx, team_id, org_id, "workspace")
            .await
            .expect("team");
        brainiac_store::orgs::upsert_user(&mut tx, user_id, org_id, "guard@example.test")
            .await
            .expect("user");
        brainiac_store::orgs::upsert_member(&mut tx, team_id, user_id, "maintainer")
            .await
            .expect("member");
        tx.commit().await.expect("commit");
    }

    // Two tokens, one principal difference: a team, or none. `tok_teamless` is
    // exactly what `.env.deploy.example` ships.
    let tokens = serde_json::json!({
        "tok_teamless": { "org": org_id, "user": user_id, "teams": [] },
        "tok_teamed":   { "org": org_id, "user": user_id, "teams": [team_id] },
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let embedder = std::sync::Arc::new(DeterministicEmbedder::default());
    let app = brainiac_server::http::router(store.clone(), embedder.clone(), None)
        .await
        .expect("router");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();

    // ── (a) the teamless write is refused, loudly ────────────────────────
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_teamless")
        .json(&serde_json::json!({ "content": "A teamless principal writes something." }))
        .send()
        .await
        .expect("teamless add");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a teamless write must fail, not return 202 and vanish"
    );
    let err: serde_json::Value = r.json().await.expect("error envelope");
    assert_eq!(err["code"], "bad_request");
    let msg = err["error"].as_str().expect("message");
    // The operator reads this in a curl response and must learn the CAUSE and a
    // remedy that actually works — both halves, since a `teams` array without a
    // `team_members` row leaves the row just as unreadable.
    assert!(msg.contains("no team"), "names the cause: {msg}");
    assert!(msg.contains("team_members"), "names the remedy: {msg}");

    // …and nothing was stored. A refused write that still wrote a source would
    // just move the silent loss one table over.
    let sources: i64 = sqlx::query_scalar("SELECT count(*) FROM sources WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&admin)
        .await
        .expect("count sources");
    assert_eq!(sources, 0, "a refused write must store nothing");

    // Same refusal on the keyed (Idempotency-Key) path — it resolves the team
    // itself rather than going through `ingest_source`.
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_teamless")
        .header("Idempotency-Key", "guard-key-1")
        .json(&serde_json::json!({ "content": "A teamless principal retries." }))
        .send()
        .await
        .expect("teamless keyed add");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the Idempotency-Key path must refuse a teamless write too"
    );

    // And per-item on bulk: one bad item is that ITEM's error, never a 400 on
    // the batch — the existing bulk contract, unchanged.
    let r = http
        .post(format!("{base}/v1/memories/bulk"))
        .bearer_auth("tok_teamless")
        .json(&serde_json::json!({ "items": [{ "content": "bulk teamless" }] }))
        .send()
        .await
        .expect("teamless bulk");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["results"][0]["code"], "bad_request");
    assert!(body["results"][0]["source_id"].is_null());

    // ── (b) a principal WITH a team still writes, and can read it back ───
    let statement =
        format!("The deployment's write-then-retrieve round trip was confirmed. Marker {MARKER}.");
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_teamed")
        .json(&serde_json::json!({ "content": statement }))
        .send()
        .await
        .expect("teamed add");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::ACCEPTED,
        "the guard must not over-reject a principal who HAS a team"
    );

    // Drain the queue with a stub extractor that emits one team-visible memory
    // carrying the marker — i.e. exactly the default the real extractor applies
    // when the model says nothing (`Visibility::Team`), which is the case that
    // used to be lost.
    let extraction = serde_json::json!({
        "memories": [{
            "kind": "fact",
            "title": "Install verification",
            "content": statement,
        }]
    })
    .to_string();
    let provider = ProviderRouter::single(std::sync::Arc::new(MockProvider::new(
        move |req: &ChatRequest| {
            if req.system.contains("distill organizational knowledge") {
                extraction.clone()
            } else {
                r#"{"memories":[]}"#.to_string()
            }
        },
    )));
    let pipeline_principal = brainiac_pipeline::pipeline_principal(org_id);
    let version = {
        let mut tx = store.scoped_tx(&pipeline_principal).await.expect("tx");
        let v = brainiac_store::memories::ensure_embedding_version(
            &mut tx,
            brainiac_core::embed::Embedder::model_name(embedder.as_ref()),
            brainiac_core::embed::Embedder::dim(embedder.as_ref()) as i32,
        )
        .await
        .expect("version");
        tx.commit().await.expect("commit");
        v
    };
    let stats = brainiac_pipeline::worker::tick(
        &store,
        &provider,
        embedder.as_ref(),
        version,
        &brainiac_pipeline::worker::WorkerConfig::default(),
    )
    .await
    .expect("tick");
    assert_eq!(stats.jobs, 1, "the teamed source was the only job queued");

    // The row exists, and it is the shape that used to be unreadable:
    // visibility='team' with a non-null team.
    let (vis, stored_team): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT visibility::text, team_id FROM memories WHERE org_id = $1 AND content LIKE $2",
    )
    .bind(org_id)
    .bind(format!("%{MARKER}%"))
    .fetch_one(&admin)
    .await
    .expect("the extracted memory exists");
    assert_eq!(vis, "team", "the default visibility is the lossy case");
    assert_eq!(
        stored_team,
        Some(team_id),
        "the memory carries the writer's team, not NULL"
    );

    // THE ASSERTION THAT WOULD HAVE CAUGHT THE DEFECT: read it back through
    // RLS, as the writing principal. Under the old behaviour a teamless write
    // reached this point with team_id NULL and this count was 0.
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .bearer_auth("tok_teamed")
        .json(&serde_json::json!({ "query": MARKER, "k": 50 }))
        .send()
        .await
        .expect("search");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let hits = body["hits"].as_array().expect("hits");
    assert!(
        hits.iter()
            .any(|h| h["content"].as_str().is_some_and(|c| c.contains(MARKER))),
        "the memory a teamed principal wrote must be retrievable by that same \
         principal; got {body}"
    );
}
