//! Skill bundles + versioning (DATABASE_URL-gated): a skill is a multi-file
//! bundle that improves over time, and improving it never disturbs the agents
//! currently pulling it.
//!
//! The claims under test:
//!
//! - **Resources survive a live round trip.** A bundle proposed with auxiliary
//!   files is served, verbatim, by both `GET /…/download` and MCP
//!   `skill_fetch` — not merely stored.
//! - **A published skill takes a NEXT VERSION, not a `Duplicate`.** Proposing
//!   against a published slug drafts v2 instead of turning the agent away.
//! - **A draft v2 is unservable while v1 serves.** This is the dangerous edge
//!   of versioning: an unsigned improvement must not leak to agents, and the
//!   signed version must not disappear while it waits.
//! - **The bound is on the whole bundle**, enforced once in the store, and the
//!   same refusal reaches both surfaces.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_server::mcp::{handle_message, McpState};
use brainiac_store::Store;
use serde_json::json;
use uuid::Uuid;

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
    org: Uuid,
    team: Uuid,
    user_maint: Uuid,
    user_agent: Uuid,
}

async fn setup(url: &str) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences, api_tokens,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(url).await.expect("connect");
    let (org, team) = (Uuid::new_v4(), Uuid::new_v4());
    let (user_maint, user_agent) = (Uuid::new_v4(), Uuid::new_v4());

    let p = brainiac_pipeline::pipeline_principal(org);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team, org, "platform")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user_maint, org, "lead@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_user(&mut tx, user_agent, org, "agent@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_member(&mut tx, team, user_maint, "maintainer")
        .await
        .expect("m");
    brainiac_store::orgs::upsert_member(&mut tx, team, user_agent, "member")
        .await
        .expect("m");
    tx.commit().await.expect("commit");

    Ctx {
        store,
        admin,
        org,
        team,
        user_maint,
        user_agent,
    }
}

async fn scoped_token(ctx: &Ctx, user: Uuid, name: &str, scopes: &[&str]) -> String {
    let (secret, prefix) = brainiac_server::auth::mint_secret();
    let hash = brainiac_server::auth::hash_token(&secret);
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    brainiac_store::tokens::create(
        &ctx.admin,
        Uuid::new_v4(),
        ctx.org,
        user,
        name,
        &prefix,
        &hash,
        &scopes,
        None,
        user,
    )
    .await
    .expect("create token");
    secret
}

async fn boot_http(ctx: &Ctx) -> String {
    let tokens = json!({
        "tok_maint": {"org": ctx.org, "user": ctx.user_maint, "teams": [ctx.team]},
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

fn call(id: i64, tool: &str, args: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": tool, "arguments": args}
    })
}

/// The JSON payload an MCP tool returned (tools answer with a text content
/// block holding JSON).
fn mcp_payload(resp: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(resp["result"]["content"][0]["text"].as_str().expect("text"))
        .expect("payload json")
}

fn mcp_state(ctx: &Ctx) -> McpState {
    McpState {
        store: ctx.store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        reranker: None,
        embedding_version: 1,
        principal: brainiac_core::Principal {
            org_id: ctx.org,
            user_id: ctx.user_agent,
            team_ids: vec![ctx.team],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        session_remote: None,
        token_id: None,
    }
}

#[tokio::test]
async fn a_skill_carries_a_bundle_and_improves_without_disturbing_agents() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    std::env::set_var("BRAINIAC_LIB_PROPOSE_PER_HOUR", "50");
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let agent = scoped_token(&ctx, ctx.user_agent, "agent", &["lib:read", "lib:propose"]).await;
    let maintainer =
        scoped_token(&ctx, ctx.user_maint, "maint", &["lib:read", "lib:publish"]).await;

    let propose_url = format!("{base}/v1/library/skills/propose");
    let slug = "review-migrations";
    let download = format!("{base}/v1/library/skills/{slug}/download");
    let publish = format!("{base}/v1/library/skills/{slug}/publish");

    // ── v1: a multi-file bundle, proposed and published ─────────────────
    let created: serde_json::Value = http
        .post(&propose_url)
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Review migrations",
            "instructions_md": "# Review migrations\nv1 body",
            "summary": "the schema-change checklist",
            "domain": "database",
            "resources": [
                {"path": "references/rls.md", "content": "every new table needs a policy"},
                {"path": "scripts/check.sh", "content": "#!/bin/sh\necho checking"}
            ]
        }))
        .send()
        .await
        .expect("propose v1")
        .json()
        .await
        .expect("json");
    assert_eq!(created["outcome"], "created");
    assert_eq!(created["semver"], "0.1.0");

    let r = http
        .post(&publish)
        .bearer_auth(&maintainer)
        .send()
        .await
        .expect("publish v1");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    // The resources round trip over REST, verbatim.
    let bundle: serde_json::Value = http
        .get(&download)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("download v1")
        .json()
        .await
        .expect("json");
    assert_eq!(bundle["semver"], "0.1.0");
    let res = bundle["resources"].as_array().expect("resources array");
    assert_eq!(res.len(), 2, "the bundle is not body-only");
    assert_eq!(res[0]["path"], "references/rls.md");
    assert_eq!(res[0]["content"], "every new table needs a policy");
    assert_eq!(res[1]["path"], "scripts/check.sh");

    // …and over MCP `skill_fetch`, which is what a coding agent actually calls.
    let mcp = mcp_state(&ctx);
    let raw = handle_message(&mcp, &call(1, "skill_fetch", json!({"slug": slug})))
        .await
        .expect("skill_fetch v1");
    let fetched = mcp_payload(&raw);
    assert_eq!(fetched["published"], true);
    assert_eq!(fetched["semver"], "0.1.0");
    assert_eq!(
        fetched["resources"].as_array().expect("arr").len(),
        2,
        "MCP serves the whole bundle, not just the body"
    );

    // ── v2: proposing against a PUBLISHED skill makes a version ─────────
    let v2: serde_json::Value = http
        .post(&propose_url)
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Review migrations",
            "instructions_md": "# Review migrations\nv2 body — now covers grants",
            "summary": "the schema-change checklist",
            "domain": "database",
            "resources": [{"path": "references/rls.md", "content": "v2 reference"}]
        }))
        .send()
        .await
        .expect("propose v2")
        .json()
        .await
        .expect("json");
    assert_eq!(
        v2["outcome"], "new_version",
        "an improvement to a published skill is a version, not a duplicate"
    );
    assert_eq!(v2["semver"], "0.2.0");
    assert_eq!(v2["serving_semver"], "0.1.0");

    // ── the risky claim: v2 is a draft, so v1 keeps serving ─────────────
    let still: serde_json::Value = http
        .get(&download)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("download while v2 drafts")
        .json()
        .await
        .expect("json");
    assert_eq!(
        still["semver"], "0.1.0",
        "an unsigned v2 must not be served"
    );
    assert!(
        still["content_md"].as_str().expect("body").contains("v1"),
        "the body served is still v1's"
    );
    assert_eq!(
        still["resources"].as_array().expect("arr").len(),
        2,
        "v1's bundle is intact while v2 waits"
    );

    let raw = handle_message(&mcp, &call(2, "skill_fetch", json!({"slug": slug})))
        .await
        .expect("skill_fetch while v2 drafts");
    assert_eq!(
        mcp_payload(&raw)["semver"],
        "0.1.0",
        "MCP does not leak the unsigned v2 either"
    );

    // The catalog shows both, one of them marked a draft.
    let detail: serde_json::Value = http
        .get(format!("{base}/v1/library/skills/{slug}"))
        .bearer_auth(&agent)
        .send()
        .await
        .expect("detail")
        .json()
        .await
        .expect("json");
    let versions = detail["versions"].as_array().expect("versions");
    assert_eq!(versions.len(), 2, "history shows both versions");
    let published: Vec<&serde_json::Value> = versions
        .iter()
        .filter(|v| v["published"] == json!(true))
        .collect();
    assert_eq!(published.len(), 1, "exactly one version is signed");
    assert_eq!(published[0]["semver"], "0.1.0");

    // ── publishing v2 advances what agents receive ──────────────────────
    let signed: serde_json::Value = http
        .post(&publish)
        .bearer_auth(&maintainer)
        .send()
        .await
        .expect("publish v2")
        .json()
        .await
        .expect("json");
    assert_eq!(signed["newly_published"], true);
    assert_eq!(signed["semver"], "0.2.0");

    let now: serde_json::Value = http
        .get(&download)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("download v2")
        .json()
        .await
        .expect("json");
    assert_eq!(now["semver"], "0.2.0");
    assert!(now["content_md"]
        .as_str()
        .expect("body")
        .contains("now covers grants"));
    assert_eq!(
        now["resources"].as_array().expect("arr").len(),
        1,
        "v2's bundle replaced v1's"
    );

    // ── a pending draft still collapses: one unsigned draft at a time ───
    let d1: serde_json::Value = http
        .post(&propose_url)
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Review migrations",
            "instructions_md": "# Review migrations\nv3 body"
        }))
        .send()
        .await
        .expect("propose v3")
        .json()
        .await
        .expect("json");
    assert_eq!(
        d1["outcome"], "new_version",
        "v2 is signed, so v3 may draft"
    );
    assert_eq!(d1["semver"], "0.3.0");

    let d2: serde_json::Value = http
        .post(&propose_url)
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Review migrations",
            "instructions_md": "# Review migrations\nv4 body"
        }))
        .send()
        .await
        .expect("propose v4")
        .json()
        .await
        .expect("json");
    assert_eq!(
        d2["outcome"], "duplicate",
        "a draft is already waiting for a signature — no second one stacks"
    );

    let drafts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM skill_versions WHERE org_id = $1 AND published_by IS NULL",
    )
    .bind(ctx.org)
    .fetch_one(&ctx.admin)
    .await
    .expect("count");
    assert_eq!(drafts, 1, "exactly one unsigned draft exists");
}

#[tokio::test]
async fn the_bound_is_on_the_whole_bundle_and_both_surfaces_agree() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    std::env::set_var("BRAINIAC_LIB_PROPOSE_PER_HOUR", "50");
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();
    let agent = scoped_token(&ctx, ctx.user_agent, "agent", &["lib:read", "lib:propose"]).await;

    let max = brainiac_store::library::MAX_SKILL_BUNDLE_CHARS;

    // A body that would have been refused by the OLD 20k per-body cap is now
    // fine — this is the point of the change: a real skill fits.
    let big_body = "x".repeat(40_000);
    let ok: serde_json::Value = http
        .post(format!("{base}/v1/library/skills/propose"))
        .bearer_auth(&agent)
        .json(&json!({
            "name": "A long procedure",
            "instructions_md": big_body,
        }))
        .send()
        .await
        .expect("propose big")
        .json()
        .await
        .expect("json");
    assert_eq!(
        ok["outcome"], "created",
        "a 40k-char skill is storable now; the old cap was 20k"
    );

    // The bound counts RESOURCES too — a small body with huge attachments is
    // refused, which a per-body cap could never catch.
    let r = http
        .post(format!("{base}/v1/library/skills/propose"))
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Smuggled bulk",
            "instructions_md": "# tiny body",
            "resources": [{"path": "big.txt", "content": "y".repeat(max + 1)}]
        }))
        .send()
        .await
        .expect("propose oversized");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the cap is on the bundle, not the body"
    );

    // MCP refuses the same bundle by the same number — one constant, two
    // surfaces, no drift.
    let mcp = mcp_state(&ctx);
    let raw = handle_message(
        &mcp,
        &call(
            1,
            "skill_propose",
            json!({
                "name": "Smuggled bulk over mcp",
                "instructions_md": "# tiny body",
                "resources": [{"path": "big.txt", "content": "y".repeat(max + 1)}]
            }),
        ),
    )
    .await
    .expect("skill_propose oversized");
    let payload = mcp_payload(&raw);
    assert_eq!(payload["outcome"], "too_large");
    assert!(
        payload["note"]
            .as_str()
            .expect("note")
            .contains(&max.to_string()),
        "the refusal names the shared limit"
    );

    // And an ordinary bundle over MCP round-trips its resources.
    let raw = handle_message(
        &mcp,
        &call(
            2,
            "skill_propose",
            json!({
                "name": "Bundled over mcp",
                "instructions_md": "# body",
                "resources": [{"path": "ref.md", "content": "attached"}]
            }),
        ),
    )
    .await
    .expect("skill_propose bundle");
    assert_eq!(mcp_payload(&raw)["outcome"], "created");

    let stored: serde_json::Value = sqlx::query_scalar(
        "SELECT v.resources FROM skill_versions v
         JOIN skills s ON s.id = v.skill_id
         WHERE s.slug = 'bundled-over-mcp'",
    )
    .fetch_one(&ctx.admin)
    .await
    .expect("stored resources");
    assert_eq!(stored[0]["path"], "ref.md");
    assert_eq!(stored[0]["content"], "attached");
}
