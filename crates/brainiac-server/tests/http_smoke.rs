//! HTTP smoke test (DATABASE_URL-gated): boot the real router on an
//! ephemeral port, seed gold fixtures, and exercise the API as a fixture
//! user — including the 401 path and an RLS-scoped search.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;

#[tokio::test]
async fn serve_health_auth_and_scoped_search() {
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
    let embedder = std::sync::Arc::new(DeterministicEmbedder::default());
    brainiac_eval::seed::seed_gold(&store, &fx, embedder.as_ref())
        .await
        .expect("seed");

    // Token for the data analyst (team-data only).
    let tokens = serde_json::json!({
        "tok_analyst": {
            "org": stable_uuid(&fx.org.org),
            "user": stable_uuid("user-data-analyst1"),
            "teams": [stable_uuid("team-data")],
        }
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    // Boot the app on an ephemeral port. NOTE: main.rs wires this same
    // router; the test uses the library-level construction.
    let app = brainiac_server_router(store, embedder).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();

    // health — no auth needed.
    let r = http
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health");
    assert!(r.status().is_success());

    // search without token → 401.
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .json(&serde_json::json!({"query": "anything"}))
        .send()
        .await
        .expect("unauth search");
    assert_eq!(r.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Scoped search: the analyst asks a payments-team question. Org-visible
    // knowledge surfaces; the payments team-private webhook memory must not.
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"query": "psp webhook signing secret rotation", "k": 50}))
        .send()
        .await
        .expect("search");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let hits = body["hits"].as_array().expect("hits");
    let forbidden = stable_uuid("mem-pay-0055").to_string();
    assert!(
        hits.iter()
            .all(|h| h["id"].as_str() != Some(forbidden.as_str())),
        "team-private memory leaked through the HTTP surface"
    );

    // memory_add → 202 with a queued job.
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"content": "manual note: event-lake backfills run at 04:00"}))
        .send()
        .await
        .expect("add");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
}

async fn brainiac_server_router(
    store: Store,
    embedder: std::sync::Arc<DeterministicEmbedder>,
) -> axum::Router {
    brainiac_server::http::router(store, embedder, None)
        .await
        .expect("router")
}
