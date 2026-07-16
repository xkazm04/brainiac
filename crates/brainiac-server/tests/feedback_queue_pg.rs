//! The feedback triage queue over the wire (DATABASE_URL-gated).
//!
//! This endpoint asks a human to PERMANENTLY DEPRECATE an org memory. The bar
//! for that payload is therefore not "does it return rows" but "does it return
//! enough to decide with": who reported, whether they are anyone, what the
//! memory is, where it came from, and how sure the corpus was. Everything
//! asserted here already existed in the store and was simply not joined.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_store::Store;
use serde_json::Value;
use uuid::Uuid;

struct Fx {
    org: Uuid,
    team_pay: Uuid,
    alice: Uuid,
    carol: Uuid,
    bot: Uuid,
}

async fn user(admin: &sqlx::PgPool, org: Uuid, email: Option<&str>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, org_id, email) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(org)
        .bind(email)
        .execute(admin)
        .await
        .expect("user");
    id
}

/// One claim, filed `age_hours` ago so ordering and per-claim ages are real
/// rather than all-now.
async fn claim(
    admin: &sqlx::PgPool,
    org: Uuid,
    mem: Uuid,
    reporter: Uuid,
    verdict: &str,
    note: Option<&str>,
    age_hours: f64,
) {
    sqlx::query(
        "INSERT INTO memory_feedback (id, org_id, memory_id, user_id, verdict, note, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, now() - make_interval(secs => $7))",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .bind(mem)
    .bind(reporter)
    .bind(verdict)
    .bind(note)
    .bind(age_hours * 3600.0)
    .execute(admin)
    .await
    .expect("claim");
}

async fn setup(admin: &sqlx::PgPool) -> Fx {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name) VALUES ($1, 'triage-org')")
        .bind(org)
        .execute(admin)
        .await
        .expect("org");
    let team_pay = Uuid::new_v4();
    sqlx::query("INSERT INTO teams (id, org_id, name) VALUES ($1, $2, 'payments')")
        .bind(team_pay)
        .bind(org)
        .execute(admin)
        .await
        .expect("team");

    let alice = user(admin, org, Some("alice@example.com")).await;
    let carol = user(admin, org, Some("carol@example.com")).await;
    // An agent principal with no email on file — `users.email` is nullable, and
    // a payload that assumed otherwise would break on exactly this reporter.
    let bot = user(admin, org, None).await;
    for (u, role) in [(alice, "maintainer"), (carol, "member")] {
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(team_pay)
            .bind(u)
            .bind(role)
            .execute(admin)
            .await
            .expect("membership");
    }
    Fx {
        org,
        team_pay,
        alice,
        carol,
        bot,
    }
}

#[tokio::test]
async fn feedback_queue_carries_what_a_deprecation_needs() {
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
                  promotions, memory_feedback, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  queue.jobs, queue.archive, knowledge_health_snapshots
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let fx = setup(&admin).await;

    // A team memory an LLM extracted, with a title, a confidence and a TTL —
    // every field the bench needs and none of which it used to receive.
    let prov = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO provenance (id, org_id, actor_kind, actor_id, model_ref)
         VALUES ($1, $2, 'agent', 'extractor-7', 'claude-sonnet-4')",
    )
    .bind(prov)
    .bind(fx.org)
    .execute(&admin)
    .await
    .expect("provenance");
    let mem = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, title, content,
                               confidence, provenance_id, valid_from, valid_to)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', 'PSP secret rotation',
                 'The PSP webhook signing secret rotates every 90 days.',
                 0.62, $4, now(), now() + interval '41 days')",
    )
    .bind(mem)
    .bind(fx.org)
    .bind(fx.team_pay)
    .bind(prov)
    .execute(&admin)
    .await
    .expect("memory");

    // FIVE `wrong` claims — the exact shape the old payload could not explain.
    // Three are one un-teamed agent firing repeatedly; two are humans on the
    // owning team. Same tally, opposite decision.
    for i in 0..3 {
        claim(
            &admin,
            fx.org,
            mem,
            fx.bot,
            "wrong",
            None,
            30.0 + f64::from(i),
        )
        .await;
    }
    claim(
        &admin,
        fx.org,
        mem,
        fx.alice,
        "wrong",
        Some("rotation moved to 30 days after the Q2 incident"),
        200.0,
    )
    .await;
    claim(
        &admin,
        fx.org,
        mem,
        fx.carol,
        "outdated",
        Some("stale"),
        1.0,
    )
    .await;
    // A resolved claim must not resurface, and must not inflate any count.
    claim(
        &admin,
        fx.org,
        mem,
        fx.carol,
        "wrong",
        Some("old news"),
        900.0,
    )
    .await;
    sqlx::query(
        "UPDATE memory_feedback SET resolved_at = now(), resolution = 'dismissed'
         WHERE memory_id = $1 AND note = 'old news'",
    )
    .bind(mem)
    .execute(&admin)
    .await
    .expect("close one");

    // An org-wide memory with no team and no provenance — the null paths.
    let orgwide = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, NULL, 'org', 'canonical', 'decision', 'Refunds read from the primary.')",
    )
    .bind(orgwide)
    .bind(fx.org)
    .execute(&admin)
    .await
    .expect("org-wide memory");
    claim(&admin, fx.org, orgwide, fx.alice, "outdated", None, 5.0).await;

    let tokens = serde_json::json!({
        "tok_alice": {"org": fx.org, "user": fx.alice, "teams": [fx.team_pay]},
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let store = Store::connect(&url).await.expect("connect");
    let app = brainiac_server::http::router(
        store,
        std::sync::Arc::new(DeterministicEmbedder::default()),
        None,
    )
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

    let body: Value = http
        .get(format!("{base}/v1/reviews/feedback"))
        .bearer_auth("tok_alice")
        .send()
        .await
        .expect("queue")
        .json()
        .await
        .expect("json");

    assert_eq!(body["total"], 2, "two memories carry open claims");
    let flagged = body["flagged"].as_array().expect("flagged array");
    assert_eq!(flagged.len(), 2);

    // Server order: wrong DESC — the 4-wrong memory leads.
    let row = &flagged[0];
    assert_eq!(row["memory_id"], mem.to_string());
    assert_eq!(row["claims"]["wrong"], 4, "the resolved claim stays closed");
    assert_eq!(row["claims"]["outdated"], 1);

    // ── the decisive number ─────────────────────────────────────────────
    // Five open claims, THREE reporters. Without this, "5 claims" reads as
    // five people and the memory gets deprecated on one bot's opinion.
    assert_eq!(row["reporters"], 3);

    // ── the memory itself ───────────────────────────────────────────────
    assert_eq!(row["title"], "PSP secret rotation");
    assert_eq!(row["team"], "payments", "a NAME, not a UUID on screen");
    assert_eq!(row["team_id"], fx.team_pay.to_string());
    assert!(
        (row["confidence"].as_f64().expect("confidence") - 0.62).abs() < 1e-6,
        "confidence: {}",
        row["confidence"]
    );
    assert_eq!(row["provenance"]["actor_kind"], "agent");
    assert_eq!(row["provenance"]["actor_id"], "extractor-7");
    assert_eq!(row["provenance"]["model_ref"], "claude-sonnet-4");

    // ── attributed, dated claims ────────────────────────────────────────
    let reports = row["reports"].as_array().expect("reports");
    assert_eq!(
        reports.len(),
        5,
        "capped at 5, and there are exactly 5 open"
    );
    for r in reports {
        assert!(r["age_secs"].as_i64().expect("age_secs") >= 0);
        assert!(r["reporter_id"].is_string());
    }
    // Newest first.
    let ages: Vec<i64> = reports
        .iter()
        .map(|r| r["age_secs"].as_i64().expect("age"))
        .collect();
    assert!(
        ages.windows(2).all(|w| w[0] <= w[1]),
        "claims must arrive newest-first: {ages:?}"
    );
    // The newest is carol's, an hour old — and NOT a two-year-old note wearing
    // the same anonymous quotation marks.
    assert_eq!(reports[0]["note"], "stale");
    assert_eq!(reports[0]["reporter_email"], "carol@example.com");
    assert!(reports[0]["reporter_on_owning_team"]
        .as_bool()
        .expect("bool"));
    assert!(
        (3000..=4200).contains(&ages[0]),
        "an hour-old claim should read as ~3600s, got {}",
        ages[0]
    );

    // The bot: no email, not on the owning team, and three claims of the five.
    let bot_reports: Vec<&Value> = reports
        .iter()
        .filter(|r| r["reporter_id"] == fx.bot.to_string())
        .collect();
    assert_eq!(bot_reports.len(), 3, "one reporter, three claims");
    for r in &bot_reports {
        assert!(r["reporter_email"].is_null(), "no email on file is null");
        assert!(
            !r["reporter_on_owning_team"].as_bool().expect("bool"),
            "the bot is on no team — it must not read as an owner"
        );
        assert!(r["note"].is_null());
    }
    let alice_report = reports
        .iter()
        .find(|r| r["reporter_id"] == fx.alice.to_string())
        .expect("alice's claim");
    assert_eq!(alice_report["reporter_email"], "alice@example.com");
    assert!(alice_report["reporter_on_owning_team"]
        .as_bool()
        .expect("bool"));

    // ── the null paths, populated honestly ──────────────────────────────
    let ow = &flagged[1];
    assert_eq!(ow["memory_id"], orgwide.to_string());
    assert!(ow["team"].is_null(), "an org-wide memory has no team name");
    assert!(ow["team_id"].is_null());
    assert!(ow["title"].is_null());
    assert!(ow["confidence"].is_null());
    assert!(
        ow["provenance"].is_null(),
        "no provenance row = whole object null, not a half-built one"
    );
    assert_eq!(ow["reporters"], 1);
    assert!(
        !ow["reports"][0]["reporter_on_owning_team"]
            .as_bool()
            .expect("bool"),
        "a teamless memory has no owning team to be on"
    );
}
