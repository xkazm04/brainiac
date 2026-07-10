//! Console REST test (DATABASE_URL-gated): review actions with the
//! maintainer gate, contradiction resolution incl. supersession, graph and
//! analytics — all exercised over the wire as fixture users.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

#[tokio::test]
async fn console_reviews_graph_analytics() {
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

    let org = stable_uuid(&fx.org.org);
    let team_pay = stable_uuid("team-payments");

    // A raw payments memory with two pending promotions (approve + reject
    // paths) — inserted via admin, as the pipeline would.
    let raw_mem = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'team', 'raw', 'fact', 'raw candidate: settlement recon runs at 07:00')",
    )
    .bind(raw_mem)
    .bind(org)
    .bind(team_pay)
    .execute(&admin)
    .await
    .expect("raw memory");
    let promo_approve = Uuid::new_v4();
    let promo_reject = Uuid::new_v4();
    for id in [promo_approve, promo_reject] {
        sqlx::query(
            "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status, policy_decision, policy_rule)
             VALUES ($1, $2, $3, 'raw', 'candidate', 'needs_review', 'test.fixture')",
        )
        .bind(id)
        .bind(org)
        .bind(raw_mem)
        .execute(&admin)
        .await
        .expect("promotion");
    }
    // An open contradiction between the two psp-timeout memories.
    let contradiction = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
         VALUES ($1, $2, $3, $4, 'test', 'open', 'suggest supersede')",
    )
    .bind(contradiction)
    .bind(org)
    .bind(stable_uuid("mem-pay-0063"))
    .bind(stable_uuid("mem-pay-0064"))
    .execute(&admin)
    .await
    .expect("contradiction");

    // Tokens: payments member, payments maintainer (lead), data analyst.
    let tok = |user: &str, teams: Vec<Uuid>| serde_json::json!({"org": org, "user": stable_uuid(user), "teams": teams});
    let tokens = serde_json::json!({
        "tok_pay_dev": tok("user-pay-dev1", vec![team_pay]),
        "tok_pay_lead": tok("user-pay-lead", vec![team_pay]),
        "tok_analyst": tok("user-data-analyst1", vec![stable_uuid("team-data")]),
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let app = brainiac_server::http::router(store, embedder)
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

    // ── promotions: member forbidden, outsider gets 404, maintainer works ─
    let r = http
        .post(format!(
            "{base}/v1/reviews/promotions/{promo_approve}/approve"
        ))
        .bearer_auth("tok_pay_dev")
        .send()
        .await
        .expect("member approve");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    // The analyst can't even see the team-visible memory → 404, not 403.
    let r = http
        .post(format!(
            "{base}/v1/reviews/promotions/{promo_approve}/approve"
        ))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("outsider approve");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);

    let r = http
        .post(format!(
            "{base}/v1/reviews/promotions/{promo_approve}/approve"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("lead approve");
    assert!(r.status().is_success(), "maintainer approve failed");
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["memory_status"], "candidate");

    // Approving again → no longer pending.
    let r = http
        .post(format!(
            "{base}/v1/reviews/promotions/{promo_approve}/approve"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("double approve");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);

    // Reject the second one → memory goes to rejected.
    let r = http
        .post(format!(
            "{base}/v1/reviews/promotions/{promo_reject}/reject"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("reject");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["memory_status"], "rejected");

    // ── contradictions: listed for the team, resolvable by the maintainer ─
    let r = http
        .get(format!("{base}/v1/reviews/contradictions"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("list contradictions");
    let body: serde_json::Value = r.json().await.expect("json");
    let list = body["contradictions"].as_array().expect("array");
    assert_eq!(list.len(), 1);
    assert!(list[0]["memory_a"]["content"].is_string());

    // Supersede: 0064 (30s) wins over 0063 (10s). Member is refused first.
    let resolve_body = serde_json::json!({
        "resolution": "supersede",
        "winner_memory_id": stable_uuid("mem-pay-0064"),
        "note": "incident review confirmed 30s"
    });
    let r = http
        .post(format!(
            "{base}/v1/reviews/contradictions/{contradiction}/resolve"
        ))
        .bearer_auth("tok_pay_dev")
        .json(&resolve_body)
        .send()
        .await
        .expect("member resolve");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    let r = http
        .post(format!(
            "{base}/v1/reviews/contradictions/{contradiction}/resolve"
        ))
        .bearer_auth("tok_pay_lead")
        .json(&resolve_body)
        .send()
        .await
        .expect("lead resolve");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["status"], "resolved_supersede");

    // The losing memory is now deprecated + points at the winner.
    let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
        "SELECT status::text, superseded_by FROM memories WHERE id = $1",
    )
    .bind(stable_uuid("mem-pay-0063"))
    .fetch_one(&admin)
    .await
    .expect("loser row");
    assert_eq!(row.0, "deprecated");
    assert_eq!(row.1, Some(stable_uuid("mem-pay-0064")));

    // ── graph: hubs + linked entities + evidence under RLS ────────────────
    let r = http
        .get(format!("{base}/v1/graph"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("graph");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert!(!body["canonicals"]
        .as_array()
        .expect("canonicals")
        .is_empty());
    let entities = body["entities"].as_array().expect("entities");
    assert!(
        entities.iter().any(|e| e["canonical_id"].is_string()),
        "no entity links in graph"
    );
    let edges = body["edges"].as_array().expect("edges");
    assert!(!edges.is_empty());
    // Evidence text of payments team-private memories must be null for the
    // analyst even though the edge metadata is org-visible.
    let forbidden = stable_uuid("mem-pay-0055").to_string();
    assert!(edges
        .iter()
        .filter(|e| e["memory_id"].as_str() == Some(forbidden.as_str()))
        .all(|e| e["evidence"].is_null()));

    // ── analytics ─────────────────────────────────────────────────────────
    let r = http
        .get(format!("{base}/v1/analytics"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("analytics");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["reviews"]["pending_promotions"], 0);
    assert_eq!(body["reviews"]["open_contradictions"], 0);
    assert!(body["graph"]["canonicals"].as_i64().expect("n") > 0);
    assert!(body["memories_by_status"].as_array().expect("arr").len() >= 2);

    // ── observatory ───────────────────────────────────────────────────────
    let r = http
        .get(format!("{base}/v1/analytics/observatory"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("observatory");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert!(!body["totals"].as_array().expect("totals").is_empty());
    assert!(!body["weekly"]["captured"].as_array().expect("cap").is_empty());
    assert!(!body["by_kind"].as_array().expect("kinds").is_empty());
    // kafka is the flagship theme: 3 team-scoped surface forms, memories > 0.
    let kafka = body["top_entities"]
        .as_array()
        .expect("entities")
        .iter()
        .find(|e| e["name"] == "kafka")
        .expect("kafka canonical present");
    assert_eq!(kafka["teams"], 3);
    assert!(kafka["memories"].as_i64().expect("n") > 0);
    // The reviews earlier in this test are reflected in the ledger stats.
    assert!(body["review"]["reviewed"].as_i64().expect("n") >= 2);
    assert!(body["review"]["avg_latency_secs"].as_i64().expect("n") >= 0);
    assert!(body["contradictions"]
        .as_array()
        .expect("contradictions")
        .iter()
        .any(|c| c["status"] == "resolved_supersede"));
}
