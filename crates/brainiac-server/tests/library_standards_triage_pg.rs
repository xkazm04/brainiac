//! Triage that can NARROW a rule, not just judge it (DATABASE_URL-gated).
//!
//! Mining and the ratification bridge file every candidate under
//! `stack = general`, and until now the gate offered exactly three verbs —
//! adopt, reject, deprecate. So a maintainer who could see that a candidate
//! belonged to `rust` had no way to say so, the per-stack standards pages the
//! design promises could only ever grow one bucket, and a comment in the
//! bridge described a "fix it during triage" mechanism that did not exist.
//!
//! The claims under test:
//!
//! - **A candidate can be re-filed before the gate**, and the stack it lands
//!   in is the one whose page then composes from it — the point of narrowing.
//! - **A re-word is a REVISION, never an in-place mutation.** The rule's
//!   version history shows rev N+1, and the standards page that publishes the
//!   old wording is dirty-marked so it recomposes. This is the risky one: a
//!   naive `UPDATE standards SET statement = …` would leave the history
//!   silent and the wiki publishing text the org no longer holds.
//! - **The edit is the maintainer's**, gated on `lib:publish` like every other
//!   verb at the gate, and refused outright on a terminal rule.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{Enforcement, Principal, StandardProvenanceKind};
use brainiac_store::{library, Store};
use serde_json::json;
use uuid::Uuid;

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
    org: Uuid,
    team: Uuid,
    maintainer: Uuid,
    reader: Uuid,
}

impl Ctx {
    fn principal(&self) -> Principal {
        Principal {
            org_id: self.org,
            user_id: self.maintainer,
            team_ids: vec![self.team],
            project_id: None,
        }
    }
}

async fn setup(url: &str) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences, api_tokens,
                  document_revisions, document_sections, documents,
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
    let (maintainer, reader) = (Uuid::new_v4(), Uuid::new_v4());

    let p = brainiac_pipeline::pipeline_principal(org);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team, org, "platform")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, maintainer, org, "lead@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_user(&mut tx, reader, org, "agent@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_member(&mut tx, team, maintainer, "maintainer")
        .await
        .expect("m");
    brainiac_store::orgs::upsert_member(&mut tx, team, reader, "member")
        .await
        .expect("m");
    tx.commit().await.expect("commit");

    Ctx {
        store,
        admin,
        org,
        team,
        maintainer,
        reader,
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
        "tok_maint": {"org": ctx.org, "user": ctx.maintainer, "teams": [ctx.team]},
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

/// A candidate exactly as mining and the bridge leave one: `general`/`practice`,
/// evidence attached so the gate is about filing, not about attribution.
async fn mined_candidate(ctx: &Ctx, slug: &str, statement: &str) -> Uuid {
    let id = Uuid::new_v4();
    let p = ctx.principal();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    library::insert_standard(
        &mut tx,
        &library::NewStandard {
            id,
            org_id: ctx.org,
            origin: brainiac_core::StandardOrigin::Sweep,
            stack: "general".into(),
            category: "practice".into(),
            slug: slug.into(),
            statement: statement.into(),
            rationale: Some("mined from what the org already does".into()),
            detail_md: None,
            enforcement: Enforcement::Recommended,
            provenance: vec![(StandardProvenanceKind::Memory, Uuid::new_v4())],
            author: Some(ctx.maintainer),
        },
    )
    .await
    .expect("candidate");
    tx.commit().await.expect("commit");
    id
}

/// The board ranks by the pressure a rule is under, and the pressure has to
/// come from the shelf itself — one list call, no N+1 detail fetches and, above
/// all, NO NEW TELEMETRY. Every number here is already being written by the
/// serve path; the board simply stopped throwing it away.
#[tokio::test]
async fn the_shelf_carries_the_pulse_and_the_dormancy_of_every_rule() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    // Three rules that tell the three stories the ordering exists for.
    let followed = mined_candidate(&ctx, "already-followed", "The org already does this.").await;
    let live = mined_candidate(&ctx, "live-rule", "Used every week.").await;
    let quiet = mined_candidate(&ctx, "quiet-rule", "Adopted, then forgotten.").await;

    {
        let p = ctx.principal();
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        for id in [live, quiet] {
            assert!(library::adopt_standard(&mut tx, id, ctx.maintainer, false)
                .await
                .expect("adopt"));
        }
        // `followed` stays PROPOSED — that is the point: the org is already
        // reaching for a rule nobody has ratified.
        for (id, n) in [(followed, 3), (live, 2)] {
            for _ in 0..n {
                library::record_usage(
                    &mut tx,
                    ctx.org,
                    brainiac_core::LibraryArtifactKind::Standard,
                    id,
                    None,
                    brainiac_core::LibraryUsageEvent::Check,
                    Some(ctx.team),
                )
                .await
                .expect("usage");
            }
        }
        tx.commit().await.expect("commit");
    }
    // The quiet rule has been adopted long enough to have been used.
    sqlx::query("UPDATE standards SET adopted_at = now() - interval '40 days' WHERE id = $1")
        .bind(quiet)
        .execute(&ctx.admin)
        .await
        .expect("age");

    let list: serde_json::Value = http
        .get(format!("{base}/v1/library/standards?lifecycle=all"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    let by_slug = |slug: &str| -> serde_json::Value {
        list["standards"]
            .as_array()
            .expect("arr")
            .iter()
            .find(|s| s["slug"] == slug)
            .unwrap_or_else(|| panic!("missing {slug}: {list}"))
            .clone()
    };

    let f = by_slug("already-followed");
    assert_eq!(f["uses"], 3, "the pulse must travel with the shelf: {f}");
    assert_eq!(
        f["dormant"], false,
        "a proposal cannot be dormant — nobody adopted it to ignore: {f}"
    );
    assert!(f["last_used_at"].is_string(), "{f}");

    let l = by_slug("live-rule");
    assert_eq!(l["uses"], 2);
    assert_eq!(l["dormant"], false, "an adopted rule in use is alive: {l}");

    let q = by_slug("quiet-rule");
    assert_eq!(q["uses"], 0);
    assert_eq!(
        q["dormant"], true,
        "an adopted rule old enough to have been used, and untouched, is the \
         retirement shortlist — the board must say so without anyone opening the \
         health report: {q}"
    );
    assert!(q["last_used_at"].is_null(), "{q}");

    // The pressure ordering is only honest if the signal is the SAME one
    // Knowledge Health already computes — one dormancy rule, not two.
    let health: serde_json::Value = http
        .get(format!("{base}/v1/analytics/knowledge-health"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("health")
        .json()
        .await
        .expect("json");
    assert_eq!(
        health["signals"]["standards_dormant"], 1,
        "the board and the leadership report must not disagree about which \
         rules are dead: {}",
        health["signals"]
    );

    // …and nothing here invented a telemetry channel: the only event kinds in
    // the table are the ones the serve path has always written.
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT event FROM library_usage_events ORDER BY event")
            .fetch_all(&ctx.admin)
            .await
            .expect("events");
    assert_eq!(kinds, vec!["check".to_string()], "no new event type");
}

#[tokio::test]
async fn triage_narrows_a_mined_candidate_into_a_stack_whose_page_composes_from_it() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let reader = scoped_token(&ctx, ctx.reader, "agent", &["lib:read"]).await;
    let id = mined_candidate(
        &ctx,
        "handlers-map-errors",
        "Request handlers never unwrap; they map errors to typed responses.",
    )
    .await;

    let patch = |token: String, body: serde_json::Value| {
        let http = http.clone();
        let url = format!("{base}/v1/library/standards/{id}");
        async move {
            http.patch(url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .expect("patch")
        }
    };

    // The edit is a maintainer verb: a token minted to READ the library must
    // not be able to re-file the org's rules.
    let r = patch(reader.clone(), json!({"stack": "rust"})).await;
    assert_eq!(
        r.status(),
        reqwest::StatusCode::FORBIDDEN,
        "narrowing a rule is a lib:publish act, not a lib:read one"
    );

    // The maintainer narrows it. The filing tokens are normalised the way a
    // slug is, so "Rust" and "rust" can never become two shelves for one stack.
    let r = patch(
        "tok_maint".to_string(),
        json!({"stack": "Rust", "category": "Errors"}),
    )
    .await;
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["stack"], "rust", "{body}");
    assert_eq!(body["category"], "errors", "{body}");
    assert_eq!(
        body["lifecycle"], "proposed",
        "narrowing is not adopting — the human gate is untouched: {body}"
    );
    assert!(
        body["rev"].is_null(),
        "re-filing is not a revision of what the org committed to: {body}"
    );
    let revs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM standard_versions WHERE standard_id = $1")
            .bind(id)
            .fetch_one(&ctx.admin)
            .await
            .expect("count");
    assert_eq!(revs, 1, "moving a shelf must not mint a version");

    // Adopt it, then ask the questions the whole direction exists for.
    let r = http
        .post(format!("{base}/v1/library/standards/{id}/adopt"))
        .bearer_auth("tok_maint")
        .json(&json!({}))
        .send()
        .await
        .expect("adopt");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    // 1. An agent asking for `rust` is served it — not `general`.
    let list = |stack: &str| {
        let http = http.clone();
        let token = reader.clone();
        let url = format!("{base}/v1/library/standards?stack={stack}");
        async move {
            http.get(url)
                .bearer_auth(token)
                .send()
                .await
                .expect("list")
                .json::<serde_json::Value>()
                .await
                .expect("json")
        }
    };
    let rust = list("rust").await;
    assert_eq!(
        rust["standards"]
            .as_array()
            .expect("arr")
            .iter()
            .map(|s| s["slug"].as_str().expect("slug"))
            .collect::<Vec<_>>(),
        vec!["handlers-map-errors"],
        "the narrowed rule must serve under its real stack: {rust}"
    );
    let general = list("general").await;
    assert_eq!(
        general["standards"].as_array().expect("arr").len(),
        0,
        "…and must have LEFT the general bucket, not been copied out of it: {general}"
    );

    // 2. The stack-specific PAGE composes from it — the promise the scaffold
    //    made and the `general`-only pipeline could never keep.
    let p = ctx.principal();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let page = brainiac_pipeline::standards_page::render_stack(&mut tx, "rust")
        .await
        .expect("render");
    tx.commit().await.expect("commit");
    assert!(
        page.markdown.contains("Request handlers never unwrap"),
        "a non-`general` standards page must pick up the narrowed rule: {}",
        page.markdown
    );
    assert!(
        page.markdown.contains("## errors"),
        "…filed under the category triage chose: {}",
        page.markdown
    );
}

#[tokio::test]
async fn a_reword_writes_a_revision_and_makes_the_page_follow() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let base = boot_http(&ctx).await;
    let http = reqwest::Client::new();

    let id = mined_candidate(&ctx, "retry-with-jitter", "Retry with backoff.").await;

    // Narrow it and adopt it, so the rule is live on a stack page.
    let r = http
        .patch(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth("tok_maint")
        .json(&json!({"stack": "rust", "category": "resilience"}))
        .send()
        .await
        .expect("narrow");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let r = http
        .post(format!("{base}/v1/library/standards/{id}/adopt"))
        .bearer_auth("tok_maint")
        .json(&json!({}))
        .send()
        .await
        .expect("adopt");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    // The page that publishes the rust rules, freshly composed (not dirty).
    let page_id = Uuid::new_v4();
    {
        let p = ctx.principal();
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        brainiac_store::documents::insert_document(
            &mut tx,
            &brainiac_store::documents::NewDocument {
                id: page_id,
                org_id: ctx.org,
                team_id: None,
                slug: "standards-rust".into(),
                title: "rust standards".into(),
                visibility: brainiac_core::Visibility::Org,
                doc_kind: brainiac_core::DocKind::StandardsPage,
                project_id: None,
            },
        )
        .await
        .expect("page");
        tx.commit().await.expect("commit");
    }
    sqlx::query("UPDATE documents SET dirty_at = NULL WHERE id = $1")
        .bind(page_id)
        .execute(&ctx.admin)
        .await
        .expect("clean the page");

    // ── the re-word ─────────────────────────────────────────────────────
    let r = http
        .patch(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth("tok_maint")
        .json(&json!({
            "statement": "Retry with exponential backoff and full jitter, capped at 30s.",
            "enforcement": "mandatory"
        }))
        .send()
        .await
        .expect("reword");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(
        body["rev"], 2,
        "a re-word is a numbered revision, not an edit: {body}"
    );
    assert_eq!(body["enforcement"], "mandatory", "{body}");

    // The HISTORY shows it. This is the assertion that separates a revision
    // from a silent in-place mutation: rev 1 still says what the org adopted,
    // rev 2 says what it holds now.
    let detail: serde_json::Value = http
        .get(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("detail")
        .json()
        .await
        .expect("json");
    let versions = detail["versions"].as_array().expect("versions");
    assert_eq!(versions.len(), 2, "the re-word must be visible: {detail}");
    assert_eq!(versions[0]["rev"], 2);
    assert!(versions[0]["statement"]
        .as_str()
        .expect("statement")
        .contains("full jitter"));
    assert_eq!(versions[1]["rev"], 1);
    assert_eq!(
        versions[1]["statement"], "Retry with backoff.",
        "the wording the org actually ratified must survive the edit: {detail}"
    );
    assert_eq!(
        detail["statement"], "Retry with exponential backoff and full jitter, capped at 30s.",
        "{detail}"
    );

    // …and the PAGE follows: the wiki must not keep publishing text the org
    // no longer holds, and nobody should have to remember to go fix it.
    let dirty: bool =
        sqlx::query_scalar("SELECT dirty_at IS NOT NULL FROM documents WHERE id = $1")
            .bind(page_id)
            .fetch_one(&ctx.admin)
            .await
            .expect("dirty_at");
    assert!(
        dirty,
        "re-wording an adopted rule must dirty-mark its standards page — otherwise the \
         page publishes a sentence the org has already replaced"
    );

    // A re-file across stacks marks BOTH pages: the one it left keeps
    // publishing the rule otherwise.
    let ts_page = Uuid::new_v4();
    {
        let p = ctx.principal();
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        brainiac_store::documents::insert_document(
            &mut tx,
            &brainiac_store::documents::NewDocument {
                id: ts_page,
                org_id: ctx.org,
                team_id: None,
                slug: "standards-typescript".into(),
                title: "typescript standards".into(),
                visibility: brainiac_core::Visibility::Org,
                doc_kind: brainiac_core::DocKind::StandardsPage,
                project_id: None,
            },
        )
        .await
        .expect("page");
        tx.commit().await.expect("commit");
    }
    sqlx::query("UPDATE documents SET dirty_at = NULL WHERE id IN ($1, $2)")
        .bind(page_id)
        .bind(ts_page)
        .execute(&ctx.admin)
        .await
        .expect("clean");
    let r = http
        .patch(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth("tok_maint")
        .json(&json!({"stack": "typescript"}))
        .send()
        .await
        .expect("refile");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let marked: Vec<(String, bool)> = sqlx::query_as(
        "SELECT slug, dirty_at IS NOT NULL FROM documents WHERE id IN ($1, $2) ORDER BY slug",
    )
    .bind(page_id)
    .bind(ts_page)
    .fetch_all(&ctx.admin)
    .await
    .expect("pages");
    assert_eq!(marked.len(), 2, "both pages must be present: {marked:?}");
    for (slug, dirty_at) in &marked {
        assert!(
            *dirty_at,
            "moving an adopted rule between stacks must mark both pages; `{slug}` was left stale"
        );
    }

    // ── terminal rules are not editable ─────────────────────────────────
    let r = http
        .post(format!("{base}/v1/library/standards/{id}/deprecate"))
        .bearer_auth("tok_maint")
        .send()
        .await
        .expect("deprecate");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let r = http
        .patch(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth("tok_maint")
        .json(&json!({"statement": "Quietly bring it back."}))
        .send()
        .await
        .expect("edit retired");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::CONFLICT,
        "retirement is one-way; re-wording a retired rule would be editing history"
    );

    // Another org's rule does not exist to edit — 404, never 403.
    let (other_org, other_user) = (Uuid::new_v4(), Uuid::new_v4());
    {
        let p = brainiac_pipeline::pipeline_principal(other_org);
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        brainiac_store::orgs::upsert_org(&mut tx, other_org, "rival")
            .await
            .expect("org");
        brainiac_store::orgs::upsert_user(&mut tx, other_user, other_org, "x@rival.test")
            .await
            .expect("u");
        tx.commit().await.expect("commit");
    }
    let (secret, prefix) = brainiac_server::auth::mint_secret();
    let hash = brainiac_server::auth::hash_token(&secret);
    brainiac_store::tokens::create(
        &ctx.admin,
        Uuid::new_v4(),
        other_org,
        other_user,
        "rival",
        &prefix,
        &hash,
        &["lib:read".to_string(), "lib:publish".to_string()],
        None,
        other_user,
    )
    .await
    .expect("token");
    let r = http
        .patch(format!("{base}/v1/library/standards/{id}"))
        .bearer_auth(&secret)
        .json(&json!({"stack": "rust"}))
        .send()
        .await
        .expect("cross-org edit");
    assert_eq!(
        r.status(),
        reqwest::StatusCode::NOT_FOUND,
        "existence is itself information"
    );
}
