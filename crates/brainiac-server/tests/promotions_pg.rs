//! Promotion review queue at scale (DATABASE_URL-gated).
//!
//! The queue endpoint is paged, and the console renders `total` as the backlog.
//! That contract only matters past the page window — every assertion here is
//! about a backlog BIGGER than one page, which is the case the old hard
//! `LIMIT 100` hid and the client then reported as the whole queue.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

/// Comfortably past the server's `limit` ceiling of 200, so a single page can
/// never accidentally contain the backlog and pass these tests by luck.
const BACKLOG: usize = 512;

#[tokio::test]
async fn promotion_queue_pages_and_counts_the_whole_backlog() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
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

    // A backlog that does not fit in a page. Ages are staggered so the queue's
    // `ORDER BY created_at ASC` is a total order we can actually assert on.
    for i in 0..BACKLOG {
        let mem = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'raw', 'fact', $4)",
        )
        .bind(mem)
        .bind(org)
        .bind(team_pay)
        .bind(format!("raw candidate #{i}"))
        .execute(&admin)
        .await
        .expect("raw memory");
        sqlx::query(
            "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status,
                                     policy_decision, policy_rule, created_at)
             VALUES ($1, $2, $3, 'raw', 'candidate', 'needs_review', 'test.backlog',
                     now() - ($4 || ' minutes')::interval)",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(mem)
        .bind((BACKLOG - i).to_string())
        .execute(&admin)
        .await
        .expect("promotion");
    }

    // The number the UI must render. Everything below is checked against THIS,
    // not against a page length.
    let db_total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM promotions WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL")
            .fetch_one(&admin)
            .await
            .expect("count");
    assert_eq!(db_total, BACKLOG as i64, "seed did not land");

    let tok = |user: &str, teams: Vec<Uuid>| serde_json::json!({"org": org, "user": stable_uuid(user), "teams": teams});
    std::env::set_var(
        "BRAINIAC_TOKENS",
        serde_json::json!({ "tok_pay_lead": tok("user-pay-lead", vec![team_pay]) }).to_string(),
    );

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

    let get = |qs: String| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let r = http
                .get(format!("{base}/v1/reviews/promotions{qs}"))
                .bearer_auth("tok_pay_lead")
                .send()
                .await
                .expect("queue");
            assert!(r.status().is_success(), "queue failed: {}", r.status());
            r.json::<serde_json::Value>().await.expect("json")
        }
    };

    // ── total is the backlog, on every page, at every window ─────────────
    //
    // THE REGRESSION: the console rendered `promotions.length` here. On this
    // corpus that reads "100 promotions waiting" over a backlog of 512 — and it
    // is the array the team filter searches, so a team past row 100 shows zero.
    let body = get(String::new()).await;
    let page = body["promotions"].as_array().expect("promotions array");
    assert_eq!(body["total"], db_total, "total must be the DB count(*)");
    assert_eq!(page.len(), 100, "default page size");
    assert!(
        (page.len() as i64) < body["total"].as_i64().expect("total"),
        "page length must NOT be mistakable for the backlog in this fixture",
    );

    // The ceiling: limit clamps to 200, and `total` is unmoved by the window.
    let body = get("?limit=200".into()).await;
    assert_eq!(
        body["promotions"].as_array().expect("promotions").len(),
        200
    );
    assert_eq!(body["total"], db_total);
    let body = get("?limit=9999".into()).await;
    assert_eq!(
        body["promotions"].as_array().expect("promotions").len(),
        200,
        "limit must clamp to 200 rather than serving the whole backlog"
    );
    assert_eq!(body["total"], db_total);

    // ── offset reaches the backlog past page one, without gap or overlap ──
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0;
    loop {
        let body = get(format!("?limit=200&offset={offset}")).await;
        assert_eq!(body["total"], db_total, "total must not drift while paging");
        let rows = body["promotions"].as_array().expect("promotions");
        if rows.is_empty() {
            break;
        }
        seen.extend(
            rows.iter()
                .map(|r| r["id"].as_str().expect("id").to_string()),
        );
        offset += rows.len();
    }
    assert_eq!(
        seen.len(),
        BACKLOG,
        "paging must reach every row in the backlog"
    );
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), BACKLOG, "paging must not repeat a row");

    // Oldest first, and stable across the page seam — a rail that reshuffles
    // between pages would show the same claim twice and skip another.
    let ages: Vec<i64> = {
        let mut out = Vec::new();
        let mut offset = 0;
        while offset < BACKLOG {
            let body = get(format!("?limit=200&offset={offset}")).await;
            let rows = body["promotions"].as_array().expect("promotions");
            out.extend(
                rows.iter()
                    .map(|r| r["age_secs"].as_i64().expect("age_secs")),
            );
            offset += rows.len();
        }
        out
    };
    assert!(
        ages.windows(2).all(|w| w[0] >= w[1]),
        "queue must be oldest-first across page boundaries"
    );

    // An offset past the end is an empty PAGE, not an empty queue — the count
    // still reports the backlog, so the UI can say "nothing here" honestly.
    let body = get("?offset=100000".into()).await;
    assert!(body["promotions"]
        .as_array()
        .expect("promotions")
        .is_empty());
    assert_eq!(body["total"], db_total);
}

/// The bulk endpoint applies the maintainer gate PER ITEM. A batch that mixes a
/// caller's own team with another team's must approve the former and refuse the
/// latter — and must not move a single memory it was not entitled to move. This
/// is the property that makes bulk safe: it is N single reviews, not one blanket
/// authorization.
#[tokio::test]
async fn bulk_review_gates_every_item_independently() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
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
    let team_data = stable_uuid("team-data");

    // Insert a raw memory + a pending promotion, return the ids.
    let seed_promo = |team: Uuid, tag: &'static str| {
        let admin = admin.clone();
        async move {
            let mem = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
                 VALUES ($1, $2, $3, 'team', 'raw', 'fact', $4)",
            )
            .bind(mem)
            .bind(org)
            .bind(team)
            .bind(format!("raw candidate ({tag})"))
            .execute(&admin)
            .await
            .expect("raw memory");
            let promo = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status,
                                         policy_decision, policy_rule)
                 VALUES ($1, $2, $3, 'raw', 'candidate', 'needs_review', 'test.bulk')",
            )
            .bind(promo)
            .bind(org)
            .bind(mem)
            .execute(&admin)
            .await
            .expect("promotion");
            (promo, mem)
        }
    };

    // Two the pay lead maintains, one they cannot even see (another team).
    let (pay_a, mem_pay_a) = seed_promo(team_pay, "pay-a").await;
    let (pay_b, mem_pay_b) = seed_promo(team_pay, "pay-b").await;
    let (data_c, mem_data_c) = seed_promo(team_data, "data-c").await;

    let tok = |user: &str, teams: Vec<Uuid>| serde_json::json!({"org": org, "user": stable_uuid(user), "teams": teams});
    std::env::set_var(
        "BRAINIAC_TOKENS",
        serde_json::json!({
            // Maintainer of payments only.
            "tok_pay_lead": tok("user-pay-lead", vec![team_pay]),
            // Member of payments, NOT a maintainer.
            "tok_pay_dev": tok("user-pay-dev1", vec![team_pay]),
        })
        .to_string(),
    );

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

    let status_of = |mem: Uuid| {
        let admin = admin.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status::text FROM memories WHERE id = $1")
                .bind(mem)
                .fetch_one(&admin)
                .await
                .expect("status")
        }
    };

    // ── mixed batch as the payments maintainer ───────────────────────────
    // pay_a + pay_b are theirs to approve; data_c is not theirs to see.
    let r = http
        .post(format!("{base}/v1/reviews/promotions/bulk"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "action": "approve", "ids": [pay_a, pay_b, data_c] }))
        .send()
        .await
        .expect("bulk");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::OK,
        "a mixed batch still returns 200 — the verdict is per row"
    );
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["decided"], 2, "both payments items approved");
    assert_eq!(body["failed"], 1, "the other team's item refused");

    // Per-row: index the verdicts by promotion id.
    let mut verdict = std::collections::HashMap::new();
    for row in body["results"].as_array().expect("results") {
        verdict.insert(
            row["promotion_id"].as_str().expect("id").to_string(),
            (
                row["ok"].as_bool().expect("ok"),
                row["status"].as_i64().expect("status"),
            ),
        );
    }
    assert_eq!(verdict[&pay_a.to_string()], (true, 200));
    assert_eq!(verdict[&pay_b.to_string()], (true, 200));
    // Not visible under RLS ⇒ 404, not 403 — the no-oracle stance, same as the
    // single-item path.
    assert_eq!(verdict[&data_c.to_string()], (false, 404));

    // The gate HELD: the two payments memories moved, the data memory did not.
    assert_eq!(status_of(mem_pay_a).await, "candidate");
    assert_eq!(status_of(mem_pay_b).await, "candidate");
    assert_eq!(
        status_of(mem_data_c).await,
        "raw",
        "a memory the caller could not act on must be untouched by the batch"
    );

    // ── a non-maintainer of a VISIBLE item is refused (the 403 path) ─────
    let (pay_d, mem_pay_d) = seed_promo(team_pay, "pay-d").await;
    let r = http
        .post(format!("{base}/v1/reviews/promotions/bulk"))
        .bearer_auth("tok_pay_dev") // member, not maintainer
        .json(&serde_json::json!({ "action": "approve", "ids": [pay_d] }))
        .send()
        .await
        .expect("bulk dev");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["decided"], 0);
    assert_eq!(body["failed"], 1);
    assert_eq!(
        body["results"][0]["status"], 403,
        "member is not a maintainer"
    );
    assert_eq!(
        status_of(mem_pay_d).await,
        "raw",
        "a non-maintainer's bulk approval must not move the memory"
    );

    // ── the batch cap and malformed batches are rejected outright ────────
    let big: Vec<Uuid> = (0..201).map(|_| Uuid::new_v4()).collect();
    let r = http
        .post(format!("{base}/v1/reviews/promotions/bulk"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "action": "approve", "ids": big }))
        .send()
        .await
        .expect("bulk big");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "over the 200 cap is refused as a batch, not truncated"
    );

    let r = http
        .post(format!("{base}/v1/reviews/promotions/bulk"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "action": "sideways", "ids": [pay_a] }))
        .send()
        .await
        .expect("bulk bad action");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
}
