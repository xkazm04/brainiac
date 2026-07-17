//! Integration tests for the wiki browse/paginate/facet queries
//! (`brainiac_store::documents::{browse, browse_count, browse_facets}`).
//! Require a live Postgres; setup() TRUNCATES, so use a THROWAWAY DATABASE_URL.

use brainiac_core::{DocKind, RevisionPolicy, Visibility};
use brainiac_store::documents::{self, DocFilter, NewDocument, NewRevision};
use brainiac_store::{orgs, Store};
use uuid::Uuid;

fn url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}
fn uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}
fn org() -> Uuid {
    uuid(1)
}
fn principal() -> brainiac_core::Principal {
    brainiac_core::Principal {
        org_id: org(),
        user_id: uuid(11),
        team_ids: vec![uuid(21)],
    }
}

async fn setup() -> Option<(Store, sqlx::PgPool, tokio::sync::MutexGuard<'static, ()>)> {
    let url = url()?;
    let guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE documents, document_revisions, document_sections, document_dependencies,
                  document_reads, team_members, users, teams, orgs CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");
    let store = Store::connect(&url).await.expect("store");
    Some((store, admin, guard))
}

/// Insert a page (org-visible so RLS never hides it here) with one revision,
/// then set its status/dirty via admin SQL (test convenience).
async fn page(
    store: &Store,
    admin: &sqlx::PgPool,
    id: u8,
    slug: &str,
    kind: DocKind,
    status: &str,
    dirty: bool,
    review: bool,
) {
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");
    documents::insert_document(
        &mut tx,
        &NewDocument {
            id: uuid(id),
            org_id: org(),
            team_id: Some(uuid(21)),
            slug: slug.into(),
            title: format!("page {slug}"),
            visibility: Visibility::Org,
            doc_kind: kind,
            project_id: None,
        },
    )
    .await
    .expect("doc");
    documents::insert_revision(
        &mut tx,
        &NewRevision {
            id: uuid(id.wrapping_add(100)),
            document_id: uuid(id),
            org_id: org(),
            content_md: format!("# {slug}\n\nbody of {slug}\n"),
            composed_from: vec![],
            trigger: "manual".into(),
            policy_decision: if review {
                RevisionPolicy::NeedsReview
            } else {
                RevisionPolicy::AutoPublished
            },
            claimed_updated_at: None,
        },
    )
    .await
    .expect("rev");
    tx.commit().await.expect("commit");

    sqlx::query("UPDATE documents SET status=$2, dirty_at = CASE WHEN $3 THEN now() ELSE NULL END WHERE id=$1")
        .bind(uuid(id))
        .bind(status)
        .bind(dirty)
        .execute(admin)
        .await
        .expect("status/dirty");
}

async fn seed(store: &Store, admin: &sqlx::PgPool) {
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");
    orgs::upsert_org(&mut tx, org(), "docs-test").await.unwrap();
    orgs::upsert_team(&mut tx, uuid(21), org(), "payments").await.unwrap();
    orgs::upsert_user(&mut tx, uuid(11), org(), "u@x").await.unwrap();
    orgs::upsert_member(&mut tx, uuid(21), uuid(11), "member").await.unwrap();
    tx.commit().await.unwrap();

    use DocKind::*;
    // space "payments": 3 pages (one dirty, one awaiting review)
    page(store, admin, 31, "payments/psp-gateway", EntityPage, "published", false, false).await;
    page(store, admin, 32, "payments/refund-runbook", Runbook, "published", true, false).await;
    page(store, admin, 33, "payments/adr-001", TopicPage, "draft", false, true).await;
    // space "core": 2 pages
    page(store, admin, 34, "core/ledger", EntityPage, "published", false, false).await;
    page(store, admin, 35, "core/posting-runbook", Runbook, "published", false, false).await;
}

#[tokio::test]
async fn browse_pages_a_space_and_count_agrees() {
    let Some((store, admin, _g)) = setup().await else { return };
    seed(&store, &admin).await;
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");

    let all = DocFilter::default();
    assert_eq!(documents::browse_count(&mut tx, &all).await.unwrap(), 5);

    let pay = DocFilter { space: Some("payments"), ..Default::default() };
    let rows = documents::browse(&mut tx, &pay, 50, 0).await.unwrap();
    let total = documents::browse_count(&mut tx, &pay).await.unwrap();
    assert_eq!(total, 3, "three pages in the payments space");
    assert_eq!(rows.len() as i64, total, "count agrees with the page");
    assert!(rows.iter().all(|p| p.doc.slug.starts_with("payments/")));

    // paging: limit 2 then offset 2 covers the space with no overlap.
    let p1 = documents::browse(&mut tx, &pay, 2, 0).await.unwrap();
    let p2 = documents::browse(&mut tx, &pay, 2, 2).await.unwrap();
    assert_eq!(p1.len(), 2);
    assert_eq!(p2.len(), 1);
    for a in &p1 {
        assert!(!p2.iter().any(|b| b.doc.id == a.doc.id));
    }
}

#[tokio::test]
async fn facets_build_the_space_directory_and_cross_filter() {
    let Some((store, admin, _g)) = setup().await else { return };
    seed(&store, &admin).await;
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");

    // Unfiltered: the space directory is the tree — payments 3, core 2.
    let f = documents::browse_facets(&mut tx, &DocFilter::default()).await.unwrap();
    let pay = f.spaces.iter().find(|s| s.value == "payments").unwrap();
    let core = f.spaces.iter().find(|s| s.value == "core").unwrap();
    assert_eq!(pay.count, 3);
    assert_eq!(core.count, 2);
    // The two tab counts over the whole corpus.
    assert_eq!(f.needs_review, 1, "one page awaiting review");
    assert_eq!(f.dirty, 1, "one page behind the corpus");

    // Filter to the payments space: the KIND facet shrinks to payments' 3 kinds,
    // but the SPACE facet still lists both spaces (a dimension never constrains
    // its own menu — the tree stays browsable).
    let pf = DocFilter { space: Some("payments"), ..Default::default() };
    let f2 = documents::browse_facets(&mut tx, &pf).await.unwrap();
    assert_eq!(f2.kinds.iter().map(|k| k.count).sum::<i64>(), 3, "kinds reflect the space filter");
    assert!(f2.spaces.iter().any(|s| s.value == "core"), "space facet ignores its own filter");
    assert_eq!(f2.needs_review, 1, "the review page is in payments");
}

#[tokio::test]
async fn pending_review_flag_and_filter() {
    let Some((store, admin, _g)) = setup().await else { return };
    seed(&store, &admin).await;
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");

    // The needs_review filter returns exactly the awaiting-review page, with its
    // flag set; every other page's flag is false.
    let rf = DocFilter { needs_review: Some(true), ..Default::default() };
    let rows = documents::browse(&mut tx, &rf, 50, 0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].doc.slug, "payments/adr-001");
    assert!(rows[0].pending_review);

    let all = documents::browse(&mut tx, &DocFilter::default(), 50, 0).await.unwrap();
    let flagged = all.iter().filter(|p| p.pending_review).count();
    assert_eq!(flagged, 1, "exactly one page flags as awaiting review");
}
