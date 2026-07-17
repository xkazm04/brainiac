//! KB3 publishing tests (DATABASE_URL-gated). Each one pins a refusal:
//!
//! - **Nothing publishes without opt-in.** `kb_enabled` is off by default; a
//!   feature that turns itself on inside someone's Confluence is an incident.
//! - **Only org-visible pages leave.** Publishing exits RLS entirely, so a team
//!   page must stay in the console — a leaked team runbook in a company wiki is
//!   an unrecoverable trust event.
//! - **A degrading corpus stops broadcasting.** The health circuit breaker holds
//!   pages at their last published revision instead of pushing stale belief to
//!   the whole company at machine speed. This is the mechanism that turns the
//!   health score from a dashboard into an actuator.
//! - **Publishing is idempotent.** The same revision is never pushed twice.

use brainiac_core::{DocKind, Lifecycle, MemoryKind, MemoryStatus, RevisionPolicy, Visibility};
use brainiac_store::documents::{NewDocument, NewRevision};
use brainiac_store::memories::NewMemory;
use brainiac_store::publishing::{self, PublishTarget};
use brainiac_store::Store;
use uuid::Uuid;

struct Ctx {
    store: Store,
    org_id: Uuid,
    team_id: Uuid,
    /// Where the git target writes.
    out_dir: std::path::PathBuf,
    target_id: Uuid,
}

async fn setup(url: &str, kb_on: bool) -> Ctx {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE document_reads, document_publications, publish_targets, document_dependencies,
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
    let (org_id, team_id, user_id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let out_dir = std::env::temp_dir().join(format!("brainiac-pub-{}", Uuid::new_v4()));
    let target_id = Uuid::new_v4();

    let p = brainiac_pipeline::pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org_id, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team_id, org_id, "payments")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user_id, org_id, "dev@meridian.test")
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, team_id, user_id, "maintainer")
        .await
        .expect("member");
    publishing::set_kb_enabled(&mut tx, org_id, kb_on)
        .await
        .expect("kb flag");
    publishing::insert_target(
        &mut tx,
        &PublishTarget {
            id: target_id,
            org_id,
            kind: "git".into(),
            config: serde_json::json!({
                "repo_path": out_dir.to_string_lossy(),
                "docs_dir": "docs"
            }),
            secret_ref: None,
            enabled: true,
        },
    )
    .await
    .expect("target");
    tx.commit().await.expect("commit");

    Ctx {
        store,
        org_id,
        team_id,
        out_dir,
        target_id,
    }
}

/// A canonical, healthy corpus: nothing deprecated, nothing pending review — so
/// the breaker has no reason to trip.
async fn healthy_corpus(ctx: &Ctx) {
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    for i in 0..10 {
        brainiac_store::memories::insert(
            &mut tx,
            &NewMemory {
                id: Uuid::new_v4(),
                org_id: ctx.org_id,
                team_id: Some(ctx.team_id),
                owner_user_id: None,
                visibility: Visibility::Org,
                status: MemoryStatus::Canonical,
                kind: MemoryKind::Fact,
                title: None,
                content: format!("fact number {i}"),
                lifecycle: Lifecycle::Shipped,
                detail_md: None,
                language: "en".into(),
                valid_from: None,
                valid_to: None,
                superseded_by: None,
                confidence: Some(0.9),
                provenance_id: None,
                project_id: None,
            },
        )
        .await
        .expect("memory");
    }
    tx.commit().await.expect("commit");
}

/// A published page, with the given visibility.
async fn published_page(ctx: &Ctx, slug: &str, visibility: Visibility) -> Uuid {
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    // worker_tx: a TEAM page is not SELECT-able by the pipeline principal under
    // scoped_tx, and Postgres applies the SELECT policy to an UPDATE's WHERE —
    // so the revision's "make this current" update would silently no-op.
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let doc_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: ctx.org_id,
            team_id: Some(ctx.team_id),
            slug: slug.into(),
            title: format!("Page {slug}"),
            visibility,
            doc_kind: DocKind::TopicPage,
            project_id: None,
        },
    )
    .await
    .expect("doc");
    brainiac_store::documents::insert_revision(
        &mut tx,
        &NewRevision {
            id: Uuid::new_v4(),
            document_id: doc_id,
            org_id: ctx.org_id,
            content_md: format!("# Page {slug}\n\nThe cap is 30 seconds.\n"),
            composed_from: vec![],
            trigger: "manual".into(),
            policy_decision: RevisionPolicy::AutoPublished,
            claimed_updated_at: None,
        },
    )
    .await
    .expect("rev");
    tx.commit().await.expect("commit");
    doc_id
}

fn published_file(ctx: &Ctx, slug: &str) -> Option<String> {
    std::fs::read_to_string(ctx.out_dir.join("docs").join(format!("{slug}.md"))).ok()
}

#[tokio::test]
async fn nothing_is_published_until_the_org_opts_in() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url, false).await; // KB layer OFF
    healthy_corpus(&ctx).await;
    published_page(&ctx, "retry-policy", Visibility::Org).await;

    let stats = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");
    assert_eq!(stats.pushed, 0);
    assert!(
        published_file(&ctx, "retry-policy").is_none(),
        "a page was published to an org that never enabled the KB layer"
    );
    let _ = std::fs::remove_dir_all(&ctx.out_dir);
}

#[tokio::test]
async fn only_org_visible_pages_leave_the_building() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url, true).await;
    healthy_corpus(&ctx).await;
    published_page(&ctx, "org-page", Visibility::Org).await;
    published_page(&ctx, "team-page", Visibility::Team).await;

    let stats = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");

    assert_eq!(stats.pushed, 1);
    // NB: `withheld_visibility` is 0, and that is the stronger result. The
    // publish principal is a synthetic user with no team memberships, so RLS
    // never even shows it the team page — the memory layer's own enforcement
    // hides it before brainiac-publish's visibility check gets a chance to run.
    // The code check remains as the second line of defence (a future caller
    // could hold a broader principal), but the first line is the one that would
    // save us, and it is the same RLS path every user query takes.
    assert_eq!(stats.withheld_visibility, 0);
    let org = published_file(&ctx, "org-page").expect("the org page must publish");
    assert!(org.contains("do not edit here"), "banner missing:\n{org}");
    assert!(
        published_file(&ctx, "team-page").is_none(),
        "a TEAM page was published outside RLS — this is the unrecoverable case"
    );
    let _ = std::fs::remove_dir_all(&ctx.out_dir);
}

#[tokio::test]
async fn a_rotting_corpus_trips_the_breaker_and_pages_hold() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url, true).await;
    healthy_corpus(&ctx).await;
    published_page(&ctx, "retry-policy", Visibility::Org).await;

    // A healthy org publishes.
    let stats = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");
    assert_eq!(stats.pushed, 1);
    assert!(published_file(&ctx, "retry-policy").is_some());

    // Now the corpus rots: most of what we believe has been deprecated and
    // nobody has re-verified it. Currency collapses.
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    sqlx::query("UPDATE memories SET status = 'deprecated' WHERE org_id = $1")
        .bind(ctx.org_id)
        .execute(&mut *tx)
        .await
        .expect("rot");
    let gate = publishing::publish_gate(&mut tx, ctx.org_id)
        .await
        .expect("gate");
    tx.commit().await.expect("commit");

    assert!(
        gate.blocked.is_some(),
        "currency {} should have tripped the breaker",
        gate.currency
    );

    // A NEW revision lands — and must NOT reach the wiki, because the corpus it
    // came from is no longer fit to broadcast.
    let doc_id = {
        let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
        let d = brainiac_store::documents::get_document_by_slug(&mut tx, "retry-policy")
            .await
            .expect("q")
            .expect("doc");
        brainiac_store::documents::insert_revision(
            &mut tx,
            &NewRevision {
                id: Uuid::new_v4(),
                document_id: d.id,
                org_id: ctx.org_id,
                content_md: "# Page retry-policy\n\nBRAND NEW CLAIM nobody verified.\n".into(),
                composed_from: vec![],
                trigger: "memory_change".into(),
                policy_decision: RevisionPolicy::AutoPublished,
                claimed_updated_at: None,
            },
        )
        .await
        .expect("rev2");
        tx.commit().await.expect("commit");
        d.id
    };
    assert!(!doc_id.is_nil());

    let stats = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");
    assert_eq!(stats.pushed, 0, "a degraded corpus kept broadcasting");
    assert!(stats.blocked > 0);

    let live = published_file(&ctx, "retry-policy").expect("the old page must still be there");
    assert!(
        !live.contains("BRAND NEW CLAIM"),
        "the breaker did not hold the page:\n{live}"
    );
    assert!(
        live.contains("The cap is 30 seconds"),
        "the page must HOLD its last published revision, not be deleted"
    );
    let _ = std::fs::remove_dir_all(&ctx.out_dir);
}

#[tokio::test]
async fn publishing_the_same_revision_twice_is_a_no_op() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url, true).await;
    healthy_corpus(&ctx).await;
    let doc_id = published_page(&ctx, "retry-policy", Visibility::Org).await;

    let first = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");
    assert_eq!(first.pushed, 1);

    let second = brainiac_publish::publish_org(&ctx.store, ctx.org_id, "https://console.test")
        .await
        .expect("publish");
    assert_eq!(second.pushed, 0, "the same revision was pushed twice");
    assert_eq!(second.unchanged, 1);

    // The ledger records what is live where — the thing that lets an operator
    // prove what left the building.
    let p = brainiac_pipeline::pipeline_principal(ctx.org_id);
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let pubs = publishing::publication(&mut tx, doc_id, ctx.target_id)
        .await
        .expect("publication")
        .expect("recorded");
    tx.commit().await.expect("commit");
    assert!(pubs.external_ref.is_some());
    let _ = std::fs::remove_dir_all(&ctx.out_dir);
}
