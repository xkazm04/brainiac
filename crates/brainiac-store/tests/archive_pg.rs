//! Integration tests for the archive browse/paginate/facet queries
//! (`brainiac_store::archive`). Require a live Postgres.
//!
//! setup() TRUNCATES the tenant, so point DATABASE_URL at a THROWAWAY database,
//! never the dev corpus.
//!
//! What must hold here:
//! - `count` agrees with `list` under the same filter (the badge can't lie).
//! - facets are cross-filtered: filtering status shrinks the KIND facet but not
//!   the STATUS facet (a dimension never constrains its own menu).
//! - as_of returns rows valid THEN, including since-deprecated ones.
//! - the validity skeleton spans the whole timeline (ignores as_of).
//! - full-text `q` matches content and title.
//! - every read inherits RLS (a data principal never sees payments-only rows).

use brainiac_core::{MemoryKind, MemoryStatus, Principal, Visibility};
use brainiac_store::archive::{self, MemoryFilter, MemorySort};
use brainiac_store::{memories, orgs, Store};
use chrono::{Duration, Utc};
use uuid::Uuid;

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}
fn uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}
fn org() -> Uuid {
    uuid(1)
}
fn pay() -> Principal {
    Principal {
        org_id: org(),
        user_id: uuid(11),
        team_ids: vec![uuid(21)],
        project_id: None,
    }
}
fn data() -> Principal {
    Principal {
        org_id: org(),
        user_id: uuid(12),
        team_ids: vec![uuid(22)],
        project_id: None,
    }
}

async fn setup() -> Option<(Store, tokio::sync::MutexGuard<'static, ()>)> {
    let url = database_url()?;
    let guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");
    let store = Store::connect(&url).await.expect("store");
    Some((store, guard))
}

#[allow(clippy::too_many_arguments)]
fn mk(
    id: u8,
    team: u8,
    vis: Visibility,
    status: MemoryStatus,
    kind: MemoryKind,
    title: Option<&str>,
    content: &str,
    valid_from_days: Option<i64>,
    valid_to_days: Option<i64>,
) -> memories::NewMemory {
    let now = Utc::now();
    memories::NewMemory {
        id: uuid(id),
        org_id: org(),
        team_id: Some(uuid(team)),
        owner_user_id: None,
        visibility: vis,
        status,
        kind,
        title: title.map(|s| s.to_string()),
        lifecycle: Default::default(),
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: valid_from_days.map(|d| now + Duration::days(d)),
        valid_to: valid_to_days.map(|d| now + Duration::days(d)),
        superseded_by: None,
        confidence: None,
        provenance_id: None,
        project_id: None,
    }
}

async fn seed(store: &Store) {
    let mut tx = store.scoped_tx(&pay()).await.expect("tx");
    let c = &mut *tx;
    orgs::upsert_org(c, org(), "arch-test").await.unwrap();
    orgs::upsert_team(c, uuid(21), org(), "payments").await.unwrap();
    orgs::upsert_team(c, uuid(22), org(), "data").await.unwrap();
    orgs::upsert_user(c, uuid(11), org(), "pay@x").await.unwrap();
    orgs::upsert_user(c, uuid(12), org(), "data@x").await.unwrap();
    orgs::upsert_member(c, uuid(21), uuid(11), "member").await.unwrap();
    orgs::upsert_member(c, uuid(22), uuid(12), "member").await.unwrap();

    use MemoryKind::*;
    use MemoryStatus::*;
    use Visibility::*;
    // payments, org-visible: a decision + a fact + a deprecated one that was
    // valid only in the past window.
    for m in [
        mk(101, 21, Org, Canonical, Decision, Some("psp retry cap"), "retry cap of 2s for psp-gateway", Some(-100), Some(100)),
        // payments TEAM-visible — the data principal must NOT see this one.
        mk(102, 21, Team, Canonical, Fact, Some("ledger authority"), "the ledger holds the authoritative balance", None, None),
        mk(103, 21, Org, Deprecated, Fact, Some("old timeout"), "psp timeout was 10 seconds", Some(-200), Some(-30)),
        mk(104, 21, Org, Candidate, Pitfall, None, "chargeback and refund race condition", Some(-10), Some(300)),
        // data team-visible — a payments principal must NOT see this.
        mk(105, 22, Team, Canonical, Howto, Some("feature store"), "rebuild the feature store nightly", None, None),
    ] {
        memories::insert(c, &m).await.expect("insert");
    }
    tx.commit().await.expect("commit");
}

async fn run(store: &Store, p: &Principal, f: &MemoryFilter) -> (Vec<archive::MemoryListRow>, i64) {
    let mut tx = store.scoped_tx(p).await.expect("tx");
    let rows = archive::list(&mut tx, f, MemorySort::Recent, 50, 0).await.expect("list");
    let total = archive::count(&mut tx, f).await.expect("count");
    (rows, total)
}

#[tokio::test]
async fn count_agrees_with_list_and_rls_scopes() {
    let Some((store, _g)) = setup().await else { return };
    seed(&store).await;

    // Payments sees its own team + all org rows (101,102,103,104); the data
    // team's row (105) is hidden.
    let (rows, total) = run(&store, &pay(), &MemoryFilter::default()).await;
    assert_eq!(total, 4, "payments sees its team + org rows");
    assert_eq!(rows.len() as i64, total, "count agrees with list");
    assert!(!rows.iter().any(|r| r.id == uuid(105)), "no data-team leak");

    // The data principal sees the org rows (101,103,104) + its own team row
    // (105), but NOT the payments TEAM-visible row (102).
    let (drows, dtotal) = run(&store, &data(), &MemoryFilter::default()).await;
    assert_eq!(dtotal, 4, "data sees org rows + own team");
    assert!(drows.iter().any(|r| r.id == uuid(105)), "sees own team row");
    assert!(!drows.iter().any(|r| r.id == uuid(102)), "no payments-team leak");
}

#[tokio::test]
async fn filter_and_fulltext_narrow_total() {
    let Some((store, _g)) = setup().await else { return };
    seed(&store).await;

    let decisions = MemoryFilter { kind: Some("decision".into()), ..Default::default() };
    let (_, total) = run(&store, &pay(), &decisions).await;
    assert_eq!(total, 1, "one payments decision");

    // FTS over content.
    let q_ledger = MemoryFilter { q: Some("ledger".into()), ..Default::default() };
    let (rows, total) = run(&store, &pay(), &q_ledger).await;
    assert_eq!(total, 1);
    assert_eq!(rows[0].id, uuid(102));

    // Title substring match (content does not contain "authority").
    let q_title = MemoryFilter { q: Some("authority".into()), ..Default::default() };
    let (_, total) = run(&store, &pay(), &q_title).await;
    assert_eq!(total, 1, "matched the title");
}

#[tokio::test]
async fn facets_are_cross_filtered() {
    let Some((store, _g)) = setup().await else { return };
    seed(&store).await;
    let mut tx = store.scoped_tx(&pay()).await.expect("tx");

    // Unfiltered: kind facet covers all four payments rows.
    let all = archive::facets(&mut tx, &MemoryFilter::default()).await.expect("facets");
    let kind_total: i64 = all.kinds.iter().map(|f| f.count).sum();
    assert_eq!(kind_total, 4);
    assert_eq!(all.statuses.iter().map(|f| f.count).sum::<i64>(), 4);

    // Filter status=canonical: the KIND facet shrinks to the 2 canonical rows...
    let canon = MemoryFilter { status: Some("canonical".into()), ..Default::default() };
    let f = archive::facets(&mut tx, &canon).await.expect("facets");
    assert_eq!(f.kinds.iter().map(|x| x.count).sum::<i64>(), 2, "kind facet reflects status filter");
    // ...but the STATUS facet does NOT (a dimension never constrains its own menu),
    // so the operator can still widen back out to deprecated/candidate.
    assert_eq!(
        f.statuses.iter().map(|x| x.count).sum::<i64>(),
        4,
        "status facet ignores its own filter"
    );
    assert!(f.statuses.iter().any(|x| x.value == "deprecated"));
}

#[tokio::test]
async fn as_of_time_travel_and_skeleton_spans_timeline() {
    let Some((store, _g)) = setup().await else { return };
    seed(&store).await;
    let mut tx = store.scoped_tx(&pay()).await.expect("tx");

    // 60 days ago: the deprecated "old timeout" (valid -200..-30) WAS true then;
    // the current decision (valid -100..100) was also true; count them.
    let then = Utc::now() - Duration::days(60);
    let f = MemoryFilter { as_of: Some(then), ..Default::default() };
    let n = archive::count(&mut tx, &f).await.expect("count");
    let rows = archive::list(&mut tx, &f, MemorySort::Recent, 50, 0).await.expect("list");
    assert_eq!(n as usize, rows.len());
    assert!(rows.iter().any(|r| r.id == uuid(103)), "deprecated-but-valid-then row returns");

    // The skeleton ignores as_of — it must span the whole corpus so the client
    // can scrub over it — so it has every visible row regardless of `as_of`.
    let skel = archive::validity_skeleton(&mut tx, &f).await.expect("skeleton");
    assert_eq!(skel.len(), 4, "skeleton spans the timeline, not the as_of slice");
    assert!(skel.iter().any(|s| s.id == uuid(103)));
}

#[tokio::test]
async fn paging_is_stable_and_ordered() {
    let Some((store, _g)) = setup().await else { return };
    seed(&store).await;
    let mut tx = store.scoped_tx(&pay()).await.expect("tx");
    let f = MemoryFilter::default();

    let page1 = archive::list(&mut tx, &f, MemorySort::Recent, 2, 0).await.expect("p1");
    let page2 = archive::list(&mut tx, &f, MemorySort::Recent, 2, 2).await.expect("p2");
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    // No overlap across pages.
    for a in &page1 {
        assert!(!page2.iter().any(|b| b.id == a.id), "pages disjoint");
    }
}
