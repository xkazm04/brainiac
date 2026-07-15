//! Contradiction-resolve concurrency (DATABASE_URL-gated).
//!
//! Two properties the queue's ledger depends on, neither of which held before:
//!
//! 1. Two maintainers resolving the SAME dispute race. Without a row lock both
//!    passed the `status = 'open'` read and both wrote — last-writer-wins on
//!    status/resolved_by/resolved_at, with the first writer's supersession
//!    side-effects already applied. Exactly one must land.
//! 2. `apply_supersession` is idempotent and reports `false` when it applied
//!    nothing. Discarding that bool stamped `resolved_supersede` on a dispute
//!    where no supersession happened — the ledger asserting a corpus change
//!    that is not in the corpus.
//!
//! Follows the TRUNCATE-and-seed convention of every other `*_pg` test in the
//! workspace (they are not mutually isolated — the suite is written to run
//! against a dedicated database).

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_store::Store;
use uuid::Uuid;

#[tokio::test]
async fn contradiction_resolve_is_serialized_and_honest() {
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

    // Three canonical payments memories of our own, so the race is not
    // entangled with whatever the gold fixtures assert about theirs.
    let (mem_a, mem_b, mem_c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, content) in [
        (mem_a, "settlement recon runs at 07:00 UTC"),
        (mem_b, "settlement recon runs at 09:00 UTC"),
        (mem_c, "settlement recon runs hourly"),
    ] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'canonical', 'fact', $4)",
        )
        .bind(id)
        .bind(org)
        .bind(team_pay)
        .bind(content)
        .execute(&admin)
        .await
        .expect("memory");
    }

    let open_contradiction = |id: Uuid, a: Uuid, b: Uuid| {
        let admin = admin.clone();
        async move {
            sqlx::query(
                "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
                 VALUES ($1, $2, $3, $4, 'test', 'open', 'detector suggests supersede')",
            )
            .bind(id)
            .bind(org)
            .bind(a)
            .bind(b)
            .execute(&admin)
            .await
            .expect("contradiction");
        }
    };

    let raced = Uuid::new_v4();
    let stale = Uuid::new_v4();
    let converged = Uuid::new_v4();
    open_contradiction(raced, mem_a, mem_b).await;
    open_contradiction(stale, mem_a, mem_c).await;
    open_contradiction(converged, mem_a, mem_b).await;

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

    // ── 0. the queue reports a backlog, not a page length ────────────────
    //
    // Three open contradictions, asked for one at a time: `total` must stay 3
    // while the array is 1. A client that renders `contradictions.length` as the
    // backlog reports "1 open dispute" the moment the queue is paged.
    let page: serde_json::Value = http
        .get(format!(
            "{base}/v1/reviews/contradictions?status=open&limit=1"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("page 1")
        .json()
        .await
        .expect("json");
    assert_eq!(
        page["contradictions"].as_array().expect("array").len(),
        1,
        "limit must bound the window"
    );
    assert_eq!(page["total"], 3, "total must count the filtered backlog");

    // `total` follows the filters (not the histogram, which counts everything).
    let filtered: serde_json::Value = http
        .get(format!(
            "{base}/v1/reviews/contradictions?status=open&detected_by=nobody"
        ))
        .bearer_auth("tok_pay_lead")
        .send()
        .await
        .expect("filtered")
        .json()
        .await
        .expect("json");
    assert_eq!(filtered["total"], 0, "total must respect detected_by");

    // ── 1. two maintainers race the same dispute ─────────────────────────
    //
    // Both fire concurrently at the same open contradiction, disagreeing about
    // the outcome: one supersedes, one dismisses. Before the row lock both
    // returned 200 and the dispute ended up dismissed WITH mem_a superseded —
    // the ledger and the corpus telling different stories.
    let supersede = http
        .post(format!("{base}/v1/reviews/contradictions/{raced}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": mem_b,
            "note": "incident review confirmed 09:00"
        }))
        .send();
    let dismiss = http
        .post(format!("{base}/v1/reviews/contradictions/{raced}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "resolution": "dismiss", "note": "not a real conflict" }))
        .send();
    let (r1, r2) = tokio::join!(supersede, dismiss);
    let codes = [
        r1.expect("resolve 1").status(),
        r2.expect("resolve 2").status(),
    ];

    let winners = codes.iter().filter(|c| c.is_success()).count();
    assert_eq!(
        winners, 1,
        "exactly one concurrent resolve must land, got {codes:?}"
    );
    for c in codes.iter().filter(|c| !c.is_success()) {
        assert!(
            *c == reqwest::StatusCode::NOT_FOUND || *c == reqwest::StatusCode::CONFLICT,
            "the losing racer must be told it lost (404/409), got {c}"
        );
    }

    // The ledger records exactly one resolver, and a terminal status.
    let (status, resolved_by, note): (String, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT status, resolved_by, resolution_note FROM contradictions WHERE id=$1",
    )
    .bind(raced)
    .fetch_one(&admin)
    .await
    .expect("raced row");
    assert_ne!(status, "open", "the winner must have closed the dispute");
    assert_eq!(
        resolved_by,
        Some(stable_uuid("user-pay-lead")),
        "the resolver must be stamped"
    );
    assert!(note.is_some(), "the winner's note must be persisted");

    // …and the corpus agrees with the ledger. The whole bug was these two
    // drifting apart: a supersede's side-effects landing under a dismissal.
    let (loser_status, superseded_by): (String, Option<Uuid>) =
        sqlx::query_as("SELECT status::text, superseded_by FROM memories WHERE id=$1")
            .bind(mem_a)
            .fetch_one(&admin)
            .await
            .expect("mem_a");
    if status == "resolved_supersede" {
        assert_eq!(loser_status, "deprecated");
        assert_eq!(superseded_by, Some(mem_b));
    } else {
        assert_eq!(
            superseded_by, None,
            "a dispute closed as `{status}` must NOT have superseded anything"
        );
        assert_eq!(loser_status, "canonical");
    }

    // Exactly one audit row for the supersession, never two.
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM promotions WHERE policy_rule = 'contradiction_supersede'",
    )
    .fetch_one(&admin)
    .await
    .expect("audit count");
    let expected = i64::from(status == "resolved_supersede");
    assert_eq!(audit, expected, "one supersession ⇒ one audit row");

    // A second, sequential attempt on the settled dispute is refused.
    let r = http
        .post(format!("{base}/v1/reviews/contradictions/{raced}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "resolution": "coexist" }))
        .send()
        .await
        .expect("re-resolve");
    assert!(
        r.status() == reqwest::StatusCode::NOT_FOUND || r.status() == reqwest::StatusCode::CONFLICT,
        "a settled dispute must not be re-resolvable, got {}",
        r.status()
    );

    // ── 2. a supersession that applies nothing is not recorded as one ─────
    //
    // Force mem_a into a superseded state, then try to supersede it again via
    // the `stale` dispute. apply_supersession is idempotent → applies nothing →
    // the handler must refuse rather than stamp `resolved_supersede` over a
    // corpus change that never happened.
    sqlx::query(
        "UPDATE memories SET superseded_by = $2, status = 'deprecated'::memory_status,
                             valid_to = now()
         WHERE id = $1",
    )
    .bind(mem_a)
    .bind(mem_b)
    .execute(&admin)
    .await
    .expect("pre-supersede mem_a");

    let r = http
        .post(format!("{base}/v1/reviews/contradictions/{stale}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": mem_c,
            "note": "hourly wins"
        }))
        .send()
        .await
        .expect("stale supersede");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CONFLICT,
        "superseding an already-superseded memory must 409, not report success"
    );

    // The refusal rolled back: the dispute is untouched and still answerable.
    let (stale_status, stale_by): (String, Option<Uuid>) =
        sqlx::query_as("SELECT status, resolved_by FROM contradictions WHERE id=$1")
            .bind(stale)
            .fetch_one(&admin)
            .await
            .expect("stale row");
    assert_eq!(stale_status, "open", "a refused resolve must not close it");
    assert_eq!(
        stale_by, None,
        "a refused resolve must not stamp a resolver"
    );

    // mem_a's supersession target is untouched — no second supersession slipped in.
    let after: Option<Uuid> = sqlx::query_scalar("SELECT superseded_by FROM memories WHERE id=$1")
        .bind(mem_a)
        .fetch_one(&admin)
        .await
        .expect("mem_a after");
    assert_eq!(after, Some(mem_b));

    // …but the same dispute IS still resolvable a truthful way.
    let r = http
        .post(format!("{base}/v1/reviews/contradictions/{stale}/resolve"))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({ "resolution": "coexist", "note": "both windows are real" }))
        .send()
        .await
        .expect("stale coexist");
    assert!(
        r.status().is_success(),
        "a refused supersede must leave the dispute answerable, got {}",
        r.status()
    );

    // ── 3. a supersede whose outcome ALREADY holds is a success, not a 409 ─
    //
    // The `false` from apply_supersession conflates "nothing to do because the
    // corpus already says exactly this" with "nothing done because the request
    // contradicts the corpus". Only the latter is a conflict. mem_a already
    // points at mem_b, so asking for exactly that must close the dispute —
    // otherwise a dispute in this state is un-resolvable by the one verdict
    // that actually fits it, and sits open forever.
    //
    // This is also the state the gold fixtures seed (mem-pay-0063 ships
    // superseded_by mem-pay-0064), which is why console_pg's supersede asserts
    // held while the handler applied nothing.
    let r = http
        .post(format!(
            "{base}/v1/reviews/contradictions/{converged}/resolve"
        ))
        .bearer_auth("tok_pay_lead")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": mem_b,
            "note": "already superseded — recording the verdict"
        }))
        .send()
        .await
        .expect("converged supersede");
    assert!(
        r.status().is_success(),
        "superseding toward the winner it ALREADY points at must succeed, got {}",
        r.status()
    );
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["status"], "resolved_supersede");

    // The recorded status is true of the corpus — which is the whole bar.
    let (loser_status, superseded_by): (String, Option<Uuid>) =
        sqlx::query_as("SELECT status::text, superseded_by FROM memories WHERE id=$1")
            .bind(mem_a)
            .fetch_one(&admin)
            .await
            .expect("mem_a converged");
    assert_eq!(loser_status, "deprecated");
    assert_eq!(superseded_by, Some(mem_b));
}
