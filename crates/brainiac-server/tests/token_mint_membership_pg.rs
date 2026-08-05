//! `POST /v1/tokens` will not mint authority for a principal that is not a
//! member of the caller's org (DATABASE_URL-gated).
//!
//! The defect this pins: `create_token` took `body.user_id` on trust
//! (`unwrap_or(caller)`) with no membership check. The endpoint requires the
//! `admin` scope, so every direct caller of it could mint a working key that
//! ACTS AS ANY UUID — including identities that belong to nobody and can
//! therefore never be recognised, revoked by their owner, or audited to a
//! person. `console/src/lib/key-guard.ts` closed the console's route to this
//! endpoint; this pins the endpoint itself, which is what every non-console
//! caller reaches.
//!
//! Both directions matter: a fabricated `user_id` is refused, and a real member
//! still gets a token (the guard must not break the mint path). The third case
//! is the one a naive fix breaks — an omitted `user_id` on a fresh deploy,
//! where the break-glass principal has no `users` row at all and
//! `scripts/first-boot.sh` mints exactly that way.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::Principal;
use brainiac_store::Store;
use uuid::Uuid;

const ORG: &str = "00000000-0000-0000-0000-0000000000b1";
/// The break-glass principal. Deliberately NOT given a `users` row — that is
/// the state a fresh deploy is actually in.
const CALLER: &str = "00000000-0000-0000-0000-0000000000b2";
/// A real member of the org.
const MEMBER: &str = "00000000-0000-0000-0000-0000000000b3";
/// Belongs to nobody. Under the old behaviour this minted a working key.
const FABRICATED: &str = "00000000-0000-0000-0000-0000000000ff";
/// A real user, in a DIFFERENT org — membership is per-org, not global.
const OTHER_ORG: &str = "00000000-0000-0000-0000-0000000000c1";
const OTHER_ORG_USER: &str = "00000000-0000-0000-0000-0000000000c2";

#[tokio::test]
async fn mint_refuses_a_non_member_and_still_serves_a_real_one() {
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
                  api_tokens, team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let org_id = Uuid::parse_str(ORG).expect("org uuid");
    let caller_id = Uuid::parse_str(CALLER).expect("caller uuid");
    let member_id = Uuid::parse_str(MEMBER).expect("member uuid");
    let fabricated = Uuid::parse_str(FABRICATED).expect("fabricated uuid");
    let other_org_id = Uuid::parse_str(OTHER_ORG).expect("other org uuid");
    let other_org_user = Uuid::parse_str(OTHER_ORG_USER).expect("other org user uuid");

    let store = Store::connect(&url).await.expect("connect");
    {
        let bootstrap = Principal {
            org_id,
            user_id: caller_id,
            team_ids: vec![],
            project_id: None,
        };
        let mut tx = store.scoped_tx(&bootstrap).await.expect("tx");
        brainiac_store::orgs::upsert_org(&mut tx, org_id, "mint-guard-org")
            .await
            .expect("org");
        brainiac_store::orgs::upsert_user(&mut tx, member_id, org_id, "member@example.test")
            .await
            .expect("member");
        // A second org with its own user: proves the check is scoped to the
        // CALLER's org, not "exists in the users table anywhere".
        brainiac_store::orgs::upsert_org(&mut tx, other_org_id, "other-org")
            .await
            .expect("other org");
        brainiac_store::orgs::upsert_user(
            &mut tx,
            other_org_user,
            other_org_id,
            "stranger@example.test",
        )
        .await
        .expect("other org user");
        tx.commit().await.expect("commit");
    }

    let tokens = serde_json::json!({
        "tok_admin": { "org": org_id, "user": caller_id, "teams": [] },
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let embedder = std::sync::Arc::new(DeterministicEmbedder::default());
    let app = brainiac_server::http::router(store.clone(), embedder, None)
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

    let mint = |body: serde_json::Value| {
        let http = http.clone();
        let base = base.clone();
        async move {
            http.post(format!("{base}/v1/tokens"))
                .bearer_auth("tok_admin")
                .json(&body)
                .send()
                .await
                .expect("mint")
        }
    };

    // ── a fabricated user_id is refused ──────────────────────────────────
    let r = mint(serde_json::json!({
        "name": "ghost key",
        "scopes": ["read", "write"],
        "user_id": fabricated,
    }))
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "minting for a uuid that belongs to nobody must be refused, not honoured"
    );
    let err: serde_json::Value = r.json().await.expect("error envelope");
    assert_eq!(err["code"], "bad_request");
    assert!(
        err["error"]
            .as_str()
            .expect("message")
            .contains("not a member of this org"),
        "the message names the reason: {err}"
    );

    // A refused mint must leave NO key behind — a stored-but-rejected key
    // would be authority nobody knows exists.
    let keys: i64 = sqlx::query_scalar("SELECT count(*) FROM api_tokens WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&admin)
        .await
        .expect("count keys");
    assert_eq!(keys, 0, "a refused mint must store nothing");

    // ── a real user of ANOTHER org is refused too ────────────────────────
    let r = mint(serde_json::json!({
        "name": "cross-org key",
        "scopes": ["read"],
        "user_id": other_org_user,
    }))
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "membership is per-org: an existing user of another org is not a member here"
    );

    // ── a legitimate member still gets a token ───────────────────────────
    let r = mint(serde_json::json!({
        "name": "member key",
        "scopes": ["read", "write"],
        "user_id": member_id,
    }))
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CREATED,
        "the guard must not break minting for a real member"
    );
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["user_id"], serde_json::json!(member_id));
    let secret = body["token"].as_str().expect("plaintext secret");
    assert!(secret.starts_with("brk_"), "a managed key was minted");

    // …and it actually authenticates as that principal.
    let r = http
        .get(format!("{base}/v1/tokens"))
        .bearer_auth(secret)
        .send()
        .await
        .expect("list with the new key");
    assert!(
        r.status().is_success() || r.status() == reqwest::StatusCode::FORBIDDEN,
        "the minted key resolves (403 only if it lacks the admin scope, which is \
         still proof it authenticated); got {}",
        r.status()
    );

    // ── an OMITTED user_id still mints for the caller ────────────────────
    // The caller has no `users` row: this is a fresh deploy's break-glass
    // principal, and scripts/first-boot.sh mints exactly this way. A membership
    // check applied to the default would brick first boot.
    let r = mint(serde_json::json!({
        "name": "first-boot key",
        "scopes": ["admin"],
    }))
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CREATED,
        "an omitted user_id defaults to the caller and must not need a roster row"
    );
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["user_id"], serde_json::json!(caller_id));

    // Explicitly naming YOURSELF is the same authority, and must behave the
    // same as omitting it — otherwise the guard rejects the caller's own id.
    let r = mint(serde_json::json!({
        "name": "self-named key",
        "scopes": ["read"],
        "user_id": caller_id,
    }))
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CREATED,
        "naming your own principal is not an escalation"
    );
}
