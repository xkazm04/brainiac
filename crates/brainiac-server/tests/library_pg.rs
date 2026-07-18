//! LB1 distribution-surface tests (DATABASE_URL-gated): the REST library API
//! driven over real HTTP with real scoped tokens, and the MCP library tools.
//!
//! The claims under test are the LIBRARY-PLAN LB1 gate:
//!
//! - **Another org's rules do not exist.** A scoped token from org B asking for
//!   org A's standard gets "not found", never "forbidden" — existence is itself
//!   information.
//! - **A `lib:read` token cannot adopt.** The maintainer gate takes
//!   `lib:publish`; a token minted to read standards must not be able to
//!   decree one.
//! - **Usage is counted for a team, never a person.** The events written by
//!   the serve path carry the caller's team id, and the table has no user
//!   column to fill.
//! - **Agents are served only ratified judgment.** MCP `standards_for` returns
//!   adopted rules only; `skill_fetch` serves only versions a named human
//!   published — a draft returns no content, exactly like an unsigned page.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{
    Enforcement, LibraryArtifactKind, LibraryUsageEvent, Principal, StandardProvenanceKind,
};
use brainiac_server::mcp::{handle_message, McpState};
use brainiac_store::{library, Store};
use serde_json::json;
use uuid::Uuid;

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
    org_a: Uuid,
    team_a: Uuid,
    user_maint: Uuid,
    user_reader: Uuid,
    org_b: Uuid,
    user_b: Uuid,
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
    let (org_a, team_a) = (Uuid::new_v4(), Uuid::new_v4());
    let (user_maint, user_reader) = (Uuid::new_v4(), Uuid::new_v4());
    let (org_b, user_b) = (Uuid::new_v4(), Uuid::new_v4());

    let p = brainiac_pipeline::pipeline_principal(org_a);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_a, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team_a, org_a, "payments")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user_maint, org_a, "lead@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_user(&mut tx, user_reader, org_a, "agent@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_member(&mut tx, team_a, user_maint, "maintainer")
        .await
        .expect("m");
    brainiac_store::orgs::upsert_member(&mut tx, team_a, user_reader, "member")
        .await
        .expect("m");
    tx.commit().await.expect("commit");

    let pb = brainiac_pipeline::pipeline_principal(org_b);
    let mut tx = store.scoped_tx(&pb).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_b, "rival")
        .await
        .expect("org b");
    brainiac_store::orgs::upsert_user(&mut tx, user_b, org_b, "outsider@rival.test")
        .await
        .expect("u b");
    tx.commit().await.expect("commit");

    Ctx {
        store,
        admin,
        org_a,
        team_a,
        user_maint,
        user_reader,
        org_b,
        user_b,
    }
}

/// Mint a managed `brk_` token with explicit scopes for a user.
async fn scoped_token(ctx: &Ctx, org: Uuid, user: Uuid, name: &str, scopes: &[&str]) -> String {
    let (secret, prefix) = brainiac_server::auth::mint_secret();
    let hash = brainiac_server::auth::hash_token(&secret);
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    brainiac_store::tokens::create(
        &ctx.admin,
        Uuid::new_v4(),
        org,
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
    // One full-authority env token for the maintainer (the operator bootstrap
    // path); every restricted caller uses a managed brk_ token instead.
    let tokens = json!({
        "tok_maint": {"org": ctx.org_a, "user": ctx.user_maint, "teams": [ctx.team_a]},
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
async fn rest_scopes_gate_the_library_and_usage_counts_teams() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let reader = scoped_token(&ctx, ctx.org_a, ctx.user_reader, "agent", &["lib:read"]).await;
    let outsider = scoped_token(
        &ctx,
        ctx.org_b,
        ctx.user_b,
        "rival",
        &["lib:read", "lib:publish"],
    )
    .await;

    // ── the bridge over HTTP: divergence → candidate, idempotent ────────
    let d1 = Uuid::new_v4();
    {
        let p = brainiac_pipeline::pipeline_principal(ctx.org_a);
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        sqlx::query(
            "INSERT INTO practice_divergences
                (id, org_id, practice, summary, recommended_standard, impact, positions, model_ref)
             VALUES ($1, $2, 'Service retry policy', 'two teams, two backoffs',
                     'Exponential backoff with full jitter, max 30s', 'high', $3, 'test-model')",
        )
        .bind(d1)
        .bind(ctx.org_a)
        .bind(json!([{"team": "payments", "approach": "3x fixed"}]))
        .execute(&mut *tx)
        .await
        .expect("divergence");
        tx.commit().await.expect("commit");
    }

    // A lib:read token must not be able to ratify…
    let r = http
        .post(format!("{base}/v1/library/divergences/{d1}/ratify"))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("ratify as reader");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    // …the maintainer can, and doing it twice yields the same candidate.
    let ratify = |c: &reqwest::Client| {
        c.post(format!("{base}/v1/library/divergences/{d1}/ratify"))
            .bearer_auth("tok_maint")
            .send()
    };
    let first: serde_json::Value = ratify(&http)
        .await
        .expect("ratify")
        .json()
        .await
        .expect("json");
    let second: serde_json::Value = ratify(&http)
        .await
        .expect("ratify")
        .json()
        .await
        .expect("json");
    assert_eq!(first["standard_id"], second["standard_id"]);
    let std_id: Uuid = first["standard_id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid");

    // ── default list is ADOPTED only: the candidate is not yet servable ──
    let r: serde_json::Value = http
        .get(format!("{base}/v1/library/standards"))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(
        r["standards"].as_array().expect("arr").len(),
        0,
        "a proposal must never be served as the org's judgment: {r}"
    );

    // ── the gate: lib:read cannot adopt; lib:publish can ────────────────
    let r = http
        .post(format!("{base}/v1/library/standards/{std_id}/adopt"))
        .bearer_auth(&reader)
        .json(&json!({}))
        .send()
        .await
        .expect("adopt as reader");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    let r = http
        .post(format!("{base}/v1/library/standards/{std_id}/adopt"))
        .bearer_auth("tok_maint")
        .json(&json!({}))
        .send()
        .await
        .expect("adopt");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let r: serde_json::Value = http
        .get(format!("{base}/v1/library/standards?stack=general"))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(r["standards"].as_array().expect("arr").len(), 1);
    assert_eq!(r["standards"][0]["slug"], "service-retry-policy");

    // ── an evidence-free rule: refused plainly, adoptable only by decree ─
    let bare = Uuid::new_v4();
    {
        let p = Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint,
            team_ids: vec![ctx.team_a],
            project_id: None,
        };
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        library::insert_standard(
            &mut tx,
            &library::NewStandard {
                id: bare,
                org_id: ctx.org_a,
                origin: Default::default(),
                stack: "general".into(),
                category: "style".into(),
                slug: "tabs-vs-spaces".into(),
                statement: "Spaces.".into(),
                rationale: None,
                detail_md: None,
                enforcement: Enforcement::Recommended,
                provenance: vec![],
                author: Some(ctx.user_maint),
            },
        )
        .await
        .expect("bare");
        tx.commit().await.expect("commit");
    }
    let r = http
        .post(format!("{base}/v1/library/standards/{bare}/adopt"))
        .bearer_auth("tok_maint")
        .json(&json!({}))
        .send()
        .await
        .expect("adopt bare");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CONFLICT,
        "no provenance + no decree must be a 409 the maintainer can act on"
    );
    let r = http
        .post(format!("{base}/v1/library/standards/{bare}/adopt"))
        .bearer_auth("tok_maint")
        .json(&json!({"decree": true}))
        .send()
        .await
        .expect("adopt decree");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    // ── reject: the gate's "no", scoped like every other decision ────────
    let bin = Uuid::new_v4();
    {
        let p = Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint,
            team_ids: vec![ctx.team_a],
            project_id: None,
        };
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        library::insert_standard(
            &mut tx,
            &library::NewStandard {
                id: bin,
                org_id: ctx.org_a,
                origin: Default::default(),
                stack: "general".into(),
                category: "style".into(),
                slug: "rewrite-everything-in-brainfuck".into(),
                statement: "No.".into(),
                rationale: None,
                detail_md: None,
                enforcement: Enforcement::Experimental,
                provenance: vec![],
                author: Some(ctx.user_maint),
            },
        )
        .await
        .expect("candidate to reject");
        tx.commit().await.expect("commit");
    }
    let r = http
        .post(format!("{base}/v1/library/standards/{bin}/reject"))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("reject as reader");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::FORBIDDEN,
        "lib:read cannot say no either"
    );
    let r = http
        .post(format!("{base}/v1/library/standards/{bin}/reject"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("reject");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let r = http
        .post(format!("{base}/v1/library/standards/{bin}/reject"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("reject again");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "rejection is terminal — a second no is a stale board"
    );

    // ── another org's rules do not exist ─────────────────────────────────
    let r = http
        .get(format!("{base}/v1/library/standards/{std_id}"))
        .bearer_auth(&outsider)
        .send()
        .await
        .expect("cross-org get");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "not found, never forbidden — existence is itself information"
    );
    let r: serde_json::Value = http
        .get(format!("{base}/v1/library/standards?lifecycle=all"))
        .bearer_auth(&outsider)
        .send()
        .await
        .expect("cross-org list")
        .json()
        .await
        .expect("json");
    assert_eq!(r["standards"].as_array().expect("arr").len(), 0);

    // ── skills: a draft serves nothing; publishing serves + counts ──────
    let (skill_id, ver_id) = (Uuid::new_v4(), Uuid::new_v4());
    {
        let p = Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint,
            team_ids: vec![ctx.team_a],
            project_id: None,
        };
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        library::insert_skill(
            &mut tx,
            &library::NewSkill {
                id: skill_id,
                org_id: ctx.org_a,
                slug: "review-migrations".into(),
                name: "Review migrations".into(),
                description: Some("checklist for schema changes".into()),
                domain: Some("database".into()),
                proposed_by: None,
            },
        )
        .await
        .expect("skill");
        library::add_skill_version(
            &mut tx,
            &library::NewSkillVersion {
                id: ver_id,
                skill_id,
                org_id: ctx.org_a,
                semver: "1.0.0".into(),
                manifest: json!({"name": "review-migrations"}),
                content_md: "# Review migrations\ncheck RLS on every new table".into(),
                resources: json!([]),
            },
        )
        .await
        .expect("version");
        tx.commit().await.expect("commit");
    }
    let r = http
        .get(format!(
            "{base}/v1/library/skills/review-migrations/download"
        ))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("download draft");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a draft nobody signed is never served"
    );

    {
        let p = Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint,
            team_ids: vec![ctx.team_a],
            project_id: None,
        };
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        assert!(
            library::publish_skill_version(&mut tx, ver_id, ctx.user_maint)
                .await
                .expect("publish")
        );
        tx.commit().await.expect("commit");
    }
    let r: serde_json::Value = http
        .get(format!(
            "{base}/v1/library/skills/review-migrations/download"
        ))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("download")
        .json()
        .await
        .expect("json");
    assert_eq!(r["semver"], "1.0.0");
    assert!(r["content_md"].as_str().expect("md").contains("RLS"));

    // An explicit usage report, and then the shape of what got written.
    let r = http
        .post(format!("{base}/v1/library/usage"))
        .bearer_auth(&reader)
        .json(&json!({
            "artifact_kind": "skill",
            "artifact_id": skill_id,
            "version": "1.0.0",
            "event": "apply"
        }))
        .send()
        .await
        .expect("usage");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    // Every event this test produced is attributed to the reader's TEAM. The
    // table cannot even name the person — that is the never-a-leaderboard
    // invariant as a schema shape, checked here at the serving surface.
    let rows: Vec<(Option<Uuid>, String)> =
        sqlx::query_as("SELECT team_id, event FROM library_usage_events WHERE artifact_id = $1")
            .bind(skill_id)
            .fetch_all(&ctx.admin)
            .await
            .expect("events");
    assert_eq!(
        rows.len(),
        2,
        "one served fetch + one reported apply: {rows:?}"
    );
    for (team, _) in &rows {
        assert_eq!(
            *team,
            Some(ctx.team_a),
            "usage must carry the caller's team"
        );
    }
}

#[tokio::test]
async fn mcp_serves_only_adopted_rules_and_published_skills() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    let maintainer = Principal {
        org_id: ctx.org_a,
        user_id: ctx.user_maint,
        team_ids: vec![ctx.team_a],
        project_id: None,
    };

    // One adopted rule, one proposal, one draft-only skill.
    let (adopted, proposed, skill_id, ver_id) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    {
        let mut tx = ctx.store.scoped_tx(&maintainer).await.expect("tx");
        let mk = |id: Uuid, slug: &str| library::NewStandard {
            id,
            org_id: ctx.org_a,
            origin: Default::default(),
            stack: "rust".into(),
            category: "errors".into(),
            slug: slug.into(),
            statement: format!("{slug}: one sentence."),
            rationale: None,
            detail_md: Some("```rust\n// the example\n```".into()),
            enforcement: Enforcement::Mandatory,
            provenance: vec![(StandardProvenanceKind::Memory, Uuid::new_v4())],
            author: Some(ctx.user_maint),
        };
        library::insert_standard(&mut tx, &mk(adopted, "no-unwrap-in-handlers"))
            .await
            .expect("s1");
        library::insert_standard(&mut tx, &mk(proposed, "still-a-proposal"))
            .await
            .expect("s2");
        assert!(
            library::adopt_standard(&mut tx, adopted, ctx.user_maint, false)
                .await
                .expect("adopt")
        );
        library::insert_skill(
            &mut tx,
            &library::NewSkill {
                id: skill_id,
                org_id: ctx.org_a,
                slug: "triage-flaky-tests".into(),
                name: "Triage flaky tests".into(),
                description: Some("find and quarantine flaky tests".into()),
                domain: Some("testing".into()),
                proposed_by: None,
            },
        )
        .await
        .expect("skill");
        library::add_skill_version(
            &mut tx,
            &library::NewSkillVersion {
                id: ver_id,
                skill_id,
                org_id: ctx.org_a,
                semver: "0.1.0".into(),
                manifest: json!({"name": "triage-flaky-tests"}),
                content_md: "# Triage flaky tests".into(),
                resources: json!([]),
            },
        )
        .await
        .expect("version");
        tx.commit().await.expect("commit");
    }

    let state = Arc::new(McpState {
        store: ctx.store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_reader,
            team_ids: vec![ctx.team_a],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        session_remote: None,
    });
    let rpc = |id: i64, method: &str, params: serde_json::Value| json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let call = |id: i64, name: &str, args: serde_json::Value| {
        rpc(id, "tools/call", json!({ "name": name, "arguments": args }))
    };
    let text_of = |resp: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().expect("text"))
            .expect("payload json")
    };

    // The four tools are advertised.
    let tools = handle_message(state.as_ref(), &rpc(1, "tools/list", json!({})))
        .await
        .expect("tools/list");
    let names = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name").to_string())
        .collect::<Vec<_>>();
    for required in [
        "standards_for",
        "skill_search",
        "skill_fetch",
        "skill_report_usage",
    ] {
        assert!(
            names.contains(&required.to_string()),
            "missing tool {required}"
        );
    }

    // standards_for: the adopted rule only — a proposal never reaches an agent
    // as if it were policy.
    let r = handle_message(
        state.as_ref(),
        &call(2, "standards_for", json!({"stack": "rust"})),
    )
    .await
    .expect("standards_for");
    let payload = text_of(&r);
    let slugs: Vec<&str> = payload["standards"]
        .as_array()
        .expect("standards")
        .iter()
        .map(|s| s["slug"].as_str().expect("slug"))
        .collect();
    assert_eq!(
        slugs,
        vec!["no-unwrap-in-handlers"],
        "adopted only: {payload}"
    );
    assert_eq!(
        payload["standards"][0]["examples_md"], "```rust\n// the example\n```",
        "examples travel verbatim"
    );

    // skill_fetch on a draft-only skill: found, but no content — a draft
    // nobody signed must not reach a coding agent.
    let r = handle_message(
        state.as_ref(),
        &call(3, "skill_fetch", json!({"slug": "triage-flaky-tests"})),
    )
    .await
    .expect("skill_fetch");
    let payload = text_of(&r);
    assert_eq!(payload["published"], false, "{payload}");
    assert!(payload.get("content_md").is_none());

    // Publish, then the bundle serves.
    {
        let mut tx = ctx.store.scoped_tx(&maintainer).await.expect("tx");
        assert!(
            library::publish_skill_version(&mut tx, ver_id, ctx.user_maint)
                .await
                .expect("publish")
        );
        tx.commit().await.expect("commit");
    }
    let r = handle_message(
        state.as_ref(),
        &call(4, "skill_fetch", json!({"slug": "triage-flaky-tests"})),
    )
    .await
    .expect("skill_fetch published");
    let payload = text_of(&r);
    assert_eq!(payload["published"], true);
    assert_eq!(payload["semver"], "0.1.0");

    // skill_search finds it now that it is published.
    let r = handle_message(
        state.as_ref(),
        &call(5, "skill_search", json!({"query": "flaky"})),
    )
    .await
    .expect("skill_search");
    let payload = text_of(&r);
    assert_eq!(payload["skills"].as_array().expect("arr").len(), 1);

    // Reporting usage lands as the agent's TEAM.
    let r = handle_message(
        state.as_ref(),
        &call(
            6,
            "skill_report_usage",
            json!({
                "artifact_kind": "skill",
                "slug": "triage-flaky-tests",
                "event": "apply"
            }),
        ),
    )
    .await
    .expect("report usage");
    let payload = text_of(&r);
    assert_eq!(payload["recorded"], true);

    let teams: Vec<Option<Uuid>> =
        sqlx::query_scalar("SELECT team_id FROM library_usage_events WHERE artifact_id = $1")
            .bind(skill_id)
            .fetch_all(&ctx.admin)
            .await
            .expect("events");
    assert!(!teams.is_empty());
    assert!(teams.iter().all(|t| *t == Some(ctx.team_a)));
}

// ── follow-up 2: the Library's signals go red on their own ──────────────────

#[tokio::test]
async fn dead_rules_surface_themselves_on_the_leadership_report() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let p = Principal {
        org_id: ctx.org_a,
        user_id: ctx.user_maint,
        team_ids: vec![ctx.team_a],
        project_id: None,
    };
    let mk = |id: Uuid, slug: &str| library::NewStandard {
        id,
        org_id: ctx.org_a,
        origin: Default::default(),
        stack: "rust".into(),
        category: "errors".into(),
        slug: slug.into(),
        statement: format!("{slug}: one sentence."),
        rationale: None,
        detail_md: None,
        enforcement: Enforcement::Recommended,
        provenance: vec![(StandardProvenanceKind::Memory, Uuid::new_v4())],
        author: Some(ctx.user_maint),
    };

    // Three adopted rules — one used, one long-adopted and untouched, one
    // adopted just now and untouched — plus a candidate aging at the gate.
    let (used, dead, fresh, waiting) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        for (id, slug) in [
            (used, "used-rule"),
            (dead, "dead-rule"),
            (fresh, "fresh-rule"),
        ] {
            library::insert_standard(&mut tx, &mk(id, slug))
                .await
                .expect("insert");
            assert!(library::adopt_standard(&mut tx, id, ctx.user_maint, false)
                .await
                .expect("adopt"));
        }
        library::insert_standard(&mut tx, &mk(waiting, "waiting-candidate"))
            .await
            .expect("candidate");
        library::record_usage(
            &mut tx,
            ctx.org_a,
            LibraryArtifactKind::Standard,
            used,
            None,
            LibraryUsageEvent::Check,
            Some(ctx.team_a),
        )
        .await
        .expect("usage");
        tx.commit().await.expect("commit");
    }
    // Age the two that need a history: the dead rule was adopted well before
    // the dormancy window, the candidate has been waiting past the gate SLO.
    sqlx::query("UPDATE standards SET adopted_at = now() - interval '40 days' WHERE id = $1")
        .bind(dead)
        .execute(&ctx.admin)
        .await
        .expect("age");
    sqlx::query("UPDATE standards SET created_at = now() - interval '20 days' WHERE id = $1")
        .bind(waiting)
        .execute(&ctx.admin)
        .await
        .expect("age candidate");

    let report: serde_json::Value = http
        .get(format!("{base}/v1/analytics/knowledge-health"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("health")
        .json()
        .await
        .expect("json");

    let s = &report["signals"];
    assert_eq!(s["standards_adopted"], 3);
    assert_eq!(s["standards_at_gate"], 1);
    assert_eq!(
        s["standards_dormant"], 1,
        "exactly the long-adopted untouched rule — a rule adopted TODAY and unused is new, \
         not dead, and flagging it would teach leaders to ignore the signal: {s}"
    );
    assert!(
        s["oldest_gate_secs"].as_i64().expect("secs") > 14 * 24 * 3600,
        "the gate's own SLA must be visible: {s}"
    );

    // The promise the Library made: a dead rule goes red in front of a leader
    // WITHOUT anyone having to open the board and notice.
    let items = report["attention"].as_array().expect("attention");
    let dead_item = items
        .iter()
        .find(|a| {
            a["kind"] == "library"
                && a["headline"]
                    .as_str()
                    .is_some_and(|h| h.contains("nobody has followed"))
        })
        .expect("a dormant rule must raise an attention item");
    assert_eq!(dead_item["severity"], "warning");
    let gate_item = items
        .iter()
        .find(|a| {
            a["kind"] == "library"
                && a["headline"]
                    .as_str()
                    .is_some_and(|h| h.contains("at the gate"))
        })
        .expect("an aging gate queue must raise an attention item");
    assert_eq!(
        gate_item["severity"], "warning",
        "past the SLO the queue is the bottleneck, not a footnote: {gate_item}"
    );

    // The composite is deliberately untouched: four pillars, no fifth. A
    // number leaders track week over week must not be silently redefined the
    // day someone enables the Library.
    let pillars = report["pillars"].as_object().expect("pillars");
    assert_eq!(
        pillars.len(),
        4,
        "the composite must stay four pillars: {pillars:?}"
    );
}

// ── LB4: agent proposals — gated, deduped, rate-limited ─────────────────────

#[tokio::test]
async fn agent_proposals_are_gated_deduped_and_rate_limited() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    // A tight budget so the limit is testable without twenty proposals.
    std::env::set_var("BRAINIAC_LIB_PROPOSE_PER_HOUR", "3");
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let reader = scoped_token(&ctx, ctx.org_a, ctx.user_reader, "read-only", &["lib:read"]).await;
    let proposer = scoped_token(
        &ctx,
        ctx.org_a,
        ctx.user_reader,
        "agent",
        &["lib:read", "lib:propose"],
    )
    .await;

    let propose = |token: String, body: serde_json::Value| {
        let http = http.clone();
        let url = format!("{base}/v1/library/standards/propose");
        async move {
            http.post(url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .expect("propose")
        }
    };

    // A lib:read token cannot propose — the scope is real, not decorative.
    let r = propose(reader.clone(), json!({"name": "x", "statement": "y."})).await;
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    // A proposal is a CANDIDATE, origin=agent — never an adopted rule.
    let r = propose(
        proposer.clone(),
        json!({
            "name": "Idempotent webhook writes",
            "statement": "Every webhook handler writes through an idempotency key.",
            "stack": "rust",
            "rationale": "replayed webhooks double-charged twice this quarter"
        }),
    )
    .await;
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["outcome"], "created");
    assert_eq!(
        body["lifecycle"], "proposed",
        "the gate stays human: {body}"
    );
    let first_id = body["standard_id"].as_str().expect("id").to_string();
    let detail: serde_json::Value = http
        .get(format!("{base}/v1/library/standards/{first_id}"))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("detail")
        .json()
        .await
        .expect("json");
    assert_eq!(
        detail["origin"], "agent",
        "triage must see who is asking: {detail}"
    );

    // The same idea again — collapsed onto the open candidate, no second row.
    let r = propose(
        proposer.clone(),
        json!({"name": "Idempotent Webhook Writes", "statement": "Different words, same name."}),
    )
    .await;
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["outcome"], "duplicate");
    assert_eq!(body["standard_id"].as_str(), Some(first_id.as_str()));
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM standards WHERE org_id = $1 AND slug = 'idempotent-webhook-writes'",
    )
    .bind(ctx.org_a)
    .fetch_one(&ctx.admin)
    .await
    .expect("count");
    assert_eq!(
        rows, 1,
        "ten agents finding the same thing make ONE candidate"
    );

    // A proposal the org already REJECTED collapses onto the rejection —
    // the agent is told, instead of the argument silently reopening.
    {
        let p = Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint,
            team_ids: vec![ctx.team_a],
            project_id: None,
        };
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        assert!(
            library::reject_standard(&mut tx, first_id.parse().expect("uuid"), ctx.user_maint)
                .await
                .expect("reject")
        );
        tx.commit().await.expect("commit");
    }
    let r = propose(
        proposer.clone(),
        json!({"name": "idempotent webhook writes", "statement": "Please reconsider."}),
    )
    .await;
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["outcome"], "duplicate");
    assert_eq!(body["lifecycle"], "rejected", "{body}");

    // Bogus evidence: refused as not-found, nothing created.
    let r = propose(
        proposer.clone(),
        json!({
            "name": "cite your sources",
            "statement": "Rules cite evidence.",
            "evidence_memory_id": Uuid::new_v4()
        }),
    )
    .await;
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);

    // The hour budget: one created so far; two more distinct ideas fit, the
    // fourth is told to wait. Duplicates and refusals never burned budget.
    // (Statements must differ — identical prose IS the dedup key, which an
    // earlier version of this test learned the honest way.)
    for name in ["second idea", "third idea"] {
        let r = propose(
            proposer.clone(),
            json!({"name": name, "statement": format!("The {name}, in one sentence.")}),
        )
        .await;
        assert_eq!(r.status(), reqwest::StatusCode::OK, "{name}");
    }
    let r = propose(
        proposer.clone(),
        json!({"name": "fourth idea", "statement": "One too many."}),
    )
    .await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "the flood gate must close at the configured budget"
    );

    // MCP: the same funnel, the same dedup — an agent proposing over stdio
    // gets the rejection told back, not a fresh candidate.
    let state = Arc::new(McpState {
        store: ctx.store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: Principal {
            org_id: ctx.org_a,
            user_id: ctx.user_maint, // a different identity — fresh budget
            team_ids: vec![ctx.team_a],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        session_remote: None,
    });
    let r = handle_message(
        state.as_ref(),
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "standard_propose", "arguments": {
                "name": "idempotent webhook writes",
                "statement": "Same idea over MCP."
            }}
        }),
    )
    .await
    .expect("mcp propose");
    let payload: serde_json::Value =
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().expect("text"))
            .expect("payload");
    assert_eq!(payload["outcome"], "duplicate");
    assert_eq!(payload["lifecycle"], "rejected");
    assert!(
        payload["note"].as_str().expect("note").contains("respect"),
        "the agent is told to respect the rejection: {payload}"
    );
}

/// F-4/F-5: an agent authors a SKILL over REST (it lands as a draft, never
/// served), the name dedupes, the scope gates it — and F-5's usage-by-slug lets
/// the same agent report use with the slug it holds, not a uuid it never saw.
#[tokio::test]
async fn skill_propose_over_rest_drafts_and_usage_accepts_a_slug() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    // Generous budget: this test exercises drafting/dedup, not the rate limit.
    std::env::set_var("BRAINIAC_LIB_PROPOSE_PER_HOUR", "50");
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let reader = scoped_token(&ctx, ctx.org_a, ctx.user_reader, "read-only", &["lib:read"]).await;
    let proposer = scoped_token(
        &ctx,
        ctx.org_a,
        ctx.user_reader,
        "agent",
        &["lib:read", "lib:propose"],
    )
    .await;
    let skills_propose = format!("{base}/v1/library/skills/propose");
    let body = json!({
        "name": "Add a data provider",
        "instructions_md": "# Add a data provider\n1. implement the trait\n2. register it in the factory\n3. add a paper-only fixture",
        "summary": "wire a new price feed end to end",
        "domain": "providers"
    });

    // A lib:read token cannot propose a skill — same gate as standards.
    let r = http
        .post(&skills_propose)
        .bearer_auth(&reader)
        .json(&body)
        .send()
        .await
        .expect("propose");
    assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

    // The proposer drafts it.
    let r = http
        .post(&skills_propose)
        .bearer_auth(&proposer)
        .json(&body)
        .send()
        .await
        .expect("propose");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let created: serde_json::Value = r.json().await.expect("json");
    assert_eq!(created["outcome"], "created");
    assert_eq!(created["slug"], "add-a-data-provider");
    let skill_id = created["skill_id"].as_str().expect("id").to_string();

    // It is a DRAFT: listed (downloadable:false) but the download refuses it —
    // an agent never receives an unsigned skill as if it were policy.
    let list: serde_json::Value = http
        .get(format!("{base}/v1/library/skills"))
        .bearer_auth(&reader)
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
        .find(|s| s["slug"] == "add-a-data-provider")
        .expect("the proposed skill is listed");
    assert_eq!(mine["maturity"], "draft");
    assert_eq!(mine["downloadable"], false);
    let r = http
        .get(format!(
            "{base}/v1/library/skills/add-a-data-provider/download"
        ))
        .bearer_auth(&reader)
        .send()
        .await
        .expect("download");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a proposed draft is never served"
    );

    // The same name again collapses onto the draft — no duplicate to reconcile.
    let r = http
        .post(&skills_propose)
        .bearer_auth(&proposer)
        .json(&body)
        .send()
        .await
        .expect("propose dup");
    let dup: serde_json::Value = r.json().await.expect("json");
    assert_eq!(dup["outcome"], "duplicate");
    assert_eq!(dup["skill_id"].as_str(), Some(skill_id.as_str()));
    assert_eq!(dup["maturity"], "draft");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM skills WHERE org_id = $1 AND slug = $2")
            .bind(ctx.org_a)
            .bind("add-a-data-provider")
            .fetch_one(&ctx.admin)
            .await
            .expect("count");
    assert_eq!(rows, 1, "no duplicate skill row");

    // F-5: usage reported by SLUG (what the agent holds), not a uuid.
    let r = http
        .post(format!("{base}/v1/library/usage"))
        .bearer_auth(&reader)
        .json(&json!({
            "artifact_kind": "skill",
            "artifact_slug": "add-a-data-provider",
            "event": "apply"
        }))
        .send()
        .await
        .expect("usage by slug");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>().await.expect("json")["recorded"],
        true
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_usage_events WHERE artifact_id = $1 AND event = 'apply'",
    )
    .bind(skill_id.parse::<Uuid>().expect("uuid"))
    .fetch_one(&ctx.admin)
    .await
    .expect("events");
    assert_eq!(
        events, 1,
        "usage-by-slug resolved to the skill and recorded"
    );

    // A slug that resolves to nothing is a clean not-recorded, never a 500.
    let r = http
        .post(format!("{base}/v1/library/usage"))
        .bearer_auth(&reader)
        .json(&json!({
            "artifact_kind": "skill",
            "artifact_slug": "no-such-skill",
            "event": "apply"
        }))
        .send()
        .await
        .expect("usage miss");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>().await.expect("json")["recorded"],
        false
    );

    // Neither id nor slug is a 400, not a silent no-op.
    let r = http
        .post(format!("{base}/v1/library/usage"))
        .bearer_auth(&reader)
        .json(&json!({ "artifact_kind": "skill", "event": "apply" }))
        .send()
        .await
        .expect("usage no target");
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
}

// ── the harness blockers (load/README.md F-1, F-2), as regressions ──────────

#[tokio::test]
async fn mcp_managed_key_resolves_and_its_scopes_gate_the_tools() {
    // F-2: the MCP surface used to resolve ONLY env tokens, so the `brk_`
    // device key that /signup mints "for the local device (the MCP agent)"
    // failed with "does not resolve to a principal" — the onboarding→agent
    // loop was broken end to end. And once it DOES resolve, its scopes must
    // gate the tools, or a read+write device key would get the whole library.
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    // A device key exactly like a coding agent holds: read the memory + KB +
    // library, propose to the library, but never publish, never admin.
    let dev_scopes = ["read", "write", "kb:read", "lib:read", "lib:propose"];
    let secret = scoped_token(&ctx, ctx.org_a, ctx.user_reader, "device", &dev_scopes).await;

    // Resolve the way MCP now does (env map → api_tokens table).
    let tokens = brainiac_server::auth::TokenMap::from_env().expect("token map");
    let resolved = brainiac_server::auth::resolve_bearer(&tokens, &ctx.store, &secret)
        .await
        .expect("resolve")
        .expect("a managed brk_ key must resolve — the onboarding loop depends on it");
    assert_eq!(resolved.principal.user_id, ctx.user_reader);

    let state = Arc::new(McpState {
        store: ctx.store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: resolved.principal,
        scopes: resolved.scopes,
        project_id: None,
        session_remote: None,
    });
    let call = |id: i64, name: &str, args: serde_json::Value| {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": name, "arguments": args } })
    };

    // lib:read is held → standards_for is allowed (empty, but not refused).
    let r = handle_message(
        state.as_ref(),
        &call(1, "standards_for", json!({"stack": "rust"})),
    )
    .await
    .expect("standards_for");
    assert!(
        r.get("error").is_none(),
        "a lib:read key must reach standards_for: {r}"
    );

    // lib:propose is held → a proposal is accepted (the funnel, not the gate).
    let r = handle_message(
        state.as_ref(),
        &call(
            2,
            "standard_propose",
            json!({"name": "device proposal", "statement": "One rule."}),
        ),
    )
    .await
    .expect("propose");
    assert!(
        r.get("error").is_none(),
        "a lib:propose key must reach standard_propose: {r}"
    );

    // Now a key WITHOUT lib:propose — the same agent, minted read-only for the
    // library — must be refused at the tool boundary, not crash.
    let ro = scoped_token(
        &ctx,
        ctx.org_a,
        ctx.user_reader,
        "readonly",
        &["read", "lib:read"],
    )
    .await;
    let ro_ctx = brainiac_server::auth::resolve_bearer(&tokens, &ctx.store, &ro)
        .await
        .expect("resolve")
        .expect("resolve");
    let ro_state = Arc::new(McpState {
        store: ctx.store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: ro_ctx.principal,
        scopes: ro_ctx.scopes,
        project_id: None,
        session_remote: None,
    });
    let r = handle_message(
        ro_state.as_ref(),
        &call(
            3,
            "standard_propose",
            json!({"name": "nope", "statement": "Refused."}),
        ),
    )
    .await
    .expect("propose refused");
    let err = r
        .get("error")
        .expect("a key without lib:propose must be refused");
    assert!(
        err["message"]
            .as_str()
            .expect("msg")
            .contains("lib:propose"),
        "the refusal must name the missing scope: {err}"
    );
    // …and the same read-only key can still READ the library (lib:read held).
    let r = handle_message(ro_state.as_ref(), &call(4, "standards_for", json!({})))
        .await
        .expect("read");
    assert!(
        r.get("error").is_none(),
        "lib:read must still work on the read-only key: {r}"
    );
}

#[tokio::test]
async fn the_token_endpoint_can_mint_every_enforced_scope() {
    // F-1: `auth::SCOPES` — the minter's vocabulary — must contain every scope
    // an endpoint enforces, or the product ships a governed endpoint whose only
    // reachable key is `admin`. This drives the REAL `POST /v1/tokens`
    // validation (the layers' own pg tests mint through the store directly and
    // so never exercised it), and asserts a minted lib:read key actually reaches
    // the library it was minted for.
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await; // tok_maint is a full-authority env token
    let http = reqwest::Client::new();

    // Every scope the layers enforce must be mintable through the endpoint.
    for scope in [
        "read",
        "write",
        "kb:read",
        "kb:publish",
        "lib:read",
        "lib:propose",
        "lib:publish",
    ] {
        let r = http
            .post(format!("{base}/v1/tokens"))
            .bearer_auth("tok_maint")
            .json(&json!({ "name": format!("k-{scope}"), "scopes": [scope] }))
            .send()
            .await
            .expect("mint");
        assert_eq!(
            r.status(),
            reqwest::StatusCode::CREATED,
            "scope `{scope}` is enforced somewhere but the minter rejects it — \
             an endpoint the product governs that no non-admin key can reach"
        );
    }

    // And the minted key WORKS: a lib:read token reaches the library.
    let minted: serde_json::Value = http
        .post(format!("{base}/v1/tokens"))
        .bearer_auth("tok_maint")
        .json(&json!({ "name": "agent-key", "scopes": ["lib:read"], "user_id": ctx.user_reader }))
        .send()
        .await
        .expect("mint")
        .json()
        .await
        .expect("json");
    let secret = minted["token"].as_str().expect("secret");
    let r = http
        .get(format!("{base}/v1/library/standards"))
        .bearer_auth(secret)
        .send()
        .await
        .expect("list");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::OK,
        "a freshly minted lib:read key must reach the library — otherwise the scope is theatre"
    );
}

#[tokio::test]
async fn source_status_returns_the_memory_ids_it_produced() {
    // F-1/F-2 over REST: an agent's memory_add returns a SOURCE id and
    // extraction is async, so it must be able to poll GET /v1/sources/{id} and
    // learn the memory ids produced — the handle it cites as a standard's
    // evidence. Before the fix the endpoint returned counts only.
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    // A source that extraction turned into one org-visible canonical memory,
    // linked the way the pipeline links them (memory -> provenance -> source).
    let (source_id, prov_id, mem_id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    sqlx::query("INSERT INTO sources (id, org_id, kind) VALUES ($1,$2,'manual')")
        .bind(source_id)
        .bind(ctx.org_a)
        .execute(&ctx.admin)
        .await
        .expect("source");
    sqlx::query("INSERT INTO provenance (id, org_id, actor_kind, actor_id, source_id) VALUES ($1,$2,'pipeline','worker:test',$3)")
        .bind(prov_id).bind(ctx.org_a).bind(source_id).execute(&ctx.admin).await.expect("provenance");
    sqlx::query(
        "INSERT INTO memories (id, org_id, visibility, status, kind, content, provenance_id)
         VALUES ($1,$2,'org','canonical','fact','a fact the agent contributed',$3)",
    )
    .bind(mem_id)
    .bind(ctx.org_a)
    .bind(prov_id)
    .execute(&ctx.admin)
    .await
    .expect("memory");

    let r: serde_json::Value = http
        .get(format!("{base}/v1/sources/{source_id}"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("status")
        .json()
        .await
        .expect("json");

    let ids: Vec<&str> = r["results"]["memory_ids"]
        .as_array()
        .expect("memory_ids")
        .iter()
        .map(|v| v.as_str().expect("id"))
        .collect();
    assert_eq!(
        ids,
        vec![mem_id.to_string()],
        "the source must report the memory it produced — the id an agent cites as evidence: {r}"
    );
    assert_eq!(r["results"]["memories"], 1);

    // Another org cannot poll this source — RLS makes it a plain 404.
    let outsider = scoped_token(&ctx, ctx.org_b, ctx.user_b, "rival", &["read"]).await;
    let r = http
        .get(format!("{base}/v1/sources/{source_id}"))
        .bearer_auth(&outsider)
        .send()
        .await
        .expect("cross-org");
    assert_eq!(r.status(), reqwest::StatusCode::NOT_FOUND);
}
