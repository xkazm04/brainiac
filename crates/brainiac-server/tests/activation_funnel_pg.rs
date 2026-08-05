//! The launch funnel's third step, end to end (DATABASE_URL-gated).
//!
//! WHAT THIS IS PROTECTING
//! -----------------------
//! `activation` is the number the whole launch funnel exists to produce, and the
//! cloud-signup cohort can only produce it HERE: those users get a project and a
//! device key from `/v1/provision` and never open the console again, so the
//! console's own `trackActivation` (console/src/lib/api.ts) is structurally
//! unable to fire for them. Two failure modes would each make the number a lie,
//! and neither is visible by reading the call site:
//!
//!   - firing on EVERY memory write ⇒ activation counts ingest volume, and a
//!     single busy org looks like a thousand activated ones;
//!   - firing on an `Idempotency-Key` REPLAY ⇒ a retried first write counts
//!     twice for the same org.
//!
//! So this test does not inspect a boolean. It stands up a fake Plausible, runs
//! the real router against a real database, and asserts what actually went over
//! the wire: four writes by one fresh org (first, second, keyed, keyed-replayed)
//! produce exactly ONE event, a second org's first write produces exactly one
//! more, and its follow-up produces none. It also asserts the beacon carries no
//! identifier and no client IP — a published privacy claim (console/app/privacy).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use brainiac_core::embed::DeterministicEmbedder;
use brainiac_store::Store;
use serde_json::Value;
use uuid::Uuid;

/// Everything the fake Plausible saw: one entry per beacon.
type Received = Arc<Mutex<Vec<(HeaderMap, Value)>>>;

async fn collect(
    State(seen): State<Received>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> &'static str {
    seen.lock().expect("lock").push((headers, body));
    "ok"
}

/// Wait for the background beacon thread to deliver `want` events, then keep
/// waiting a beat longer so a SPURIOUS extra event would also have landed and be
/// caught by the count assertion — a check that returns the instant it sees the
/// event it wants cannot detect one it does not.
async fn settle(seen: &Received, want: usize, limit: Duration) -> usize {
    let start = Instant::now();
    while start.elapsed() < limit {
        if seen.lock().expect("lock").len() >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let n = seen.lock().expect("lock").len();
    n
}

#[tokio::test]
async fn activation_fires_once_for_an_orgs_first_memory_and_never_again() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE embedding_versions, memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    // ── the fake Plausible, up BEFORE any analytics call ──────────────────
    // src/analytics.rs resolves its configuration once into a OnceLock, so the
    // env has to be right before the first `enabled()` in the process. That is
    // also why this is the only test in this binary.
    let seen: Received = Arc::new(Mutex::new(Vec::new()));
    let plausible = Router::new()
        .route("/api/event", post(collect))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind plausible");
    let plausible_addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, plausible).await.expect("serve plausible");
    });
    std::env::set_var("PLAUSIBLE_DOMAIN", "brainiac.test");
    std::env::set_var("PLAUSIBLE_HOST", format!("http://{plausible_addr}"));

    // ── the real stack ────────────────────────────────────────────────────
    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = std::sync::Arc::new(DeterministicEmbedder::default());
    brainiac_eval::seed::seed_gold(&store, &fx, embedder.as_ref())
        .await
        .expect("seed");

    // Two brand-new signups, provisioned through the SAME call /v1/provision
    // uses — org + personal team + user, exactly as a cloud sign-up produces
    // them. This is the cohort the console structurally cannot observe, and
    // going through the real provisioning path (rather than inventing uuids)
    // means "this org has no sources yet" is true for the same reason it is
    // true in production.
    let mut conn = admin.acquire().await.expect("admin conn");
    let one = brainiac_store::identities::provision(
        &mut conn,
        "google",
        &format!("subject-{}", Uuid::new_v4()),
        "first@example.test",
        "first-signup",
    )
    .await
    .expect("provision one");
    let two = brainiac_store::identities::provision(
        &mut conn,
        "google",
        &format!("subject-{}", Uuid::new_v4()),
        "second@example.test",
        "second-signup",
    )
    .await
    .expect("provision two");
    drop(conn);
    let fresh_org = one.org_id;
    let tokens = serde_json::json!({
        "tok_signup": { "org": one.org_id, "user": one.user_id, "teams": [one.team_id] },
        // A second signup, to prove "first" is scoped to the org rather than to
        // the process or the deployment.
        "tok_signup2": { "org": two.org_id, "user": two.user_id, "teams": [two.team_id] },
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let app = brainiac_server::http::router(store, embedder, None)
        .await
        .expect("router");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind api");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve api");
    });
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();

    let add = |token: &'static str, content: &'static str, key: Option<&'static str>| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let mut req = http
                .post(format!("{base}/v1/memories"))
                .bearer_auth(token)
                .json(&serde_json::json!({ "content": content }));
            if let Some(k) = key {
                req = req.header("Idempotency-Key", k);
            }
            let r = req.send().await.expect("memory_add");
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            assert_eq!(
                status,
                reqwest::StatusCode::ACCEPTED,
                "memory_add must be accepted: {body}"
            );
        }
    };

    // 1. The org's first memory ever — the one activation.
    add("tok_signup", "the deploy runbook lives in docs/deploy", None).await;
    assert_eq!(
        settle(&seen, 1, Duration::from_secs(5)).await,
        1,
        "the first memory must produce exactly one activation event"
    );

    // 2-4. Everything after it: a second write, a keyed write, and the SAME key
    // replayed. None of them is an activation — the org already activated.
    add("tok_signup", "staging redeploys nightly at 02:00", None).await;
    add("tok_signup", "the on-call rota is in the wiki", Some("k-1")).await;
    add("tok_signup", "the on-call rota is in the wiki", Some("k-1")).await;
    settle(&seen, 0, Duration::from_millis(1)).await;
    assert_eq!(
        seen.lock().expect("lock").len(),
        1,
        "activation is once per org, not once per write"
    );

    // 5-6. A SECOND org activating is a second activation (the funnel counts
    // orgs that got value, and this one just did) — but its own follow-up write
    // is not. "First" is scoped to the org, not to the process.
    add("tok_signup2", "we deploy on Tuesdays", None).await;
    assert_eq!(
        settle(&seen, 2, Duration::from_secs(5)).await,
        2,
        "a different org's first memory activates too"
    );
    add("tok_signup2", "and never on a Friday", None).await;
    settle(&seen, 0, Duration::from_millis(1)).await;
    let events = seen.lock().expect("lock");
    assert_eq!(events.len(), 2, "still exactly one per org: {events:?}");

    // ── the beacon's contents are a privacy claim, not an implementation ──
    let (headers, body) = &events[0];
    assert_eq!(body["name"], "activation");
    assert_eq!(body["domain"], "brainiac.test");
    assert_eq!(body["props"], serde_json::json!({ "via": "memory_add" }));
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["domain", "name", "props", "url"],
        "no org/project/user/memory id may ride along"
    );
    assert!(
        !headers.contains_key("x-forwarded-for"),
        "no end-user IP is forwarded to the analytics vendor"
    );
    assert!(
        headers.contains_key("user-agent"),
        "the Events API rejects a request with no user-agent"
    );
    let raw = body.to_string();
    assert!(
        !raw.contains(&fresh_org.to_string()),
        "the org id must never appear in the beacon"
    );
}
