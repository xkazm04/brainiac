//! KB2 read-surface tests (DATABASE_URL-gated): the REST docs API, the MCP doc
//! tools, and entity-page auto-scaffolding.
//!
//! The three claims under test are the ones a reader has to be able to trust:
//!
//! - **A page you may not see does not exist.** RLS scopes the reader exactly
//!   like `memory_search`; a team page is invisible to a non-member, and the API
//!   says "not found" rather than "forbidden" — existence is itself information.
//! - **An agent can read pages but never write one.** There is no doc_write
//!   tool, and an unpublished page serves no content to an agent: a draft nobody
//!   signed must not reach a coding agent through the back door.
//! - **The wiki grows where the knowledge is.** Auto-scaffolding creates an
//!   entity page only for a canonical entity carrying org-visible knowledge from
//!   ≥2 teams — and creates it as a DRAFT, because the machine decides that a
//!   page should exist while a human decides that it is right.

use brainiac_core::{DocKind, Lifecycle, MemoryKind, MemoryStatus, SectionMode, Visibility};
use brainiac_store::documents::{NewDocument, NewRevision, NewSection};
use brainiac_store::memories::NewMemory;
use brainiac_store::Store;
use uuid::Uuid;

struct Ctx {
    store: Store,
    org_id: Uuid,
    team_a: Uuid,
    team_b: Uuid,
    /// Member (maintainer) of team A only.
    user_a: Uuid,
    /// Member of team B only — the outsider whose view we test.
    user_b: Uuid,
}

async fn setup(url: &str) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE document_reads, document_dependencies, document_revisions, document_sections, documents,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(url).await.expect("connect");
    let (org_id, team_a, team_b) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let (user_a, user_b) = (Uuid::new_v4(), Uuid::new_v4());

    let p = brainiac_pipeline::pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_id, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team_a, org_id, "payments")
        .await
        .expect("t");
    brainiac_store::orgs::upsert_team(&mut tx, team_b, org_id, "platform")
        .await
        .expect("t");
    brainiac_store::orgs::upsert_user(&mut tx, user_a, org_id, "a@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_user(&mut tx, user_b, org_id, "b@meridian.test")
        .await
        .expect("u");
    brainiac_store::orgs::upsert_member(&mut tx, team_a, user_a, "maintainer")
        .await
        .expect("m");
    brainiac_store::orgs::upsert_member(&mut tx, team_b, user_b, "member")
        .await
        .expect("m");
    tx.commit().await.expect("commit");

    Ctx {
        store,
        org_id,
        team_a,
        team_b,
        user_a,
        user_b,
    }
}

fn principal(org: Uuid, user: Uuid) -> brainiac_core::Principal {
    brainiac_core::Principal {
        org_id: org,
        user_id: user,
        team_ids: vec![],
    }
}

/// A published page owned by team A, visible to team A only.
///
/// NB: seeded through `worker_tx`, not `scoped_tx`. A TEAM document is only
/// SELECT-able by a member of that team (or the worker scope), and Postgres
/// applies the SELECT policy to an UPDATE's `WHERE` clause — so writing a team
/// page as the pipeline principal through `scoped_tx` silently updates ZERO rows
/// instead of erroring. Production is safe (compose_tick opens `worker_tx` for
/// team pages precisely because of this), but it is a sharp edge: an RLS
/// no-op looks exactly like success.
async fn team_page(ctx: &Ctx) -> Uuid {
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let doc_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: ctx.org_id,
            team_id: Some(ctx.team_a),
            slug: "payments-runbook".into(),
            title: "Payments runbook".into(),
            visibility: Visibility::Team,
            doc_kind: DocKind::Runbook,
        },
    )
    .await
    .expect("doc");
    brainiac_store::documents::insert_section(
        &mut tx,
        &NewSection {
            id: Uuid::new_v4(),
            document_id: doc_id,
            org_id: ctx.org_id,
            position: 0,
            heading: "Ownership".into(),
            mode: SectionMode::Pinned,
            binding: None,
            pinned_content: Some("Owned by payments.".into()),
        },
    )
    .await
    .expect("section");
    let rev_id = Uuid::new_v4();
    brainiac_store::documents::insert_revision(
        &mut tx,
        &NewRevision {
            id: rev_id,
            document_id: doc_id,
            org_id: ctx.org_id,
            content_md: "# Payments runbook\n\nOwned by payments.\n".into(),
            composed_from: vec![],
            trigger: "manual".into(),
            policy_decision: brainiac_core::RevisionPolicy::AutoPublished,
            claimed_updated_at: None,
        },
    )
    .await
    .expect("rev");
    tx.commit().await.expect("commit");
    doc_id
}

#[tokio::test]
async fn a_team_page_is_invisible_to_a_non_member() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    team_page(&ctx).await;

    // The owner sees it.
    let mut tx = ctx
        .store
        .scoped_tx(&principal(ctx.org_id, ctx.user_a))
        .await
        .expect("tx");
    let mine = brainiac_store::documents::get_document_by_slug(&mut tx, "payments-runbook")
        .await
        .expect("q");
    let list_mine = brainiac_store::documents::list_documents(&mut tx)
        .await
        .expect("list");
    tx.commit().await.expect("c");
    assert!(mine.is_some(), "the owning team must see its own page");
    assert_eq!(list_mine.len(), 1);

    // The outsider does not — and gets "not found", not "forbidden".
    let mut tx = ctx
        .store
        .scoped_tx(&principal(ctx.org_id, ctx.user_b))
        .await
        .expect("tx");
    let theirs = brainiac_store::documents::get_document_by_slug(&mut tx, "payments-runbook")
        .await
        .expect("q");
    let list_theirs = brainiac_store::documents::list_documents(&mut tx)
        .await
        .expect("list");
    tx.commit().await.expect("c");
    assert!(
        theirs.is_none(),
        "a non-member could read another team's page"
    );
    assert!(list_theirs.is_empty());
}

#[tokio::test]
async fn mcp_doc_get_serves_published_pages_and_refuses_unsigned_drafts() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    team_page(&ctx).await;

    // A DRAFT page: composed, never signed. (worker_tx — see team_page.)
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let draft_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: draft_id,
            org_id: ctx.org_id,
            team_id: Some(ctx.team_a),
            slug: "draft-page".into(),
            title: "Draft".into(),
            visibility: Visibility::Team,
            doc_kind: DocKind::TopicPage,
        },
    )
    .await
    .expect("doc");
    brainiac_store::documents::insert_revision(
        &mut tx,
        &NewRevision {
            id: Uuid::new_v4(),
            document_id: draft_id,
            org_id: ctx.org_id,
            content_md: "# Draft\n\nSomething nobody signed [m:x].\n".into(),
            composed_from: vec![],
            trigger: "memory_change".into(),
            policy_decision: brainiac_core::RevisionPolicy::NeedsReview,
            claimed_updated_at: None,
        },
    )
    .await
    .expect("rev");
    tx.commit().await.expect("commit");

    let state = mcp_state(&ctx, ctx.user_a).await;

    // Published: the agent gets the markdown.
    let out = call_doc_tool(
        &state,
        "doc_get",
        serde_json::json!({"slug": "payments-runbook"}),
    )
    .await;
    assert_eq!(out["found"], true);
    assert_eq!(out["published"], true);
    assert!(out["content_md"]
        .as_str()
        .expect("content")
        .contains("Payments runbook"));

    // Draft: the agent is told it exists and is unpublished — and gets NO
    // content. A draft nobody signed must not reach an agent through the back
    // door; that would defeat the review gate as surely as a doc_write tool.
    let out = call_doc_tool(&state, "doc_get", serde_json::json!({"slug": "draft-page"})).await;
    assert_eq!(out["found"], true);
    assert_eq!(out["published"], false);
    assert!(
        out.get("content_md").is_none(),
        "an unsigned draft leaked its content to an agent: {out}"
    );

    // doc_search only ever offers published pages.
    let out = call_doc_tool(
        &state,
        "doc_search",
        serde_json::json!({"query": "payments"}),
    )
    .await;
    let slugs: Vec<&str> = out["pages"]
        .as_array()
        .expect("pages")
        .iter()
        .map(|p| p["slug"].as_str().unwrap_or_default())
        .collect();
    assert!(slugs.contains(&"payments-runbook"));
    assert!(
        !slugs.contains(&"draft-page"),
        "search offered an unsigned draft"
    );

    // A page the operator cannot see is simply not found.
    let outsider = mcp_state(&ctx, ctx.user_b).await;
    let out = call_doc_tool(
        &outsider,
        "doc_get",
        serde_json::json!({"slug": "payments-runbook"}),
    )
    .await;
    assert_eq!(out["found"], false);

    // ── read analytics (0025) ────────────────────────────────────────────
    // Exactly ONE read was recorded in all of the above: the published page
    // served to the agent. The unsigned draft served no content, and the
    // outsider's not-found served nothing — neither is a read.
    let mut tx = ctx
        .store
        .scoped_tx(&principal(ctx.org_id, ctx.user_a))
        .await
        .expect("tx");
    let reads: Vec<(String, bool)> =
        sqlx::query_as("SELECT via, was_dirty FROM document_reads ORDER BY read_at")
            .fetch_all(&mut *tx)
            .await
            .expect("reads");
    tx.commit().await.expect("commit");
    assert_eq!(
        reads,
        vec![("mcp".to_string(), false)],
        "one agent read of the published page; drafts and not-founds record nothing"
    );
}

async fn mcp_state(ctx: &Ctx, user: Uuid) -> std::sync::Arc<brainiac_server::mcp::McpState> {
    use brainiac_core::embed::{DeterministicEmbedder, Embedder};
    let embedder = DeterministicEmbedder::default();
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let version = brainiac_store::memories::ensure_embedding_version(
        &mut tx,
        embedder.model_name(),
        embedder.dim() as i32,
    )
    .await
    .expect("version");
    tx.commit().await.expect("commit");
    std::sync::Arc::new(brainiac_server::mcp::McpState {
        store: ctx.store.clone(),
        embedder: std::sync::Arc::new(DeterministicEmbedder::default()),
        embedding_version: version,
        principal: principal(ctx.org_id, user),
    })
}

/// Drive a tool through the real JSON-RPC entry point (not a private helper), so
/// the test exercises the same path an agent does — including the tool-result
/// envelope.
async fn call_doc_tool(
    state: &std::sync::Arc<brainiac_server::mcp::McpState>,
    tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    });
    let r = brainiac_server::mcp::handle_message(state, &req)
        .await
        .expect("response");
    assert_eq!(r["result"]["isError"], false, "tool {tool} errored: {r}");
    let text = r["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text");
    serde_json::from_str(text).expect("tool payload is json")
}

#[tokio::test]
async fn entity_pages_scaffold_only_where_knowledge_actually_crosses_teams() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");

    // Two canonical entities. `kafka` accumulates org knowledge from BOTH teams
    // (it earns a page). `sidecar` gets plenty of knowledge from ONE team only
    // (it does not — that is team business, not an org-wide page).
    let mk_entity = |name: &str, team: Uuid| (Uuid::new_v4(), name.to_string(), team);
    let kafka_raw_a = mk_entity("kafka", ctx.team_a);
    let kafka_raw_b = mk_entity("Kafka", ctx.team_b);
    let sidecar_raw = mk_entity("sidecar", ctx.team_a);

    for (id, name, team) in [&kafka_raw_a, &kafka_raw_b, &sidecar_raw] {
        brainiac_store::entities::insert_entity(
            &mut tx,
            *id,
            ctx.org_id,
            Some(*team),
            name,
            "tech",
            &[],
            None,
        )
        .await
        .expect("entity");
    }

    let kafka_canon = Uuid::new_v4();
    let sidecar_canon = Uuid::new_v4();
    brainiac_store::entities::insert_canonical(&mut tx, kafka_canon, ctx.org_id, "kafka", "tech")
        .await
        .expect("canon");
    brainiac_store::entities::insert_canonical(
        &mut tx,
        sidecar_canon,
        ctx.org_id,
        "sidecar",
        "tech",
    )
    .await
    .expect("canon");
    for (raw, canon) in [
        (kafka_raw_a.0, kafka_canon),
        (kafka_raw_b.0, kafka_canon),
        (sidecar_raw.0, sidecar_canon),
    ] {
        brainiac_store::entities::link(&mut tx, raw, canon, 1.0, "test", None)
            .await
            .expect("link");
    }

    // 2 org memories per team on kafka (4 total, 2 teams) → earns a page.
    // 5 org memories on sidecar, all team A (1 team) → does not.
    let mem = |content: &str, team: Uuid, entity: Uuid| {
        let id = Uuid::new_v4();
        (id, content.to_string(), team, entity)
    };
    let rows = vec![
        mem("kafka is the event bus", ctx.team_a, kafka_raw_a.0),
        mem("kafka retention is 7 days", ctx.team_a, kafka_raw_a.0),
        mem(
            "kafka MSK is the managed cluster",
            ctx.team_b,
            kafka_raw_b.0,
        ),
        mem(
            "kafka topics are named by domain",
            ctx.team_b,
            kafka_raw_b.0,
        ),
        mem("sidecar injects tracing", ctx.team_a, sidecar_raw.0),
        mem("sidecar has a 50ms overhead", ctx.team_a, sidecar_raw.0),
        mem("sidecar is opt-in", ctx.team_a, sidecar_raw.0),
        mem("sidecar version is pinned", ctx.team_a, sidecar_raw.0),
        mem("sidecar logs to stdout", ctx.team_a, sidecar_raw.0),
    ];
    for (id, content, team, entity) in &rows {
        brainiac_store::memories::insert(
            &mut tx,
            &NewMemory {
                id: *id,
                org_id: ctx.org_id,
                team_id: Some(*team),
                owner_user_id: None,
                visibility: Visibility::Org,
                status: MemoryStatus::Canonical,
                kind: MemoryKind::Fact,
                title: None,
                content: content.clone(),
                lifecycle: Lifecycle::Shipped,
                detail_md: None,
                language: "en".into(),
                valid_from: None,
                valid_to: None,
                superseded_by: None,
                confidence: Some(0.9),
                provenance_id: None,
            },
        )
        .await
        .expect("memory");
        brainiac_store::memories::link_entity(&mut tx, *id, *entity)
            .await
            .expect("anchor");
    }
    tx.commit().await.expect("commit");

    // Scaffold under worker authority (it must see every team's memories).
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let created = brainiac_pipeline::compose::scaffold_entity_pages(&mut tx, ctx.org_id, 10)
        .await
        .expect("scaffold");
    tx.commit().await.expect("commit");

    assert_eq!(
        created.len(),
        1,
        "exactly one entity (kafka) should have earned a page"
    );

    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let doc = brainiac_store::documents::get_document(&mut tx, created[0])
        .await
        .expect("q")
        .expect("doc");
    let sections = brainiac_store::documents::sections(&mut tx, doc.id)
        .await
        .expect("sections");
    tx.commit().await.expect("c");

    assert_eq!(doc.title, "kafka");
    assert_eq!(doc.doc_kind, DocKind::EntityPage);
    assert_eq!(doc.visibility, Visibility::Org);
    // A scaffolded page is a DRAFT: the machine decided a page should exist; a
    // human still decides it is right.
    assert_eq!(doc.status, brainiac_core::DocStatus::Draft);
    assert!(doc.dirty_at.is_some(), "a new page must compose");
    // The lifecycle split is structural, not editorial: "how it works" can never
    // quietly absorb "how we intend it to work".
    assert!(sections
        .iter()
        .any(|s| s.heading == "On its way (not yet shipped)"));

    // Idempotent: a second sweep must not duplicate the page.
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let again = brainiac_pipeline::compose::scaffold_entity_pages(&mut tx, ctx.org_id, 10)
        .await
        .expect("scaffold");
    tx.commit().await.expect("commit");
    assert!(again.is_empty(), "scaffolding duplicated an existing page");

    // ── the deterministic neighborhood diagram (D9 rung a) ────────────────
    // Composed with no edges in the graph: NO diagram. An empty mermaid block
    // would be decoration, and diagrams here are language, not decoration.
    use brainiac_core::embed::{DeterministicEmbedder, Embedder};
    let embedder = DeterministicEmbedder::default();
    let providers = brainiac_gateway::ProviderRouter::single(std::sync::Arc::new(
        brainiac_gateway::MockProvider::new(|_| "The event bus carries settlements.".to_string()),
    ));
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let version =
        brainiac_store::memories::ensure_embedding_version(&mut tx, embedder.model_name(), 8)
            .await
            .expect("version");
    tx.commit().await.expect("commit");
    let stats = brainiac_pipeline::worker::compose_tick(
        &ctx.store, &providers, &embedder, version, ctx.org_id, 10,
    )
    .await
    .expect("compose");
    assert_eq!(stats.composed, 1);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let rev = brainiac_store::documents::revisions(&mut tx, doc.id, 1)
        .await
        .expect("revs")
        .into_iter()
        .next()
        .expect("revision");
    tx.commit().await.expect("commit");
    assert!(
        !rev.content_md.contains("## Neighborhood"),
        "an entity with no edges must not get an empty diagram:\n{}",
        rev.content_md
    );

    // The graph learns an edge (kafka feeds sidecar, asserted by a memory) —
    // the next compose renders it, compiled from the DB row, no model involved.
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    brainiac_store::entities::insert_edge(
        &mut tx,
        Uuid::new_v4(),
        ctx.org_id,
        kafka_raw_a.0,
        sidecar_raw.0,
        "feeds",
        Some(rows[0].0),
    )
    .await
    .expect("edge");
    brainiac_store::documents::mark_dirty(&mut tx, doc.id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit");
    let stats = brainiac_pipeline::worker::compose_tick(
        &ctx.store, &providers, &embedder, version, ctx.org_id, 10,
    )
    .await
    .expect("compose");
    assert_eq!(stats.composed, 1);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let rev = brainiac_store::documents::revisions(&mut tx, doc.id, 1)
        .await
        .expect("revs")
        .into_iter()
        .next()
        .expect("revision");
    tx.commit().await.expect("commit");
    assert!(
        rev.content_md.contains("## Neighborhood") && rev.content_md.contains("```mermaid"),
        "the edge must render as a diagram:\n{}",
        rev.content_md
    );
    assert!(
        rev.content_md.contains("-->|feeds|") && rev.content_md.contains("[\"sidecar\"]"),
        "the diagram must show the relation and the neighbor:\n{}",
        rev.content_md
    );
}
