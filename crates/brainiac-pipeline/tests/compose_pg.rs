//! Compose-stage integration tests (DATABASE_URL-gated) — the document layer's
//! load-bearing promises, exercised against a real Postgres with RLS on.
//!
//! Each test here corresponds to a claim the product makes in public:
//!
//! - **The wiki cannot rot.** Resolve a contradiction → the page that cited the
//!   losing claim is marked dirty and recomposes onto the winner. Nobody edits
//!   anything. (`contradiction_resolution_propagates_to_the_page`)
//! - **A page cannot leak.** A team-private memory must not reach an org page —
//!   enforced by composing as a principal that literally cannot read it, so the
//!   test would fail even if the model tried to smuggle it in. (`org_page_cannot_see_team_private_memory`)
//! - **Human prose is never touched.** A pinned section survives regeneration
//!   byte-identically. (`pinned_sections_survive_regeneration_byte_identical`)
//! - **Nothing publishes itself into existence.** A page's first revision needs
//!   a human; an additive recompose of an already-published page does not.
//!   (`first_revision_needs_a_human_then_additive_recompose_auto_publishes`)
//! - **Artifacts are copied, not retold.** `detail_md` reaches the page verbatim.
//!   (`detail_md_reaches_the_page_verbatim`)

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{
    DocKind, Lifecycle, MemoryKind, MemoryStatus, RevisionPolicy, SectionBinding, SectionMode,
    Visibility,
};
use brainiac_gateway::{ChatRequest, MockProvider, ProviderRouter};
use brainiac_pipeline::worker;
use brainiac_store::documents::{NewDocument, NewRevision, NewSection};
use brainiac_store::memories::NewMemory;
use brainiac_store::Store;
use std::sync::Arc;
use uuid::Uuid;

/// A composer that behaves like a well-behaved model: it cites every memory it
/// was handed, and nothing else. Quality/hallucination behaviour of a REAL model
/// is the `docs` eval profile's job (EVAL §2.6); these tests pin the plumbing and
/// the firewalls, which must hold regardless of what the model does.
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
    org_id: Uuid,
    team_a: Uuid,
    team_b: Uuid,
    user_id: Uuid,
}

/// A minimal org: two teams, one user who belongs to team A only.
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
    let org_id = Uuid::new_v4();
    let team_a = Uuid::new_v4();
    let team_b = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let principal = brainiac_pipeline::pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_id, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team_a, org_id, "payments")
        .await
        .expect("team a");
    brainiac_store::orgs::upsert_team(&mut tx, team_b, org_id, "platform")
        .await
        .expect("team b");
    brainiac_store::orgs::upsert_user(&mut tx, user_id, org_id, "dev@meridian.test")
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, team_a, user_id, "maintainer")
        .await
        .expect("member");
    tx.commit().await.expect("commit setup");

    Ctx {
        store,
        org_id,
        team_a,
        team_b,
        user_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn memory(
    id: Uuid,
    org_id: Uuid,
    team_id: Uuid,
    visibility: Visibility,
    content: &str,
    detail_md: Option<&str>,
) -> NewMemory {
    NewMemory {
        id,
        org_id,
        team_id: Some(team_id),
        owner_user_id: None,
        visibility,
        status: MemoryStatus::Canonical,
        kind: MemoryKind::Fact,
        title: None,
        content: content.into(),
        lifecycle: Lifecycle::Shipped,
        detail_md: detail_md.map(|d| d.into()),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: Some(0.9),
        provenance_id: None,
        project_id: None,
    }
}

/// An org page with one composed section bound to a free-text query.
async fn org_page(ctx: &Ctx, slug: &str, query: &str) -> Uuid {
    let doc_id = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: ctx.org_id,
            team_id: None,
            slug: slug.into(),
            title: "Retry policy".into(),
            visibility: Visibility::Org,
            doc_kind: DocKind::TopicPage,
            project_id: None,
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
                query: query.into(),
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

async fn run_compose(ctx: &Ctx) -> worker::ComposeStats {
    let embedder = DeterministicEmbedder::default();
    let providers = ProviderRouter::single(Arc::new(citing_mock()));
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

    worker::compose_tick(&ctx.store, &providers, &embedder, version, ctx.org_id, 50)
        .await
        .expect("compose tick")
}

/// Embed a memory so the retrieval binding can actually find it.
async fn embed_memory(ctx: &Ctx, id: Uuid, content: &str) {
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
    let v = brainiac_core::embed::Embedder::embed(&embedder, content)
        .await
        .expect("embed");
    brainiac_store::memories::upsert_embedding(&mut tx, id, version, &v)
        .await
        .expect("upsert embedding");
    tx.commit().await.expect("commit embed");
}

async fn current_md(ctx: &Ctx, doc_id: Uuid) -> Option<String> {
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let rev = brainiac_store::documents::current_revision(&mut tx, doc_id)
        .await
        .expect("rev");
    tx.commit().await.expect("commit");
    rev.map(|r| r.content_md)
}

async fn latest_revision(ctx: &Ctx, doc_id: Uuid) -> brainiac_core::DocumentRevision {
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let revs = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
        .await
        .expect("revisions");
    tx.commit().await.expect("commit");
    revs.into_iter().next().expect("a revision exists")
}

#[tokio::test]
async fn org_page_cannot_see_team_private_memory() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    // Two memories about the same topic: one the org shares, one team-private.
    let shared = Uuid::new_v4();
    let secret = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            shared,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            None,
        ),
    )
    .await
    .expect("shared");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            secret,
            ctx.org_id,
            ctx.team_b,
            Visibility::Team,
            "retry cap breach paged us at 3am after the psp incident",
            None,
        ),
    )
    .await
    .expect("secret");
    tx.commit().await.expect("commit memories");

    embed_memory(&ctx, shared, "retry cap is 30 seconds with jitter").await;
    embed_memory(
        &ctx,
        secret,
        "retry cap breach paged us at 3am after the psp incident",
    )
    .await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;
    run_compose(&ctx).await;

    let rev = latest_revision(&ctx, doc_id).await;
    // Zero tolerance (EVAL §2.6): the private memory must appear nowhere — not
    // in the prose, not in the provenance closure.
    assert!(
        !rev.content_md.contains("3am"),
        "team-private content leaked into an org page:\n{}",
        rev.content_md
    );
    assert!(
        !rev.composed_from.contains(&secret),
        "team-private memory entered an org page's provenance closure"
    );
    assert!(
        rev.composed_from.contains(&shared),
        "the org-visible memory should have composed"
    );
}

#[tokio::test]
async fn contradiction_resolution_propagates_to_the_page() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    let old = Uuid::new_v4();
    let new = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            old,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 2 seconds",
            None,
        ),
    )
    .await
    .expect("old");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, old, "retry cap is 2 seconds").await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;
    run_compose(&ctx).await;

    // A human publishes the first revision (nothing auto-publishes into being).
    let first = latest_revision(&ctx, doc_id).await;
    assert_eq!(first.policy_decision, RevisionPolicy::NeedsReview);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::documents::approve_revision(&mut tx, first.id, ctx.user_id, chrono::Utc::now())
        .await
        .expect("approve");
    tx.commit().await.expect("commit approve");

    let published = current_md(&ctx, doc_id).await.expect("published");
    assert!(published.contains("2 seconds"), "{published}");

    // The org learns better and a maintainer resolves the contradiction. NOBODY
    // TOUCHES THE PAGE.
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            new,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            None,
        ),
    )
    .await
    .expect("new");
    let applied = brainiac_store::governance::apply_supersession(
        &mut tx,
        ctx.org_id,
        old,
        new,
        Some(ctx.user_id),
        "contradiction-resolved",
    )
    .await
    .expect("supersede");
    assert!(applied);
    tx.commit().await.expect("commit supersede");
    embed_memory(&ctx, new, "retry cap is 30 seconds with jitter").await;

    // The supersession marked the page dirty by itself — that is the whole
    // claim. Recompose and the page now states the org's current belief.
    let stats = run_compose(&ctx).await;
    assert_eq!(
        stats.composed, 1,
        "the resolved contradiction must have marked the page dirty"
    );

    let after = latest_revision(&ctx, doc_id).await;
    assert!(
        after.content_md.contains("30 seconds"),
        "page did not pick up the winning claim:\n{}",
        after.content_md
    );
    assert!(
        !after.content_md.contains("2 seconds"),
        "page is still serving the superseded belief:\n{}",
        after.content_md
    );
    assert!(!after.composed_from.contains(&old));
}

#[tokio::test]
async fn pinned_sections_survive_regeneration_byte_identical() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    const PINNED: &str = "Owned by payments. Ping #pay-oncall before changing anything here.";

    let m = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            m,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            None,
        ),
    )
    .await
    .expect("m");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, m, "retry cap is 30 seconds with jitter").await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::documents::insert_section(
        &mut tx,
        &NewSection {
            id: Uuid::new_v4(),
            document_id: doc_id,
            org_id: ctx.org_id,
            position: 1,
            heading: "Ownership".into(),
            mode: SectionMode::Pinned,
            binding: None,
            pinned_content: Some(PINNED.into()),
        },
    )
    .await
    .expect("pinned");
    tx.commit().await.expect("commit");

    run_compose(&ctx).await;
    let first = latest_revision(&ctx, doc_id).await;
    assert!(first.content_md.contains(PINNED));

    // Regenerate (twice) — the human's prose must come through untouched.
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit");
    run_compose(&ctx).await;

    let second = latest_revision(&ctx, doc_id).await;
    assert!(
        second.content_md.contains(PINNED),
        "regeneration mangled human-owned prose:\n{}",
        second.content_md
    );
}

#[tokio::test]
async fn first_revision_needs_a_human_then_additive_recompose_auto_publishes() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    let a = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            a,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            None,
        ),
    )
    .await
    .expect("a");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, a, "retry cap is 30 seconds with jitter").await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;
    run_compose(&ctx).await;

    // Nothing publishes itself into existence.
    let first = latest_revision(&ctx, doc_id).await;
    assert_eq!(first.policy_decision, RevisionPolicy::NeedsReview);
    assert!(current_md(&ctx, doc_id).await.is_none());

    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::documents::approve_revision(&mut tx, first.id, ctx.user_id, chrono::Utc::now())
        .await
        .expect("approve");
    tx.commit().await.expect("commit");

    // Now the org learns something ADDITIONAL. No published claim is lost, so
    // the page may update itself — this is the flywheel the product promises.
    let b = Uuid::new_v4();
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            b,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap applies to psp-gateway consumers too",
            None,
        ),
    )
    .await
    .expect("b");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, b, "retry cap applies to psp-gateway consumers too").await;

    let stats = run_compose(&ctx).await;
    assert_eq!(
        stats.auto_published, 1,
        "an additive recompose must publish"
    );

    let live = current_md(&ctx, doc_id).await.expect("published");
    assert!(live.contains("psp-gateway"), "{live}");
}

#[tokio::test]
async fn detail_md_reaches_the_page_verbatim() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    const ARTIFACT: &str = "```yaml\nretry:\n  max_backoff: 30s\n  jitter: full\n```";

    let m = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            m,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            Some(ARTIFACT),
        ),
    )
    .await
    .expect("m");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, m, "retry cap is 30 seconds with jitter").await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;
    run_compose(&ctx).await;

    let rev = latest_revision(&ctx, doc_id).await;
    // The config the team actually merged, on the page, character for character
    // — copied by us, never re-typed by a model (KB-PLAN D3).
    assert!(
        rev.content_md.contains("max_backoff: 30s"),
        "the artifact never reached the page:\n{}",
        rev.content_md
    );
    assert!(rev.content_md.contains("jitter: full"));
}

/// A hostile model: it cites a memory it was never given. The firewall must
/// strip the fake citation and refuse to auto-publish — otherwise an invented
/// claim would look sourced, which is worse than an obviously unsourced one.
#[tokio::test]
async fn an_inventing_model_cannot_auto_publish() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;

    let m = Uuid::new_v4();
    let principal = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    brainiac_store::memories::insert(
        &mut tx,
        &memory(
            m,
            ctx.org_id,
            ctx.team_a,
            Visibility::Org,
            "retry cap is 30 seconds with jitter",
            None,
        ),
    )
    .await
    .expect("m");
    tx.commit().await.expect("commit");
    embed_memory(&ctx, m, "retry cap is 30 seconds with jitter").await;

    let doc_id = org_page(&ctx, "retry-policy", "retry cap").await;

    // Publish a clean first revision by hand so the page has a baseline (an
    // additive recompose would otherwise be eligible to auto-publish).
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let rev_id = Uuid::new_v4();
    brainiac_store::documents::insert_revision(
        &mut tx,
        &NewRevision {
            id: rev_id,
            document_id: doc_id,
            org_id: ctx.org_id,
            content_md: format!("# Retry policy\n\nretry cap is 30 seconds [m:{m}].\n"),
            composed_from: vec![m],
            trigger: "manual".into(),
            policy_decision: RevisionPolicy::AutoPublished,
            claimed_updated_at: None,
        },
    )
    .await
    .expect("baseline revision");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("dirty");
    tx.commit().await.expect("commit");

    // Now compose with a model that fabricates a citation.
    let ghost = Uuid::new_v4();
    let embedder = DeterministicEmbedder::default();
    let providers = ProviderRouter::single(Arc::new(MockProvider::new(move |_req| {
        format!("The retry cap is 30 seconds [m:{ghost}].\n")
    })));
    let mut tx = ctx.store.scoped_tx(&principal).await.expect("tx");
    let version = brainiac_store::memories::ensure_embedding_version(
        &mut tx,
        brainiac_core::embed::Embedder::model_name(&embedder),
        brainiac_core::embed::Embedder::dim(&embedder) as i32,
    )
    .await
    .expect("version");
    tx.commit().await.expect("commit");

    worker::compose_tick(&ctx.store, &providers, &embedder, version, ctx.org_id, 50)
        .await
        .expect("compose");

    let rev = latest_revision(&ctx, doc_id).await;
    assert_eq!(
        rev.policy_decision,
        RevisionPolicy::NeedsReview,
        "a fabricated citation must never auto-publish"
    );
    assert!(
        !rev.content_md.contains(&ghost.to_string()),
        "the fabricated citation survived into the page:\n{}",
        rev.content_md
    );
}
