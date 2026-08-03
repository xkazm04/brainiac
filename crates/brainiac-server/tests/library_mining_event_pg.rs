//! Event-driven library mining (DATABASE_URL-gated) — the freshness upgrade:
//! a settled contradiction mines its own candidate the moment
//! `POST /v1/reviews/contradictions/{id}/resolve` settles it
//! `resolved_supersede`, instead of waiting for the (disabled-by-default,
//! weekly) `library` sweep to rediscover it.
//!
//! Two properties this file proves, neither of which any other `*_pg` test
//! covers:
//!
//! 1. The candidate exists the moment the HTTP resolve returns — with the
//!    scheduled sweep (`library_sweep::mine_all` / the `sweep_schedules`
//!    cron) NEVER having run. If the event trigger silently no-ops, this is
//!    the only test that would notice.
//! 2. No duplicate candidate: the same winning memory settling a SECOND,
//!    independent contradiction reuses the dedup window
//!    (`standard_provenance` keyed on the memory) and produces nothing new —
//!    and neither does a weekly sweep run afterward as a backstop. The
//!    org-wide sweep and the event path share one dedup mechanism, not two.
//!
//! Follows the TRUNCATE-and-seed convention of every other `*_pg` test (not
//! mutually isolated — run against a dedicated database).

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

#[tokio::test]
async fn contradiction_settlement_mines_without_the_cron_and_never_duplicates() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
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

    // One winner that states a convention (kind=decision, so miner (c)
    // qualifies it), and two independent losers so the winner settles TWO
    // separate contradictions — the realistic duplicate scenario: the same
    // signal (the winner memory) named twice.
    let (mem_win, mem_lose1, mem_lose2) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, content) in [
        (mem_win, "Feature flags retire within two releases."),
        (
            mem_lose1,
            "Feature flags live forever (payments' old take).",
        ),
        (
            mem_lose2,
            "Feature flags live forever (a second team's old take).",
        ),
    ] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'canonical', 'decision', $4)",
        )
        .bind(id)
        .bind(org)
        .bind(team_pay)
        .bind(content)
        .execute(&admin)
        .await
        .expect("memory");
    }

    let (c1, c2) = (Uuid::new_v4(), Uuid::new_v4());
    for (cid, loser) in [(c1, mem_lose1), (c2, mem_lose2)] {
        sqlx::query(
            "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
             VALUES ($1, $2, $3, $4, 'test', 'open', 'detector suggests supersede')",
        )
        .bind(cid)
        .bind(org)
        .bind(mem_win)
        .bind(loser)
        .execute(&admin)
        .await
        .expect("contradiction");
    }

    let tok = |user: &str, teams: Vec<Uuid>| serde_json::json!({"org": org, "user": stable_uuid(user), "teams": teams});
    let tokens = serde_json::json!({
        "tok_pay_lead": tok("user-pay-lead", vec![team_pay]),
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

    // Sanity: nothing in the Library yet — no sweep has ever run, and no
    // contradiction has settled.
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM standards WHERE org_id = $1")
        .bind(org)
        .fetch_one(&admin)
        .await
        .expect("before count");
    assert_eq!(before, 0, "nothing mined before any settlement");

    // ── 1. settle the FIRST contradiction — the event fires inline, with the
    // weekly `library` sweep never having run at all ──────────────────────
    let r = http
        .post(format!("{base}/v1/reviews/contradictions/{c1}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": mem_win,
            "note": "flags rot; the org chose a retirement window"
        }))
        .send()
        .await
        .expect("resolve c1");
    assert!(r.status().is_success(), "resolve c1: {}", r.status());

    let candidates_for_winner = || {
        let admin = admin.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM standard_provenance sp
                 JOIN standards s ON s.id = sp.standard_id
                 WHERE sp.kind = 'memory' AND sp.ref_id = $1",
            )
            .bind(mem_win)
            .fetch_one(&admin)
            .await
            .expect("candidate count")
        }
    };

    let after_c1 = candidates_for_winner().await;
    assert_eq!(
        after_c1, 1,
        "settling c1 must mine exactly one candidate for the winner, with no sweep having run"
    );
    let (lifecycle, category, statement): (String, String, String) = sqlx::query_as(
        "SELECT s.lifecycle::text, s.category, s.statement
         FROM standard_provenance sp JOIN standards s ON s.id = sp.standard_id
         WHERE sp.kind = 'memory' AND sp.ref_id = $1",
    )
    .bind(mem_win)
    .fetch_one(&admin)
    .await
    .expect("candidate row");
    assert_eq!(
        lifecycle, "proposed",
        "mined candidates are never authorities"
    );
    assert_eq!(category, "convention");
    assert_eq!(statement, "Feature flags retire within two releases.");

    // ── 2. settle a SECOND, independent contradiction naming the SAME
    // winner — the dedup window (shared with the weekly sweep) must hold:
    // no second candidate ────────────────────────────────────────────────
    let r = http
        .post(format!("{base}/v1/reviews/contradictions/{c2}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": mem_win,
            "note": "same convention, a second team's old take"
        }))
        .send()
        .await
        .expect("resolve c2");
    assert!(r.status().is_success(), "resolve c2: {}", r.status());

    let after_c2 = candidates_for_winner().await;
    assert_eq!(
        after_c2, 1,
        "a second settlement of the SAME winner must not mint a second candidate"
    );

    // ── 3. the weekly sweep, run afterward as the backstop, must ALSO be a
    // no-op for this cluster — proving the event path and the sweep share
    // one dedup mechanism, not two parallel ones ─────────────────────────
    let window = brainiac_pipeline::library_sweep::dedup_window_days();
    let sweep_stats = brainiac_pipeline::library_sweep::mine_all(&admin, window)
        .await
        .expect("backstop sweep");
    assert_eq!(
        sweep_stats.from_contradiction, 0,
        "the backstop sweep must mine nothing new for an already-claimed signal: {sweep_stats:?}"
    );

    let after_sweep = candidates_for_winner().await;
    assert_eq!(
        after_sweep, 1,
        "the backstop sweep must not duplicate the event-mined candidate"
    );
}
