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
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive,
                  knowledge_health_snapshots
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

    let app = brainiac_server::http::router(store, embedder, None)
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
    // Review velocity (P0.1): the maintainer approved one promotion above, so the
    // abandonment signal must register it and expose a median review latency.
    assert!(
        body["reviews"]["reviewed_last_7d"].as_i64().expect("r7") >= 1,
        "an approved promotion must count toward review throughput: {body}"
    );
    assert!(
        body["reviews"]["median_time_to_review_secs"].is_i64(),
        "a reviewed queue must report a median latency: {body}"
    );

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
    assert!(!body["weekly"]["captured"]
        .as_array()
        .expect("cap")
        .is_empty());
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

    // ── knowledge health: the leadership report ───────────────────────────
    let r = http
        .get(format!("{base}/v1/analytics/knowledge-health"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("knowledge-health");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let score = body["score"].as_i64().expect("score");
    assert!((0..=100).contains(&score), "score in range: {score}");
    assert!(matches!(
        body["grade"].as_str().expect("grade"),
        "Healthy" | "Watch" | "At risk" | "Critical"
    ));
    // Four pillars, all present and in range.
    for k in ["consistency", "currency", "liquidity", "governance"] {
        let v = body["pillars"][k].as_i64().unwrap_or(-1);
        assert!((0..=100).contains(&v), "pillar {k} in range: {v}");
    }
    // The signals expose the flagship org-level number.
    assert!(
        body["signals"]["canonical_entities"]
            .as_i64()
            .expect("canon")
            > 0
    );
    // The attention list is the score made actionable — at least present.
    assert!(
        body["attention"].is_array(),
        "attention list present: {body}"
    );
    // No snapshots taken yet → the trend is empty.
    assert!(
        body["trend"].as_array().expect("trend").is_empty(),
        "trend empty before any snapshot: {body}"
    );

    // Record a snapshot, then the trend carries one point at the current score.
    let r = http
        .post(format!("{base}/v1/analytics/knowledge-health/snapshot"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("snapshot");
    assert!(r.status().is_success(), "snapshot failed: {}", r.status());
    let snap: serde_json::Value = r.json().await.expect("json");
    assert_eq!(snap["score"], score, "snapshot records the live score");
    let r = http
        .get(format!("{base}/v1/analytics/knowledge-health"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("kh again");
    let body: serde_json::Value = r.json().await.expect("json");
    let trend = body["trend"].as_array().expect("trend");
    assert_eq!(trend.len(), 1, "one snapshot in the trend: {body}");
    assert_eq!(trend[0]["score"], score);

    // ── cortex map: overview ──────────────────────────────────────────────
    let r = http
        .get(format!("{base}/v1/graph/overview"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("overview");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["teams"].as_array().expect("teams").len(), 3);
    let kafka = body["canonicals"]
        .as_array()
        .expect("canonicals")
        .iter()
        .find(|c| c["name"] == "kafka")
        .expect("kafka hub");
    assert_eq!(kafka["teams"], 3);
    assert!(!body["team_links"].as_array().expect("links").is_empty());

    // ── cortex map: canonical drill-down under RLS ────────────────────────
    let kafka_id = kafka["id"].as_str().expect("id").to_string();
    let r = http
        .get(format!("{base}/v1/graph/canonical/{kafka_id}"))
        .bearer_auth("tok_analyst")
        .send()
        .await
        .expect("canonical");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["canonical"]["name"], "kafka");
    let forms = body["surface_forms"].as_array().expect("forms");
    assert_eq!(forms.len(), 3, "three team dialects of kafka");
    assert!(!body["edges"].as_array().expect("edges").is_empty());
    // The analyst must not see payments team-visible memory content anywhere
    // in the drill-down (evidence or anchored memories).
    let leak = "checkout.events.v2; the v1 topic is frozen";
    assert!(body["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .all(|e| e["evidence"].as_str().is_none_or(|s| !s.contains(leak))));
    assert!(body["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .all(|m| !m["content"].as_str().expect("content").contains(leak)));

    // ── archive: as-of time travel ────────────────────────────────────────
    // At 2026-04-01 the 10s psp timeout (mem-pay-0063) was still valid and
    // its 30s successor (mem-pay-0064) did not exist yet.
    let old_id = stable_uuid("mem-pay-0063").to_string();
    let new_id = stable_uuid("mem-pay-0064").to_string();
    let r = http
        .get(format!(
            "{base}/v1/memories?as_of=2026-04-01T00:00:00Z&limit=200"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("as-of list");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let ids: Vec<&str> = body["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .map(|m| m["id"].as_str().expect("id"))
        .collect();
    assert!(
        ids.contains(&old_id.as_str()),
        "deprecated-but-then-valid row must resurface"
    );
    assert!(
        !ids.contains(&new_id.as_str()),
        "not-yet-valid successor must be absent"
    );

    // Without as_of both exist; kind filter narrows.
    let r = http
        .get(format!("{base}/v1/memories?limit=200"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("list");
    let body: serde_json::Value = r.json().await.expect("json");
    // RLS: the payments lead sees org rows + payments-team rows only
    // (~46 of 82 — 27 org + payments' share of the 50 team-tier rows).
    assert!(body["total"].as_i64().expect("total") >= 35);
    let r = http
        .get(format!("{base}/v1/memories?kind=pitfall&limit=200"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("kind filter");
    let body: serde_json::Value = r.json().await.expect("json");
    assert!(body["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .all(|m| m["kind"] == "pitfall"));

    // ── archive: detail + supersession lineage ────────────────────────────
    let r = http
        .get(format!("{base}/v1/memories/{new_id}"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("detail");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["memory"]["id"], new_id);
    assert!(!body["entities"].as_array().expect("entities").is_empty());
    let preds = body["chain"]["predecessors"].as_array().expect("preds");
    assert!(
        preds.iter().any(|p| p["id"] == old_id),
        "10s timeout must appear as the predecessor of the 30s decision"
    );

    // ── ingest monitor: submit → feed shows it queued ─────────────────────
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({"content": "manual note: chargeback evidence window is 14 days"}))
        .send()
        .await
        .expect("memory add");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
    let submitted: serde_json::Value = r.json().await.expect("json");
    let source_id = submitted["source_id"].as_str().expect("source_id");

    let r = http
        .get(format!("{base}/v1/sources?limit=10"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("sources feed");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let feed = body["sources"].as_array().expect("sources");
    let mine = feed
        .iter()
        .find(|s| s["id"].as_str() == Some(source_id))
        .expect("submitted source in feed");
    assert_eq!(mine["status"], "queued");
    assert_eq!(mine["memories"], 0);

    let r = http
        .get(format!("{base}/v1/pipeline/runs?limit=20"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("runs");
    assert!(r.status().is_success(), "pipeline runs endpoint answers");

    // ── keys: org directory + blast-radius preview ────────────────────────
    let r = http
        .get(format!("{base}/v1/org/users"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("org users");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["users"].as_array().expect("users").len(), 6);

    // The analyst's radius: sees her own private note, org rows, data-team
    // rows — and materially less than the payments lead.
    let analyst_id = stable_uuid("user-data-analyst1").to_string();
    let r = http
        .post(format!("{base}/v1/tokens/preview"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "user_id": analyst_id }))
        .send()
        .await
        .expect("preview analyst");
    assert!(r.status().is_success());
    let analyst: serde_json::Value = r.json().await.expect("json");
    assert_eq!(analyst["teams"], serde_json::json!(["data"]));
    assert!(analyst["visible"]["private"].as_i64().expect("n") >= 1);
    assert!(analyst["visible"]["org"].as_i64().expect("n") >= 20);

    let lead_id = stable_uuid("user-pay-lead").to_string();
    let r = http
        .post(format!("{base}/v1/tokens/preview"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "user_id": lead_id }))
        .send()
        .await
        .expect("preview lead");
    let lead: serde_json::Value = r.json().await.expect("json");
    assert_eq!(lead["teams"], serde_json::json!(["payments"]));
    // Different principals → different radii; both bounded below by the org
    // tier and above by the corpus (the two team shares are near-equal in
    // this fixture, so no strict ordering is asserted).
    let lead_total = lead["visible"]["total"].as_i64().expect("n");
    let analyst_total = analyst["visible"]["total"].as_i64().expect("n");
    assert!(lead_total >= 35 && analyst_total >= 35);
    assert!(lead["visible"]["canonical"].as_i64().expect("n") > 0);

    // ── pipeline runs pagination: a trail past the first page is reachable ─
    // Mirrors the promotions paging proof on a console.rs list endpoint. Seed
    // 101 runs, then prove `offset` reaches the 101st while `total` reports the
    // full count (limit-only truncation would have hidden everything past 100).
    for i in 0..101 {
        sqlx::query(
            "INSERT INTO pipeline_runs (id, org_id, stage, status, started_at)
             VALUES ($1, $2, 'extract', 'ok', now() - make_interval(secs => $3))",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(f64::from(i))
        .execute(&admin)
        .await
        .expect("seed pipeline run");
    }
    let r = http
        .get(format!("{base}/v1/pipeline/runs?limit=100&offset=0"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("pipeline runs page 1");
    assert!(r.status().is_success());
    let page1: serde_json::Value = r.json().await.expect("json");
    let total = page1["total"].as_i64().expect("total");
    assert!(total >= 101, "total reports the full trail, got {total}");
    assert_eq!(
        page1["runs"].as_array().expect("runs").len(),
        100,
        "limit is honoured"
    );
    let first_ids: std::collections::HashSet<String> = page1["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|p| p["id"].as_str().expect("id").to_string())
        .collect();
    let r = http
        .get(format!("{base}/v1/pipeline/runs?limit=100&offset=100"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("pipeline runs page 2");
    let page2: serde_json::Value = r.json().await.expect("json");
    assert_eq!(page2["total"].as_i64(), Some(total), "total is page-stable");
    let page2_rows = page2["runs"].as_array().expect("runs");
    assert!(
        !page2_rows.is_empty() && page2_rows.len() as i64 >= total - 100,
        "the 101st+ runs are reachable past offset 100"
    );
    assert!(
        page2_rows
            .iter()
            .all(|p| !first_ids.contains(p["id"].as_str().expect("id"))),
        "the second page returns rows the first page never showed"
    );

    // ── operator sweeps: schedule + run-now (admin) ───────────────────────
    // sweep_schedules is global config (not in the TRUNCATE list), so a prior
    // run of this test may have armed it — reset the seeded rows to a known
    // baseline before asserting.
    sqlx::query(
        "UPDATE sweep_schedules
         SET enabled = false, cadence_secs = 604800, next_run_at = NULL,
             last_status = NULL, last_detail = NULL, last_duration_ms = NULL",
    )
    .execute(&admin)
    .await
    .expect("reset sweeps");

    // List: both seeded sweeps, disabled and never run.
    let r = http
        .get(format!("{base}/v1/ops/sweeps"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("list sweeps");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    let sweeps = body["sweeps"].as_array().expect("sweeps");
    assert_eq!(sweeps.len(), 2, "two seeded sweeps: {body}");
    assert!(
        sweeps
            .iter()
            .all(|s| s["enabled"] == false && s["last_status"].is_null()),
        "seeded sweeps start disabled and unrun: {body}"
    );
    assert!(
        sweeps.iter().any(|s| s["kind"] == "divergence")
            && sweeps.iter().any(|s| s["kind"] == "health_snapshot"),
        "both known sweep kinds present: {body}"
    );

    // Enable divergence at an hourly cadence → it arms next_run_at.
    let r = http
        .put(format!("{base}/v1/ops/sweeps/divergence"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({"enabled": true, "cadence_secs": 3600}))
        .send()
        .await
        .expect("enable divergence");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["cadence_secs"], 3600);
    assert!(
        body["next_run_at"].is_string(),
        "enabling arms the schedule: {body}"
    );

    // A cadence below the floor is rejected.
    let r = http
        .put(format!("{base}/v1/ops/sweeps/divergence"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({"cadence_secs": 30}))
        .send()
        .await
        .expect("too-fast cadence");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);

    // Run-now on a disabled sweep still arms it (a one-shot trigger).
    let r = http
        .post(format!("{base}/v1/ops/sweeps/health_snapshot/run"))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("run health snapshot");
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["queued"], true);
    assert!(
        body["next_run_at"].is_string(),
        "run-now arms next_run_at: {body}"
    );

    // An unknown sweep kind is a 404, not a silent no-op.
    let r = http
        .put(format!("{base}/v1/ops/sweeps/nonesuch"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({"enabled": true}))
        .send()
        .await
        .expect("unknown kind");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);
}
