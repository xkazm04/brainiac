//! The skill publish path (DATABASE_URL-gated): the last step of the
//! contribution→distribution loop, driven over real HTTP with real scoped
//! tokens.
//!
//! The claims under test:
//!
//! - **A draft serves nothing, a published version serves.** `skill_propose`
//!   drafts; `download` 404s until a named human publishes; after publish the
//!   bundle is served. The loop closes without raw SQL.
//! - **Publishing is the maintainer gate.** `lib:propose` (the agent's scope)
//!   cannot publish; `lib:publish` can.
//! - **Publishing is idempotent.** A second call is not an error, reports the
//!   version already serving, and mints no second version.
//! - **A skill with nothing to sign 404s**, and so does an unknown slug.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
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

#[tokio::test]
async fn propose_then_publish_closes_the_skill_loop_and_replays_cleanly() {
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

    // ── the agent proposes ──────────────────────────────────────────────
    let created: serde_json::Value = http
        .post(format!("{base}/v1/library/skills/propose"))
        .bearer_auth(&agent)
        .json(&json!({
            "name": "Review migrations",
            "instructions_md": "# Review migrations\n1. RLS on every new table\n2. grants for brainiac_app",
            "summary": "the schema-change checklist",
            "domain": "database"
        }))
        .send()
        .await
        .expect("propose")
        .json()
        .await
        .expect("json");
    assert_eq!(created["outcome"], "created");
    let slug = created["slug"].as_str().expect("slug").to_string();
    assert_eq!(slug, "review-migrations");

    let download = format!("{base}/v1/library/skills/{slug}/download");
    let publish = format!("{base}/v1/library/skills/{slug}/publish");

    // ── before publish: the draft serves nothing ────────────────────────
    let r = http
        .get(&download)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("download");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a draft nobody signed is not served"
    );

    // ── the gate: the proposing scope cannot publish ────────────────────
    let r = http
        .post(&publish)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("publish as agent");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::FORBIDDEN,
        "an agent proposes; a human publishes"
    );

    // ── the human publishes ─────────────────────────────────────────────
    let r = http
        .post(&publish)
        .bearer_auth(&maintainer)
        .send()
        .await
        .expect("publish");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let first: serde_json::Value = r.json().await.expect("json");
    assert_eq!(first["newly_published"], true);
    assert_eq!(first["semver"], "0.1.0");
    assert!(first["published_at"].is_string(), "stamped by the database");

    // ── after publish: the bundle serves ────────────────────────────────
    let r = http
        .get(&download)
        .bearer_auth(&agent)
        .send()
        .await
        .expect("download");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::OK,
        "the published bundle is served"
    );
    let bundle: serde_json::Value = r.json().await.expect("json");
    assert_eq!(bundle["slug"], "review-migrations");
    assert_eq!(bundle["semver"], "0.1.0");
    assert!(bundle["content_md"]
        .as_str()
        .expect("body")
        .contains("RLS on every new table"));

    // The catalog now says so too.
    let list: serde_json::Value = http
        .get(format!("{base}/v1/library/skills"))
        .bearer_auth(&agent)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    let mine = list["skills"]
        .as_array()
        .expect("skills")
        .iter()
        .find(|s| s["slug"] == "review-migrations")
        .expect("listed");
    assert_eq!(mine["maturity"], "published");
    assert_eq!(mine["downloadable"], true);

    // ── idempotency: a replay is not an error and mints nothing ─────────
    let r = http
        .post(&publish)
        .bearer_auth(&maintainer)
        .send()
        .await
        .expect("publish again");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::OK,
        "publishing an already-published skill is not an error"
    );
    let second: serde_json::Value = r.json().await.expect("json");
    assert_eq!(second["newly_published"], false);
    assert_eq!(second["semver"], first["semver"]);
    assert_eq!(
        second["published_at"], first["published_at"],
        "the replay did not re-stamp the signature"
    );

    let versions: i64 = sqlx::query_scalar("SELECT count(*) FROM skill_versions WHERE org_id = $1")
        .bind(ctx.org)
        .fetch_one(&ctx.admin)
        .await
        .expect("count");
    assert_eq!(versions, 1, "the replay created no second version");

    // ── nothing to sign ─────────────────────────────────────────────────
    let r = http
        .post(format!("{base}/v1/library/skills/no-such-skill/publish"))
        .bearer_auth(&maintainer)
        .send()
        .await
        .expect("publish unknown");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);
}
