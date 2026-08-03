//! Project isolation at the COMPOSE stage (DATABASE_URL-gated).
//!
//! Migration 0040 gave a project opt-in RLS isolation, and it is real for every
//! caller that reads through `scoped_tx`. The knowledge base is the one place it
//! was not: a TEAM page composes under `worker_tx`, and 0040 clause 1 grants the
//! worker unconditional row visibility ON PURPOSE (the extraction / embedding /
//! contradiction stages have to read back rows they just wrote). So the RLS
//! layer cannot close this path by design, and an isolated client project's
//! memory could be published onto a page whose audience was never entitled to
//! it. `compose::admits` now caps it.
//!
//! Every assertion here is a flip of ONE boolean — `projects.isolated` — with
//! the same org, same page, same memories, same compose tick. That is what makes
//! it a security proof and not a description:
//!
//! - `isolated_project_memory_never_reaches_a_page_outside_it` — non-isolated:
//!   composes (byte-identical to today); isolated: gone from prose AND from the
//!   provenance closure, while the org-shared memory beside it is untouched.
//! - `an_isolated_projects_own_page_still_composes_its_knowledge` — isolation is
//!   a boundary around a project, not a blackout inside it.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{
    DocKind, Lifecycle, MemoryKind, MemoryStatus, SectionBinding, SectionMode, Visibility,
};
use brainiac_gateway::{ChatRequest, MockProvider, ProviderRouter};
use brainiac_pipeline::worker;
use brainiac_store::documents::{NewDocument, NewSection};
use brainiac_store::memories::NewMemory;
use brainiac_store::Store;
use std::sync::Arc;
use uuid::Uuid;

/// The two claims in play. The isolated one is deliberately ORG-visible: the
/// visibility ladder would already stop a team-private memory, so an org-visible
/// row is the only thing that actually tests the project cap.
const SHARED_CLAIM: &str = "webhook retries back off exponentially to 30 seconds";
const CLIENT_CLAIM: &str = "webhook retries for acme are capped at 5 seconds by contract";

/// Cites every memory it was handed and nothing else (mirrors compose_pg.rs) —
/// so anything appearing on the page appeared because compose ADMITTED it.
fn citing_mock() -> MockProvider {
    MockProvider::new(|req: &ChatRequest| {
        let mut out = String::new();
        for line in req.user.lines() {
            let Some(rest) = line.strip_prefix("- id=") else {
                continue;
            };
            let Some((id, tail)) = rest.split_once(' ') else {
                continue;
            };
            let claim = tail.split(":: ").nth(1).unwrap_or("claim");
            out.push_str(&format!("{claim} [m:{id}].\n\n"));
        }
        if out.is_empty() {
            out.push_str("(no knowledge captured yet)");
        }
        out
    })
}

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
    org_id: Uuid,
    team_a: Uuid,
    project: Uuid,
    shared_memory: Uuid,
    client_memory: Uuid,
}

/// One org, one team, one project ("acme"), and two org-visible memories about
/// the same topic — one org-shared, one stamped to the project. The project
/// starts NON-isolated, which is the default every existing org lives in.
async fn setup(url: &str) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE document_reads, document_dependencies, document_revisions, document_sections, documents,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  project_repos, projects,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(url).await.expect("connect");
    let org_id = Uuid::new_v4();
    let team_a = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let project = Uuid::new_v4();
    let shared_memory = Uuid::new_v4();
    let client_memory = Uuid::new_v4();

    let principal = brainiac_pipeline::pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_id, "agency")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team_a, org_id, "platform")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user_id, org_id, "dev@agency.test")
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, team_a, user_id, "maintainer")
        .await
        .expect("member");
    tx.commit().await.expect("commit org");

    // Projects carry no RLS (they are what PRODUCES principals) — admin pool.
    brainiac_store::projects::create(&admin, project, org_id, "acme")
        .await
        .expect("project");

    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    for (id, content, project_id) in [
        (shared_memory, SHARED_CLAIM, None),
        (client_memory, CLIENT_CLAIM, Some(project)),
    ] {
        brainiac_store::memories::insert(
            &mut tx,
            &NewMemory {
                id,
                org_id,
                team_id: Some(team_a),
                owner_user_id: None,
                visibility: Visibility::Org,
                status: MemoryStatus::Canonical,
                kind: MemoryKind::Fact,
                title: None,
                content: content.into(),
                lifecycle: Lifecycle::Shipped,
                detail_md: None,
                language: "en".into(),
                valid_from: None,
                valid_to: None,
                superseded_by: None,
                confidence: Some(0.9),
                provenance_id: None,
                project_id,
            },
        )
        .await
        .expect("memory");
    }
    tx.commit().await.expect("commit memories");

    let ctx = Ctx {
        store,
        admin,
        org_id,
        team_a,
        project,
        shared_memory,
        client_memory,
    };
    embed_memory(&ctx, shared_memory, SHARED_CLAIM).await;
    embed_memory(&ctx, client_memory, CLIENT_CLAIM).await;
    ctx
}

async fn embedding_version(ctx: &Ctx) -> i32 {
    let embedder = DeterministicEmbedder::default();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let version = brainiac_store::memories::ensure_embedding_version(
        &mut tx,
        brainiac_core::embed::Embedder::model_name(&embedder),
        brainiac_core::embed::Embedder::dim(&embedder) as i32,
    )
    .await
    .expect("version");
    tx.commit().await.expect("commit");
    version
}

async fn embed_memory(ctx: &Ctx, id: Uuid, content: &str) {
    let embedder = DeterministicEmbedder::default();
    let version = embedding_version(ctx).await;
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let v = brainiac_core::embed::Embedder::embed(&embedder, content)
        .await
        .expect("embed");
    brainiac_store::memories::upsert_embedding(&mut tx, id, version, &v)
        .await
        .expect("upsert embedding");
    tx.commit().await.expect("commit embed");
}

/// A page with one composed section bound to the shared topic. `visibility`
/// decides which compose path it takes: `Team` → `worker_tx` (the path RLS
/// deliberately cannot cap), `Org` → `scoped_tx`.
async fn page(ctx: &Ctx, slug: &str, visibility: Visibility, project_id: Option<Uuid>) -> Uuid {
    let doc_id = Uuid::new_v4();
    // Worker scope: a team page is invisible to a principal with no membership
    // in that team, and Postgres applies the SELECT policy to the rows an
    // UPDATE reads — so `mark_dirty` under an org-wide principal would silently
    // touch 0 rows and the page would never be composed at all.
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.worker_tx(&principal).await.expect("tx");
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: ctx.org_id,
            team_id: match visibility {
                Visibility::Team => Some(ctx.team_a),
                _ => None,
            },
            slug: slug.into(),
            title: "Webhook retries".into(),
            visibility,
            doc_kind: DocKind::TopicPage,
            project_id,
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
            heading: "What we know".into(),
            mode: SectionMode::Composed,
            binding: Some(SectionBinding {
                query: "webhook retries".into(),
                max_items: 10,
                ..Default::default()
            }),
            pinned_content: None,
        },
    )
    .await
    .expect("section");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit page");
    doc_id
}

async fn mark_dirty(ctx: &Ctx, doc_id: Uuid) {
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.worker_tx(&principal).await.expect("tx");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit dirty");
}

async fn run_compose(ctx: &Ctx) {
    let embedder = DeterministicEmbedder::default();
    let providers = ProviderRouter::single(Arc::new(citing_mock()));
    let version = embedding_version(ctx).await;
    let stats = worker::compose_tick(&ctx.store, &providers, &embedder, version, ctx.org_id, 50)
        .await
        .expect("compose tick");
    assert_eq!(
        stats.composed, 1,
        "the page under test must actually have composed — a silently skipped \
         page would make every assertion below vacuously true"
    );
}

/// (markdown, provenance closure) of the page's newest revision. Read under the
/// WORKER scope because a team page's revisions are invisible to a principal
/// with no membership in that team (`documents_read`, migration 0017) — the
/// assertions below are about what compose ADMITTED, not about who may read the
/// page afterwards.
async fn latest(ctx: &Ctx, doc_id: Uuid) -> (String, Vec<Uuid>) {
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.worker_tx(&principal).await.expect("tx");
    let revs = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
        .await
        .expect("revisions");
    tx.commit().await.expect("commit");
    let rev = revs.into_iter().next().expect("a revision exists");
    (rev.content_md, rev.composed_from)
}

/// THE leak, and its fix, on one page across one boolean flip.
#[tokio::test]
async fn isolated_project_memory_never_reaches_a_page_outside_it() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    // A TEAM page: composed under `worker_tx`, which sees every row in the org
    // by design (0040 clause 1). This is the path RLS cannot save.
    let doc_id = page(&ctx, "webhook-retries", Visibility::Team, None).await;

    // ── Phase 1: project NOT isolated (the default every org lives in) ──
    run_compose(&ctx).await;
    let (md, from) = latest(&ctx, doc_id).await;
    assert!(
        from.contains(&ctx.shared_memory) && from.contains(&ctx.client_memory),
        "default (non-isolated) composition must be unchanged: both claims compose\n{md}"
    );

    // ── Phase 2: flip ONE boolean. Nothing else about the world changes. ──
    let flipped = brainiac_store::projects::set_isolated(&ctx.admin, ctx.org_id, ctx.project, true)
        .await
        .expect("set_isolated");
    assert!(flipped, "the project was flipped to isolated");

    mark_dirty(&ctx, doc_id).await;
    run_compose(&ctx).await;
    let (md, from) = latest(&ctx, doc_id).await;

    // Zero tolerance, exactly as with the team-private leak (EVAL §2.6): not in
    // the prose, not in the provenance closure.
    assert!(
        !from.contains(&ctx.client_memory),
        "an isolated project's memory entered a team page's provenance closure\n{md}"
    );
    assert!(
        !md.contains("acme"),
        "an isolated project's claim was published onto a page outside it\n{md}"
    );
    // …and the isolation cap took ONLY what it was supposed to take.
    assert!(
        from.contains(&ctx.shared_memory) && md.contains("30 seconds"),
        "the org-shared claim beside it must still compose\n{md}"
    );
}

/// Isolation is a boundary around a project, not a blackout inside it: the
/// project's OWN page still composes its own knowledge — on the very same
/// worker path the test above proves is capped.
#[tokio::test]
async fn an_isolated_projects_own_page_still_composes_its_knowledge() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    brainiac_store::projects::set_isolated(&ctx.admin, ctx.org_id, ctx.project, true)
        .await
        .expect("set_isolated");

    let own = page(
        &ctx,
        "acme-webhook-retries",
        Visibility::Team,
        Some(ctx.project),
    )
    .await;
    run_compose(&ctx).await;

    let (md, from) = latest(&ctx, own).await;
    assert!(
        from.contains(&ctx.client_memory) && md.contains("acme"),
        "the isolated project's own page must still see its own claim\n{md}"
    );
    assert!(
        from.contains(&ctx.shared_memory),
        "…plus the org-shared conventions it inherits (the inclusive lens)\n{md}"
    );
}
