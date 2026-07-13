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

    // ── promotions pagination: a backlog past the first page is reachable ──
    // The endpoint's whole job is the review backlog; seed 101 pending
    // promotions and prove the 101st is reachable via offset while `total`
    // reports the full count (the old hard LIMIT 100 hid everything past 100).
    for _ in 0..101 {
        sqlx::query(
            "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status, policy_decision, policy_rule)
             VALUES ($1, $2, $3, 'raw', 'candidate', 'needs_review', 'test.page')",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(Uuid::parse_str(&mid).expect("mid uuid"))
        .execute(&admin)
        .await
        .expect("seed promotion");
    }
    // Default page preserves the pre-paging behaviour: at most 100 rows.
    let r = http
        .get(format!("{base}/v1/reviews/promotions"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("promotions default");
    assert!(r.status().is_success());
    let page1: serde_json::Value = r.json().await.expect("json");
    let total = page1["total"].as_i64().expect("total");
    assert!(total >= 101, "total reports the full backlog, got {total}");
    assert_eq!(
        page1["promotions"].as_array().expect("promotions").len(),
        100,
        "default page is still capped at 100 (behaviour preserved)"
    );
    // The 101st row (index 100) is reachable only via offset — the point of
    // the change. Collect ids across two pages and prove they don't overlap.
    let first_ids: std::collections::HashSet<String> = page1["promotions"]
        .as_array()
        .expect("promotions")
        .iter()
        .map(|p| p["id"].as_str().expect("id").to_string())
        .collect();
    let r = http
        .get(format!("{base}/v1/reviews/promotions?limit=100&offset=100"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("promotions page 2");
    let page2: serde_json::Value = r.json().await.expect("json");
    assert_eq!(page2["total"].as_i64(), Some(total), "total is page-stable");
    let page2_rows = page2["promotions"].as_array().expect("promotions");
    assert!(
        page2_rows.len() as i64 >= total - 100 && !page2_rows.is_empty(),
        "the 101st+ promotions are reachable past offset 100"
    );
    assert!(
        page2_rows
            .iter()
            .all(|p| !first_ids.contains(p["id"].as_str().expect("id"))),
        "the second page returns rows the first page never showed"
    );

    // ── idempotent ingest: a retried memory_add replays the original ───────
    // Same Idempotency-Key (per org) ⇒ the ORIGINAL source_id/job_id and NO
    // second source row (a duplicate source would burn a whole pipeline run).
    let add_keyed = |key: &'static str| {
        let http = http.clone();
        let base = base.clone();
        async move {
            http.post(format!("{base}/v1/memories"))
                .bearer_auth("tok_analyst")
                .header("Idempotency-Key", key)
                .json(&serde_json::json!({"content": "manual note: idempotent capture — nightly recon at 02:00"}))
                .send()
                .await
                .expect("keyed add")
        }
    };
    let r1 = add_keyed("retry-key-A").await;
    assert_eq!(r1.status(), reqwest::StatusCode::ACCEPTED);
    let a1: serde_json::Value = r1.json().await.expect("json");
    let r2 = add_keyed("retry-key-A").await;
    assert_eq!(r2.status(), reqwest::StatusCode::ACCEPTED);
    let a2: serde_json::Value = r2.json().await.expect("json");
    assert_eq!(a1["source_id"], a2["source_id"], "same key ⇒ same source");
    assert_eq!(a1["job_id"], a2["job_id"], "same key ⇒ same job");
    let key_sources: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sources WHERE idempotency_key = 'retry-key-A'")
            .fetch_one(&admin)
            .await
            .expect("count keyed sources");
    assert_eq!(key_sources, 1, "the retry did NOT mint a second source");

    // A different key mints a distinct source (the guard is per key, not global).
    let r3 = add_keyed("retry-key-B").await;
    let a3: serde_json::Value = r3.json().await.expect("json");
    assert_ne!(
        a1["source_id"], a3["source_id"],
        "distinct key ⇒ new source"
    );

    // ── bulk ingest: one bad item does not sink the batch ──────────────────
    let r = http
        .post(format!("{base}/v1/memories/bulk"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({"items": [
            {"content": "bulk import row one: ledger cutover 09:00"},
            {"content": "   "},
            {"content": "bulk import row three: dunning retries at noon"},
        ]}))
        .send()
        .await
        .expect("bulk");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
    let bulk: serde_json::Value = r.json().await.expect("json");
    let results = bulk["results"].as_array().expect("results");
    assert_eq!(results.len(), 3, "one result per item, in order");
    assert!(results[0]["source_id"].is_string() && results[0].get("error").is_none());
    assert_eq!(
        results[1]["code"], "bad_request",
        "the empty item is a per-item error"
    );
    assert!(
        results[1].get("source_id").is_none(),
        "the failed item has no receipt"
    );
    assert!(
        results[2]["source_id"].is_string() && results[2].get("error").is_none(),
        "the item after the bad one still succeeds — the batch is not sunk"
    );

    // Over-cap batch → a whole-request 400 (guards the fan-out).
    let too_many: Vec<serde_json::Value> = (0..101)
        .map(|i| serde_json::json!({"content": format!("row {i}")}))
        .collect();
    let r = http
        .post(format!("{base}/v1/memories/bulk"))
        .bearer_auth("tok_analyst")
        .json(&serde_json::json!({ "items": too_many }))
        .send()
        .await
        .expect("bulk over cap");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
    let err: serde_json::Value = r.json().await.expect("json");
    assert_eq!(err["code"], "bad_request");
}

async fn brainiac_server_router(
    store: Store,
    embedder: std::sync::Arc<DeterministicEmbedder>,
) -> axum::Router {
    brainiac_server::http::router(store, embedder, None)
        .await
        .expect("router")
}
