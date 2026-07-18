//! Developer-onboarding pairing tests (DATABASE_URL-gated): the projects
//! registry + device-flow pairing driven over real HTTP.
//!
//! The claims under test are the onboarding contract:
//!
//! - **The whitelist gates approval.** A pairing for an unregistered remote
//!   cannot be approved (409); registering the repo under a project is what
//!   unlocks it, and the project is DERIVED from the remote, never chosen at
//!   approval time.
//! - **The key is minted at claim, exactly once.** The approving poll returns
//!   the secret; every later poll says `claimed` and carries no token.
//! - **The minted key is scoped.** read+write and project-bound: it can
//!   search memories but cannot list tokens (admin) — a leaked onboarding key
//!   must not mint more keys.
//! - **A denied pairing mints nothing.**

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_store::Store;
use serde_json::json;
use uuid::Uuid;

struct Ctx {
    store: Store,
    org: Uuid,
    team: Uuid,
    user: Uuid,
}

async fn setup(url: &str) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE onboard_requests, project_repos, projects, api_tokens,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(url).await.expect("connect");
    let (org, team, user) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let p = brainiac_pipeline::pipeline_principal(org);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team, org, "payments")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user, org, "lead@meridian.test")
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, team, user, "maintainer")
        .await
        .expect("member");
    tx.commit().await.expect("commit");
    Ctx {
        store,
        org,
        team,
        user,
    }
}

async fn boot_http(ctx: &Ctx) -> String {
    let tokens = json!({
        "tok_admin": {"org": ctx.org, "user": ctx.user, "teams": [ctx.team]},
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());
    let app = brainiac_server::http::router(
        ctx.store.clone(),
        Arc::new(DeterministicEmbedder::default()),
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
    format!("http://{addr}")
}

#[tokio::test]
async fn pairing_flow_mints_a_scoped_key_exactly_once() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    // ── the registry: create a project, whitelist a repo ────────────────
    let r = http
        .post(format!("{base}/v1/projects"))
        .bearer_auth("tok_admin")
        .json(&json!({"name": "payments"}))
        .send()
        .await
        .expect("create project");
    assert_eq!(r.status(), reqwest::StatusCode::CREATED);
    let project: serde_json::Value = r.json().await.expect("json");
    let project_id = project["id"].as_str().expect("id").to_string();

    // Duplicate name is a 409, not a second project.
    let r = http
        .post(format!("{base}/v1/projects"))
        .bearer_auth("tok_admin")
        .json(&json!({"name": "payments"}))
        .send()
        .await
        .expect("dup project");
    assert_eq!(r.status(), reqwest::StatusCode::CONFLICT);

    // Whitelist via the ssh spelling; the registry stores the normalized form.
    let r = http
        .post(format!("{base}/v1/projects/{project_id}/repos"))
        .bearer_auth("tok_admin")
        .json(&json!({"remote": "git@github.com:Meridian/payments-api.git"}))
        .send()
        .await
        .expect("add repo");
    assert_eq!(r.status(), reqwest::StatusCode::CREATED);
    let repo: serde_json::Value = r.json().await.expect("json");
    assert_eq!(repo["remote"], "github.com/meridian/payments-api");

    // ── pairing: start (unauthenticated, https spelling of the same repo) ─
    let r = http
        .post(format!("{base}/v1/onboard/start"))
        .json(&json!({
            "remote": "https://github.com/meridian/payments-api",
            "label": "dev@laptop"
        }))
        .send()
        .await
        .expect("start");
    assert_eq!(r.status(), reqwest::StatusCode::CREATED);
    let started: serde_json::Value = r.json().await.expect("json");
    let device_code = started["device_code"].as_str().expect("device_code");
    assert_eq!(started["remote"], "github.com/meridian/payments-api");

    // Pending until a human decides.
    let poll = |code: &str| {
        http.post(format!("{base}/v1/onboard/poll"))
            .json(&json!({"device_code": code}))
            .send()
    };
    let p: serde_json::Value = poll(device_code)
        .await
        .expect("poll")
        .json()
        .await
        .expect("json");
    assert_eq!(p["status"], "pending");
    assert!(p.get("token").is_none());

    // The console queue sees it, matched to the project by the whitelist.
    let r = http
        .get(format!("{base}/v1/onboard/requests"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("requests");
    let queue: serde_json::Value = r.json().await.expect("json");
    let req = &queue["requests"][0];
    assert_eq!(req["remote"], "github.com/meridian/payments-api");
    assert_eq!(req["project_name"], "payments");
    let request_id = req["id"].as_str().expect("id").to_string();

    // The queue is an admin surface: the pairing's own codes don't open it.
    let r = http
        .get(format!("{base}/v1/onboard/requests"))
        .send()
        .await
        .expect("anon requests");
    assert_eq!(r.status(), reqwest::StatusCode::UNAUTHORIZED);

    // ── approve, then claim: the token appears once ─────────────────────
    let r = http
        .post(format!("{base}/v1/onboard/requests/{request_id}/approve"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("approve");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let claimed: serde_json::Value = poll(device_code)
        .await
        .expect("poll")
        .json()
        .await
        .expect("json");
    assert_eq!(claimed["status"], "approved");
    assert_eq!(claimed["project_name"], "payments");
    let secret = claimed["token"].as_str().expect("token").to_string();
    assert!(secret.starts_with("brk_"));

    let again: serde_json::Value = poll(device_code)
        .await
        .expect("poll")
        .json()
        .await
        .expect("json");
    assert_eq!(again["status"], "claimed");
    assert!(again.get("token").is_none());

    // ── the minted key: works for memory reads, refused as an admin ─────
    let r = http
        .post(format!("{base}/v1/memories/search"))
        .bearer_auth(&secret)
        .json(&json!({"query": "onboarding connectivity check", "k": 1}))
        .send()
        .await
        .expect("search");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let r = http
        .get(format!("{base}/v1/tokens"))
        .bearer_auth(&secret)
        .send()
        .await
        .expect("tokens as device key");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    // The ledger shows the key bound to its project.
    let r = http
        .get(format!("{base}/v1/tokens"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("tokens");
    let tokens: serde_json::Value = r.json().await.expect("json");
    let minted = tokens["tokens"]
        .as_array()
        .expect("array")
        .iter()
        .find(|t| t["name"].as_str().unwrap_or("").starts_with("onboard ·"))
        .expect("minted key listed");
    assert_eq!(minted["project_name"], "payments");
    assert_eq!(
        minted["scopes"].as_array().expect("scopes").len(),
        2,
        "read+write only"
    );

    // ── PR0: writes are stamped with the key's project ──────────────────
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    let project_uuid = claimed["project_id"].as_str().expect("project id");

    // A write from the onboarded (project-scoped) key stamps its source.
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth(&secret)
        .json(&json!({"content": "Payments retries use exponential backoff."}))
        .send()
        .await
        .expect("memory_add");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
    let receipt: serde_json::Value = r.json().await.expect("json");
    let stamped: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM sources WHERE id = $1::uuid")
            .bind(receipt["source_id"].as_str().expect("source_id"))
            .fetch_one(&admin)
            .await
            .expect("source row");
    assert_eq!(
        stamped.map(|u| u.to_string()).as_deref(),
        Some(project_uuid)
    );

    // An org-wide key writes org-shared (NULL) by default…
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_admin")
        .json(&json!({"content": "Org convention: RFC-style decision records."}))
        .send()
        .await
        .expect("memory_add org");
    let receipt: serde_json::Value = r.json().await.expect("json");
    let stamped: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM sources WHERE id = $1::uuid")
            .bind(receipt["source_id"].as_str().expect("source_id"))
            .fetch_one(&admin)
            .await
            .expect("source row");
    assert_eq!(stamped, None, "org key ⇒ org-shared, never defaulted");

    // …but can attribute explicitly (the CI/import case)…
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_admin")
        .json(&json!({
            "content": "Payments imports settle nightly.",
            "project_id": project_uuid
        }))
        .send()
        .await
        .expect("memory_add attributed");
    assert_eq!(r.status(), reqwest::StatusCode::ACCEPTED);
    let receipt: serde_json::Value = r.json().await.expect("json");
    let stamped: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM sources WHERE id = $1::uuid")
            .bind(receipt["source_id"].as_str().expect("source_id"))
            .fetch_one(&admin)
            .await
            .expect("source row");
    assert_eq!(
        stamped.map(|u| u.to_string()).as_deref(),
        Some(project_uuid)
    );

    // …and a project the org does not own is a 400, not a silent mint.
    let r = http
        .post(format!("{base}/v1/memories"))
        .bearer_auth("tok_admin")
        .json(&json!({
            "content": "mis-attributed",
            "project_id": Uuid::new_v4()
        }))
        .send()
        .await
        .expect("memory_add bad project");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unwhitelisted_remotes_cannot_be_approved_and_denial_mints_nothing() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let r = http
        .post(format!("{base}/v1/onboard/start"))
        .json(&json!({"remote": "https://github.com/meridian/shadow-repo"}))
        .send()
        .await
        .expect("start");
    let started: serde_json::Value = r.json().await.expect("json");
    let device_code = started["device_code"].as_str().expect("dc").to_string();

    let r = http
        .get(format!("{base}/v1/onboard/requests"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("requests");
    let queue: serde_json::Value = r.json().await.expect("json");
    let req = &queue["requests"][0];
    assert!(req["project_name"].is_null(), "no whitelist match");
    let request_id = req["id"].as_str().expect("id").to_string();

    // Approval refuses: the remote is registered nowhere.
    let r = http
        .post(format!("{base}/v1/onboard/requests/{request_id}/approve"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("approve");
    assert_eq!(r.status(), reqwest::StatusCode::CONFLICT);

    // Denial ends it; the poller is told and no key exists.
    let r = http
        .post(format!("{base}/v1/onboard/requests/{request_id}/deny"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("deny");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let p: serde_json::Value = http
        .post(format!("{base}/v1/onboard/poll"))
        .json(&json!({"device_code": device_code}))
        .send()
        .await
        .expect("poll")
        .json()
        .await
        .expect("json");
    assert_eq!(p["status"], "denied");
    assert!(p.get("token").is_none());

    let r = http
        .get(format!("{base}/v1/tokens"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("tokens");
    let tokens: serde_json::Value = r.json().await.expect("json");
    assert_eq!(
        tokens["tokens"].as_array().expect("array").len(),
        0,
        "denial minted nothing"
    );

    // An unknown device code is a 404, and garbage remotes never open a row.
    let r = http
        .post(format!("{base}/v1/onboard/poll"))
        .json(&json!({"device_code": "obc_deadbeef"}))
        .send()
        .await
        .expect("poll unknown");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);
    let r = http
        .post(format!("{base}/v1/onboard/start"))
        .json(&json!({"remote": "not a remote"}))
        .send()
        .await
        .expect("start garbage");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
}

/// Monorepo support (migrations/0039_project_path_prefix.sql): one remote,
/// several projects, split by `path_prefix`. Covers the split itself, the
/// longest-prefix tiebreak, and the whole-repo ('') fallback every
/// pre-monorepo caller (no `path`) still gets — the two prior tests in this
/// file exercise exactly that back-compat path and must stay green.
#[tokio::test]
async fn monorepo_path_prefix_resolves_by_longest_match() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let create_project = |name: &'static str| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let r = http
                .post(format!("{base}/v1/projects"))
                .bearer_auth("tok_admin")
                .json(&json!({"name": name}))
                .send()
                .await
                .expect("create project");
            assert_eq!(r.status(), reqwest::StatusCode::CREATED);
            let project: serde_json::Value = r.json().await.expect("json");
            project["id"].as_str().expect("id").to_string()
        }
    };
    let web_id = create_project("web").await;
    let api_id = create_project("api").await;
    let root_id = create_project("apps-root").await;
    let whole_id = create_project("whole-repo").await;

    let remote = "https://github.com/acme/mono";
    let add_repo = |project_id: String, path_prefix: &'static str| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let r = http
                .post(format!("{base}/v1/projects/{project_id}/repos"))
                .bearer_auth("tok_admin")
                .json(&json!({"remote": remote, "path_prefix": path_prefix}))
                .send()
                .await
                .expect("add repo");
            assert_eq!(
                r.status(),
                reqwest::StatusCode::CREATED,
                "path {path_prefix:?}"
            );
            let repo: serde_json::Value = r.json().await.expect("json");
            assert_eq!(repo["path_prefix"], path_prefix);
        }
    };
    // Same remote, four different prefixes — the whole point of the split.
    add_repo(web_id.clone(), "apps/web").await;
    add_repo(api_id.clone(), "apps/api").await;
    add_repo(root_id.clone(), "apps").await;
    add_repo(whole_id.clone(), "").await;

    // Re-adding the same (remote, path_prefix) pair is still a 409 — the
    // uniqueness constraint moved from (org, remote) to (org, remote,
    // path_prefix), it didn't disappear.
    let r = http
        .post(format!("{base}/v1/projects/{web_id}/repos"))
        .bearer_auth("tok_admin")
        .json(&json!({"remote": remote, "path_prefix": "apps/web"}))
        .send()
        .await
        .expect("dup repo");
    assert_eq!(r.status(), reqwest::StatusCode::CONFLICT);

    let start = |path: &'static str| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let mut body = json!({"remote": remote, "label": "dev@laptop"});
            if !path.is_empty() {
                body["path"] = json!(path);
            }
            let r = http
                .post(format!("{base}/v1/onboard/start"))
                .json(&body)
                .send()
                .await
                .expect("start");
            assert_eq!(r.status(), reqwest::StatusCode::CREATED);
            let started: serde_json::Value = r.json().await.expect("json");
            started["device_code"]
                .as_str()
                .expect("device_code")
                .to_string()
        }
    };
    let queue_match_for = |request_index: usize| {
        let http = http.clone();
        let base = base.clone();
        async move {
            let r = http
                .get(format!("{base}/v1/onboard/requests"))
                .bearer_auth("tok_admin")
                .send()
                .await
                .expect("requests");
            let queue: serde_json::Value = r.json().await.expect("json");
            queue["requests"][request_index]["project_name"]
                .as_str()
                .map(|s| s.to_string())
        }
    };

    // Deep path under apps/web: apps/web (8 chars) beats apps (4 chars) and
    // the '' fallback — longest prefix wins, resolves to "web".
    let _web_code = start("apps/web/src/x").await;
    assert_eq!(queue_match_for(0).await.as_deref(), Some("web"));

    // A sibling path under apps/, not under apps/web or apps/api: only the
    // "apps" row's prefix is actually a prefix of it, so it resolves there —
    // proof the match isn't "any row whose prefix looks similar".
    let _root_code = start("apps/other").await;
    assert_eq!(queue_match_for(1).await.as_deref(), Some("apps-root"));

    // No path at all (whole-repo checkout): resolves to the '' row, exactly
    // the behavior every caller had before path_prefix existed.
    let _whole_code = start("").await;
    assert_eq!(queue_match_for(2).await.as_deref(), Some("whole-repo"));

    // Segment-boundary, not bare character prefix: `appserver/x` character-
    // starts with "apps" but is NOT under the `apps` segment, so it must fall
    // through to the '' whole-repo row — never mis-attribute to "apps-root".
    // (A bare `left(path, len(prefix)) = prefix` match would wrongly claim it —
    // a cross-project leak; the Director-added segment guard prevents it.)
    let _seg_code = start("appserver/x").await;
    assert_eq!(queue_match_for(3).await.as_deref(), Some("whole-repo"));

    // And the resolution isn't just cosmetic in the queue view — approval
    // (which re-runs the same lookup) actually lands the key in "web".
    let r = http
        .get(format!("{base}/v1/onboard/requests"))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("requests");
    let queue: serde_json::Value = r.json().await.expect("json");
    let web_request_id = queue["requests"][0]["id"].as_str().expect("id").to_string();
    let r = http
        .post(format!(
            "{base}/v1/onboard/requests/{web_request_id}/approve"
        ))
        .bearer_auth("tok_admin")
        .send()
        .await
        .expect("approve");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let decision: serde_json::Value = r.json().await.expect("json");
    assert_eq!(decision["project_name"], "web");
    assert_eq!(decision["project_id"], web_id);
}
