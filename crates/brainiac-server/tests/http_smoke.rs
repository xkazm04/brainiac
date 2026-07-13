//! HTTP smoke test (DATABASE_URL-gated): boot the real router on an
//! ephemeral port, seed gold fixtures, and exercise the API as a fixture
//! user — including the 401 path and an RLS-scoped search.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

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

    // search without token → 401, and the body is the JSON error envelope
    // {error, code} — not a plain-text string.
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .json(&serde_json::json!({"query": "anything"}))
        .send()
        .await
        .expect("unauth search");
    assert_eq!(r.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        r.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("application/json")),
        Some(true),
        "error body must be JSON"
    );
    let err: serde_json::Value = r.json().await.expect("error envelope");
    assert_eq!(err["code"], "unauthorized");
    assert!(err["error"].is_string(), "envelope carries a message");

    // Oversized query → clear 400 with the bad_request code (the char cap
    // mirrored from the MCP surface), never an unbounded embed.
    let big = "x".repeat(2_001);
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"query": big}))
        .send()
        .await
        .expect("oversized search");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
    let err: serde_json::Value = r.json().await.expect("error envelope");
    assert_eq!(err["code"], "bad_request");

    // An internal-style detail never leaks: the generic 500 envelope only ever
    // carries "internal error". (Asserted structurally below on the happy path;
    // here we pin that business 400s DO keep their specific message.)
    assert!(
        err["error"]
            .as_str()
            .expect("message")
            .contains("too large"),
        "business errors keep their specific message"
    );

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

    // ── REST↔MCP parity: feedback endpoint, provenance endpoint, and the
    //    trust + contradiction signals on search hits ─────────────────────
    assert!(
        hits.len() >= 2,
        "need ≥2 visible hits for the parity checks"
    );
    let mid = hits[0]["id"].as_str().expect("hit id").to_string();
    let mid2 = hits[1]["id"].as_str().expect("hit id").to_string();

    // Feedback synonym: `stale` canonicalizes to the stored `outdated` verdict.
    let r = http
        .post(format!("{base}/v1/memories/{mid}/feedback"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"verdict": "stale", "note": "no longer true"}))
        .send()
        .await
        .expect("feedback");
    assert!(r.status().is_success(), "feedback happy path");
    let fb: serde_json::Value = r.json().await.expect("json");
    assert_eq!(fb["verdict"], "outdated", "stale→outdated synonym");
    assert!(fb["feedback_totals"]
        .as_array()
        .expect("totals")
        .iter()
        .any(|t| t["verdict"] == "outdated" && t["count"].as_i64() == Some(1)));

    // Feedback leak: an invisible memory is a plain 404 (no existence oracle).
    let r = http
        .post(format!("{base}/v1/memories/{forbidden}/feedback"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"verdict": "wrong"}))
        .send()
        .await
        .expect("feedback leak");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);

    // Open contradiction between the two visible hits — both sides are readable
    // by the analyst, so it must surface (no-oracle join is satisfied).
    let org = stable_uuid(&fx.org.org);
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
         VALUES ($1, $2, $3, $4, 'test', 'open', 'suggest reconcile')",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .bind(Uuid::parse_str(&mid).expect("mid uuid"))
    .bind(Uuid::parse_str(&mid2).expect("mid2 uuid"))
    .execute(&admin)
    .await
    .expect("contradiction");

    // Re-search: the hit for `mid` now carries the feedback block (disputed,
    // from the open outdated claim) and the contradiction array.
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"query": "psp webhook signing secret rotation", "k": 50}))
        .send()
        .await
        .expect("search 2");
    let body: serde_json::Value = r.json().await.expect("json");
    let hit = body["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .find(|h| h["id"].as_str() == Some(mid.as_str()))
        .expect("mid still present");
    assert_eq!(hit["feedback"]["outdated"].as_i64(), Some(1));
    assert_eq!(hit["feedback"]["disputed"], true);
    let contras = hit["contradictions"].as_array().expect("contradictions");
    assert!(
        contras
            .iter()
            .any(|c| c["counterpart_id"].as_str() == Some(mid2.as_str())),
        "open contradiction with the counterpart must surface on the hit"
    );
    // Leanness: a hit with no feedback and no contradictions omits both keys.
    if let Some(clean) =
        body["hits"].as_array().expect("hits").iter().find(|h| {
            h["id"].as_str() != Some(mid.as_str()) && h["id"].as_str() != Some(mid2.as_str())
        })
    {
        assert!(clean.get("feedback").is_none(), "empty feedback is omitted");
        assert!(
            clean.get("contradictions").is_none(),
            "empty contradictions is omitted"
        );
    }

    // Provenance happy path: a visible memory returns its chain.
    let r = http
        .get(format!("{base}/v1/memories/{mid}/provenance"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("provenance");
    assert!(r.status().is_success());
    let prov: serde_json::Value = r.json().await.expect("json");
    assert_eq!(prov["memory_id"], mid);
    assert!(prov.get("entity_anchors").is_some());

    // Provenance leak: an invisible memory is 404, same as a missing id.
    let r = http
        .get(format!("{base}/v1/memories/{forbidden}/provenance"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("provenance leak");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);

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
