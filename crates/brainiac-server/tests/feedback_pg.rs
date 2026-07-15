//! Dispute resolution over the wire (DATABASE_URL-gated): the maintainer gate
//! on destructive answers — including the org-wide (teamless) memories the gate
//! used to wave through — the coherence guards, and the no-op paths that used to
//! report 200 while nothing happened.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

/// A memory + one open `wrong` claim against it, inserted as the pipeline and a
/// reporting agent would. `team` = None makes it org-wide, the case the gate
/// skipped entirely.
async fn seed_disputed(
    admin: &sqlx::PgPool,
    org: Uuid,
    team: Option<Uuid>,
    reporter: Uuid,
    content: &str,
) -> Uuid {
    let mem = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content, valid_from, valid_to)
         VALUES ($1, $2, $3, $4::visibility, 'canonical', 'fact', $5, now(), now() + interval '30 days')",
    )
    .bind(mem)
    .bind(org)
    .bind(team)
    .bind(if team.is_some() { "team" } else { "org" })
    .bind(content)
    .execute(admin)
    .await
    .expect("memory");
    sqlx::query(
        "INSERT INTO memory_feedback (id, org_id, memory_id, user_id, verdict, note)
         VALUES ($1, $2, $3, $4, 'wrong', 'reporter says this is wrong')",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .bind(mem)
    .bind(reporter)
    .execute(admin)
    .await
    .expect("claim");
    mem
}

async fn status_of(admin: &sqlx::PgPool, mem: Uuid) -> String {
    use sqlx::Row;
    sqlx::query("SELECT status::text AS s FROM memories WHERE id = $1")
        .bind(mem)
        .fetch_one(admin)
        .await
        .expect("status")
        .get("s")
}

async fn open_claims(admin: &sqlx::PgPool, mem: Uuid) -> i64 {
    use sqlx::Row;
    sqlx::query(
        "SELECT count(*) AS n FROM memory_feedback
         WHERE memory_id = $1 AND resolved_at IS NULL",
    )
    .bind(mem)
    .fetch_one(admin)
    .await
    .expect("claims")
    .get("n")
}

#[tokio::test]
async fn dispute_resolution_gates_and_no_ops() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  memory_feedback, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  queue.jobs, queue.archive, knowledge_health_snapshots
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
    let dev = stable_uuid("user-pay-dev1"); // member of payments, maintainer of NOTHING

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

    let resolve = |mem: Uuid, token: &'static str, body: serde_json::Value| {
        let http = http.clone();
        let base = base.clone();
        async move {
            http.post(format!("{base}/v1/reviews/feedback/{mem}/resolve"))
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .expect("resolve")
        }
    };

    // ── SECURITY: an org-wide memory has no owning team, and the old gate
    // read that as "no check to run". A plain member holding `write` could
    // permanently deprecate org-level knowledge. ───────────────────────────
    let org_mem = seed_disputed(
        &admin,
        org,
        None,
        dev,
        "org-wide: deploys freeze on Fridays",
    )
    .await;
    let r = resolve(
        org_mem,
        "tok_pay_dev",
        serde_json::json!({"resolution": "deprecated"}),
    )
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::FORBIDDEN,
        "a non-maintainer must not deprecate an org-wide memory"
    );
    assert_eq!(
        status_of(&admin, org_mem).await,
        "canonical",
        "the refused call must not have touched the corpus"
    );
    assert_eq!(
        open_claims(&admin, org_mem).await,
        1,
        "the refused call must not have closed the claim"
    );

    // A maintainer of ANY team may answer an org-wide dispute (docs.rs's stance
    // for org-wide pages) — the gate is stricter, not impassable.
    let r = resolve(
        org_mem,
        "tok_pay_lead",
        serde_json::json!({"resolution": "deprecated"}),
    )
    .await;
    assert!(
        r.status().is_success(),
        "org maintainer must be able to answer: {}",
        r.status()
    );
    assert_eq!(status_of(&admin, org_mem).await, "deprecated");
    assert_eq!(open_claims(&admin, org_mem).await, 0);

    // ── The team gate still holds, and invisibility is a 404 (no oracle) ───
    let team_mem = seed_disputed(
        &admin,
        org,
        Some(team_pay),
        dev,
        "team: recon runs at 07:00",
    )
    .await;
    let r = resolve(
        team_mem,
        "tok_pay_dev",
        serde_json::json!({"resolution": "deprecated"}),
    )
    .await;
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN, "member refused");
    let r = resolve(
        team_mem,
        "tok_analyst",
        serde_json::json!({"resolution": "deprecated"}),
    )
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "an outsider gets 404, never 403 — no existence oracle"
    );

    // ── `reverified` reports the boundary it actually set ─────────────────
    let r = resolve(
        team_mem,
        "tok_pay_lead",
        serde_json::json!({"resolution": "reverified", "days": 90}),
    )
    .await;
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["claims_closed"], 1);
    assert!(
        body["valid_to"].is_string(),
        "a successful reverify must carry the new boundary, never null: {body}"
    );

    // ── No open claims ⇒ 409, not a cheerful 200 over a settled dispute ───
    let r = resolve(
        team_mem,
        "tok_pay_lead",
        serde_json::json!({"resolution": "deprecated"}),
    )
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CONFLICT,
        "answering an already-answered dispute must conflict, not report claims_closed: 0"
    );
    assert_eq!(
        status_of(&admin, team_mem).await,
        "canonical",
        "the 409 must not have deprecated the memory anyway"
    );

    // ── Coherence: `reverified` cannot extend a deprecated row ────────────
    // A deprecated memory carrying a fresh claim: extend_validity guards only on
    // superseded_by, so this used to push valid_to a year out on a retired row
    // and return 200.
    let dep_mem = seed_disputed(&admin, org, Some(team_pay), dev, "team: retired practice").await;
    sqlx::query("UPDATE memories SET status = 'deprecated'::memory_status WHERE id = $1")
        .bind(dep_mem)
        .execute(&admin)
        .await
        .expect("deprecate");
    let before: Option<chrono::DateTime<chrono::Utc>> = {
        use sqlx::Row;
        sqlx::query("SELECT valid_to FROM memories WHERE id = $1")
            .bind(dep_mem)
            .fetch_one(&admin)
            .await
            .expect("valid_to")
            .get("valid_to")
    };
    let r = resolve(
        dep_mem,
        "tok_pay_lead",
        serde_json::json!({"resolution": "reverified", "days": 365}),
    )
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CONFLICT,
        "re-verifying a deprecated memory is incoherent and must be refused"
    );
    let after: Option<chrono::DateTime<chrono::Utc>> = {
        use sqlx::Row;
        sqlx::query("SELECT valid_to FROM memories WHERE id = $1")
            .bind(dep_mem)
            .fetch_one(&admin)
            .await
            .expect("valid_to")
            .get("valid_to")
    };
    assert_eq!(
        before, after,
        "the refused reverify must not move the window"
    );
    assert_eq!(
        open_claims(&admin, dep_mem).await,
        1,
        "the refused reverify must leave the dispute open"
    );

    // `dismissed` still works on that row — the memory stands untouched.
    let r = resolve(
        dep_mem,
        "tok_pay_lead",
        serde_json::json!({"resolution": "dismissed"}),
    )
    .await;
    assert!(r.status().is_success(), "dismiss must remain available");
    assert_eq!(open_claims(&admin, dep_mem).await, 0);
}
