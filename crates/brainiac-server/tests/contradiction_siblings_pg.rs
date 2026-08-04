//! Sibling disputes after a supersession (DATABASE_URL-gated).
//!
//! A memory can be party to two open contradictions. Resolving the first
//! deprecates it — and the second stayed open, still pointing at it. That is
//! three failures at once:
//!
//! 1. A maintainer is later shown a dispute about a memory that is already
//!    dead, reads it, and decides it. The governance loop's entire value is that
//!    a human decision means something.
//! 2. The dispute is *unanswerable*: `supersede` on the dead side is refused by
//!    `apply_supersession` (already superseded), and the other direction would
//!    retire a live memory in favour of a retired one.
//! 3. While it sits there open it WITHHOLDS both of its sides from serving — so
//!    a dead row was suppressing a live memory indefinitely.
//!
//! Resolving now settles those siblings as `moot` (with a note saying what
//! happened and that NOTHING was decided) and carries their live question
//! forward as a fresh OPEN dispute against the winner. That is discovery, not
//! adjudication: no memory changes standing, and a human still decides.
//!
//! Follows the TRUNCATE-and-seed convention of every other `*_pg` test in the
//! workspace (they are not mutually isolated — run against a dedicated database).

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_store::Store;
use uuid::Uuid;

struct World {
    admin: sqlx::PgPool,
    base: String,
    http: reqwest::Client,
    /// The memory both disputes argue about, and the one the resolution retires.
    disputed: Uuid,
    /// The winner of the resolved dispute.
    winner: Uuid,
    /// The other side of the SIBLING dispute — never decided by any of this.
    bystander: Uuid,
    /// (X vs winner), the dispute that gets resolved.
    resolved: Uuid,
    /// (X vs bystander), the sibling that used to be stranded.
    sibling: Uuid,
}

/// One org, one team, a maintainer and an outsider, three canonical memories,
/// and two open disputes that share a side.
async fn world(url: &str) -> World {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE embedding_versions, memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive,
                  knowledge_health_snapshots
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let org = Uuid::new_v4();
    let team = Uuid::new_v4();
    let maintainer = Uuid::new_v4();
    let outsider = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name) VALUES ($1,'meridian')")
        .bind(org)
        .execute(&admin)
        .await
        .expect("org");
    sqlx::query("INSERT INTO teams (id, org_id, name) VALUES ($1,$2,'payments')")
        .bind(team)
        .bind(org)
        .execute(&admin)
        .await
        .expect("team");
    for (id, email) in [
        (maintainer, "lead@meridian.test"),
        (outsider, "visitor@meridian.test"),
    ] {
        sqlx::query("INSERT INTO users (id, org_id, email) VALUES ($1,$2,$3)")
            .bind(id)
            .bind(org)
            .bind(email)
            .execute(&admin)
            .await
            .expect("user");
    }
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1,$2,'maintainer')")
        .bind(team)
        .bind(maintainer)
        .execute(&admin)
        .await
        .expect("maintainer");
    // Deliberately a MEMBER, not a maintainer: the gate must still be a gate.
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1,$2,'member')")
        .bind(team)
        .bind(outsider)
        .execute(&admin)
        .await
        .expect("member");

    let (disputed, winner, bystander) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, content) in [
        (disputed, "settlement recon runs at 07:00 UTC"),
        (winner, "settlement recon runs at 09:00 UTC"),
        (bystander, "settlement recon runs hourly"),
    ] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1,$2,$3,'team','canonical','fact',$4)",
        )
        .bind(id)
        .bind(org)
        .bind(team)
        .bind(content)
        .execute(&admin)
        .await
        .expect("memory");
    }

    let (resolved, sibling) = (Uuid::new_v4(), Uuid::new_v4());
    for (id, a, b) in [(resolved, disputed, winner), (sibling, disputed, bystander)] {
        sqlx::query(
            "INSERT INTO contradictions
                (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
             VALUES ($1,$2,$3,$4,'test','open','detector suggests supersede')",
        )
        .bind(id)
        .bind(org)
        .bind(a)
        .bind(b)
        .execute(&admin)
        .await
        .expect("contradiction");
    }

    let tok = |user: Uuid| serde_json::json!({"org": org, "user": user, "teams": [team]});
    std::env::set_var(
        "BRAINIAC_TOKENS",
        serde_json::json!({
            "tok_maintainer": tok(maintainer),
            "tok_member": tok(outsider),
        })
        .to_string(),
    );

    let store = Store::connect(url).await.expect("connect");
    let embedder = std::sync::Arc::new(DeterministicEmbedder::default());
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

    World {
        admin,
        base: format!("http://{addr}"),
        http: reqwest::Client::new(),
        disputed,
        winner,
        bystander,
        resolved,
        sibling,
    }
}

async fn contradiction_row(w: &World, id: Uuid) -> (String, Option<Uuid>, Option<String>) {
    sqlx::query_as("SELECT status, resolved_by, resolution_note FROM contradictions WHERE id=$1")
        .bind(id)
        .fetch_one(&w.admin)
        .await
        .expect("contradiction row")
}

async fn memory_standing(w: &World, id: Uuid) -> (String, Option<Uuid>) {
    sqlx::query_as("SELECT status::text, superseded_by FROM memories WHERE id=$1")
        .bind(id)
        .fetch_one(&w.admin)
        .await
        .expect("memory row")
}

/// The whole arc: the sibling is settled with a reason, its live question is
/// re-posed against the winner, and no memory the maintainer did not decide
/// about changes standing.
#[tokio::test]
async fn resolving_a_dispute_settles_its_siblings_instead_of_stranding_them() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let w = world(&url).await;

    let resp = w
        .http
        .post(format!(
            "{}/v1/reviews/contradictions/{}/resolve",
            w.base, w.resolved
        ))
        .bearer_auth("tok_maintainer")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": w.winner,
            "note": "incident review confirmed 09:00"
        }))
        .send()
        .await
        .expect("resolve");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["status"], "resolved_supersede");

    // ── 1. the sibling is no longer presented as an open decision ────────
    let (status, resolved_by, note) = contradiction_row(&w, w.sibling).await;
    assert_eq!(
        status, "moot",
        "the sibling dispute must be settled, not left open"
    );
    assert_eq!(
        body["siblings_settled"],
        serde_json::json!([w.sibling]),
        "the response must report exactly what it settled"
    );

    // …and it is gone from the queue a reviewer actually reads.
    let open: serde_json::Value = w
        .http
        .get(format!("{}/v1/reviews/contradictions?status=open", w.base))
        .bearer_auth("tok_maintainer")
        .send()
        .await
        .expect("queue")
        .json()
        .await
        .expect("json");
    let open_ids: Vec<String> = open["contradictions"]
        .as_array()
        .expect("array")
        .iter()
        .map(|c| c["id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        !open_ids.contains(&w.sibling.to_string()),
        "a dispute about an already-superseded memory must not be offered for decision: {open_ids:?}"
    );

    // ── 2. the audit says WHY it closed — it inherits a reason ───────────
    let note = note.expect("a settled sibling must carry a note");
    assert!(
        note.contains(&w.disputed.to_string())
            && note.contains(&w.winner.to_string())
            && note.contains(&w.resolved.to_string()),
        "the note must name the memory that died, what superseded it, and the \
         resolution that did it: {note}"
    );
    assert!(
        note.contains("NOTHING was decided"),
        "the note must say plainly that this dispute was never judged: {note}"
    );
    assert!(
        note.contains("detector suggests supersede"),
        "the original detector note must survive, not be overwritten: {note}"
    );
    assert!(
        resolved_by.is_some(),
        "the actor whose decision made it moot must be stamped"
    );

    // …and it is filterable, so `moot` rows are auditable rather than hidden.
    let mooted: serde_json::Value = w
        .http
        .get(format!("{}/v1/reviews/contradictions?status=moot", w.base))
        .bearer_auth("tok_maintainer")
        .send()
        .await
        .expect("moot queue")
        .json()
        .await
        .expect("json");
    assert_eq!(mooted["total"], 1, "moot must be a queryable status");

    // ── 3. the live question is carried forward, not lost ────────────────
    let carried = body["siblings_carried_forward"]
        .as_array()
        .expect("array")
        .clone();
    assert_eq!(carried.len(), 1, "one settled sibling ⇒ one live question");
    let carried_id: Uuid = carried[0].as_str().expect("id").parse().expect("uuid");
    let (c_status, c_a, c_b, c_detected): (String, Uuid, Uuid, String) = sqlx::query_as(
        "SELECT status, memory_a, memory_b, detected_by FROM contradictions WHERE id=$1",
    )
    .bind(carried_id)
    .fetch_one(&w.admin)
    .await
    .expect("carried row");
    assert_eq!(c_status, "open", "the carried question awaits a human");
    assert_eq!((c_a, c_b), (w.winner, w.bystander));
    assert_eq!(
        c_detected, "sibling_carry_forward",
        "an inherited question must never masquerade as a detector finding"
    );

    // ── 4. nothing was auto-decided ──────────────────────────────────────
    //
    // Exactly one memory changed standing: the one the maintainer named. The
    // bystander — whose dispute was closed WITHOUT being judged — is untouched,
    // and so is the winner.
    assert_eq!(
        memory_standing(&w, w.disputed).await,
        ("deprecated".into(), Some(w.winner))
    );
    assert_eq!(
        memory_standing(&w, w.bystander).await,
        ("canonical".into(), None)
    );
    assert_eq!(
        memory_standing(&w, w.winner).await,
        ("canonical".into(), None)
    );

    // One supersession ⇒ one audit row. Settling a sibling is not a second one.
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM promotions WHERE policy_rule = 'contradiction_supersede'",
    )
    .fetch_one(&w.admin)
    .await
    .expect("audit");
    assert_eq!(audit, 1, "settling siblings must not deprecate anything");

    // ── 5. no runaway cascade ────────────────────────────────────────────
    //
    // Resolving the carried-forward dispute settles what it names and stops.
    // (It also proves a carried row is an ordinary, resolvable dispute.)
    let resp = w
        .http
        .post(format!(
            "{}/v1/reviews/contradictions/{carried_id}/resolve",
            w.base
        ))
        .bearer_auth("tok_maintainer")
        .json(&serde_json::json!({ "resolution": "coexist" }))
        .send()
        .await
        .expect("resolve carried");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let total_open: i64 =
        sqlx::query_scalar("SELECT count(*) FROM contradictions WHERE status = 'open'")
            .fetch_one(&w.admin)
            .await
            .expect("open count");
    assert_eq!(total_open, 0, "the loop terminates; it does not breed rows");
}

/// Requirement: settling siblings is not a back door around the maintainer
/// gate. A caller who may not supersede must change NOTHING — not the corpus,
/// not the dispute, and not its siblings.
#[tokio::test]
async fn a_non_maintainer_settles_nothing() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let w = world(&url).await;

    let resp = w
        .http
        .post(format!(
            "{}/v1/reviews/contradictions/{}/resolve",
            w.base, w.resolved
        ))
        .bearer_auth("tok_member")
        .json(&serde_json::json!({
            "resolution": "supersede",
            "winner_memory_id": w.winner,
        }))
        .send()
        .await
        .expect("resolve");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    assert_eq!(contradiction_row(&w, w.resolved).await.0, "open");
    assert_eq!(
        contradiction_row(&w, w.sibling).await.0,
        "open",
        "a refused resolve must not settle a sibling"
    );
    assert_eq!(
        memory_standing(&w, w.disputed).await,
        ("canonical".into(), None)
    );
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM contradictions")
        .fetch_one(&w.admin)
        .await
        .expect("count");
    assert_eq!(
        total, 2,
        "a refused resolve must not carry anything forward"
    );
}
