//! End-to-end round trip of the OKF export boundary (DATABASE_URL-gated).
//!
//! The unit tests in `src/okf.rs` hand the publisher a hand-built
//! `PageToPublish` — they pin the *rendering*. Nothing exercised the path the
//! product actually runs: real rows in Postgres, RLS on, `publish_org` deciding
//! what may leave, and a bundle on disk that an OKF consumer has to be able to
//! read. A regression there ships malformed artifacts to KB/OKF consumers
//! silently, because no caller of ours ever parses what we wrote.
//!
//! So this file writes the bundle with the real export and then READS IT BACK
//! as a consumer would — parsing the YAML frontmatter, following the `index.md`
//! links, checking the ids resolve to memories that actually exist — and
//! asserts on invariants rather than on a snapshot string. The invariants:
//!
//! - every page opens with well-formed, closed frontmatter carrying the OKF
//!   required fields, and none of them is empty;
//! - `resource` and the `index.md` links resolve — to the page's own slug, to a
//!   file that exists in the bundle;
//! - `x_brainiac_cited_memories` is exactly the revision's provenance closure,
//!   and every id in it resolves to a live memory row;
//! - a key is emitted only when it has content (no `tags:` with nothing under
//!   it — a consumer would read that as a parse error, or worse, as a fact);
//! - the bundle is a closed world per org: an export for org A contains nothing
//!   of org B's, in either direction, including the team-private canary.

use brainiac_core::{
    DocKind, Lifecycle, MemoryKind, MemoryStatus, Principal, RevisionPolicy, Visibility,
};
use brainiac_store::documents::{NewDocument, NewRevision};
use brainiac_store::memories::NewMemory;
use brainiac_store::publishing::{self, PublishTarget};
use brainiac_store::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const CONSOLE: &str = "https://console.test";
/// Strings that must never cross an org boundary.
const TEAM_CANARY: &str = "TEAM-PRIVATE-CANARY";
const ORG_B_CANARY: &str = "ACME-ORG-B-CANARY";

// ─────────────────────────── the consumer's parser ───────────────────────────

/// A page's YAML frontmatter, as much of it as OKF actually uses: scalars and
/// lists of scalars. Deliberately hand-rolled — parsing with the same library
/// that wrote it would prove nothing.
#[derive(Debug, Default)]
struct Frontmatter {
    scalars: BTreeMap<String, String>,
    lists: BTreeMap<String, Vec<String>>,
}

impl Frontmatter {
    fn get(&self, key: &str) -> Option<&str> {
        self.scalars.get(key).map(String::as_str)
    }
    fn require(&self, key: &str) -> &str {
        let v = self
            .get(key)
            .unwrap_or_else(|| panic!("frontmatter is missing required key `{key}`: {self:?}"));
        assert!(
            !v.is_empty(),
            "frontmatter key `{key}` is present but empty"
        );
        v
    }
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return v.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split a written file into (frontmatter, body). Panics — loudly — on anything
/// a conformant OKF consumer would choke on, which is the point.
fn parse_page(text: &str) -> (Frontmatter, String) {
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("page does not open with a frontmatter fence:\n{text}"));
    let end = rest
        .find("\n---\n")
        .unwrap_or_else(|| panic!("frontmatter is never closed:\n{text}"));
    let (fm_text, body) = (&rest[..end], &rest[end + 5..]);

    let mut fm = Frontmatter::default();
    let mut open_list: Option<String> = None;
    for line in fm_text.lines() {
        if let Some(item) = line.strip_prefix("  - ") {
            let key = open_list
                .clone()
                .unwrap_or_else(|| panic!("list item `{item}` under no key:\n{fm_text}"));
            fm.lists.entry(key).or_default().push(unquote(item));
            continue;
        }
        let (k, v) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("frontmatter line is not `key: value`: `{line}`"));
        if v.trim().is_empty() {
            open_list = Some(k.to_string());
            fm.lists.entry(k.to_string()).or_default();
        } else {
            open_list = None;
            assert!(
                fm.scalars.insert(k.to_string(), unquote(v)).is_none(),
                "frontmatter key `{k}` appears twice"
            );
        }
    }
    (fm, body.to_string())
}

/// The `* [Title](target.md) - description` entries of an index.
fn index_links(index: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in index.lines() {
        let Some(rest) = line.strip_prefix("* [") else {
            continue;
        };
        let Some((title, rest)) = rest.split_once("](") else {
            panic!("malformed index entry: `{line}`");
        };
        let Some((target, _)) = rest.split_once(')') else {
            panic!("malformed index entry: `{line}`");
        };
        out.push((title.to_string(), target.to_string()));
    }
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Every file in the bundle plus the pointer files — for the isolation sweep.
fn all_text_under(root: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(all_text_under(&path));
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((path, text));
        }
    }
    out
}

// ───────────────────────────────── fixtures ─────────────────────────────────

/// One org, its OKF target, and where that target writes.
struct OrgCtx {
    org_id: Uuid,
    team_id: Uuid,
    /// The repo checkout the OKF target owns (pointer files land here).
    repo: PathBuf,
}

impl OrgCtx {
    fn principal(&self) -> Principal {
        Principal {
            org_id: self.org_id,
            user_id: Uuid::from_bytes(*b"brainiac-tester!"),
            team_ids: vec![self.team_id],
            project_id: None,
        }
    }
    fn bundle(&self) -> PathBuf {
        self.repo.join("docs").join("okf")
    }
    fn page(&self, slug: &str) -> PathBuf {
        self.bundle().join(format!("{slug}.md"))
    }
}

struct Ctx {
    store: Store,
    admin: sqlx::PgPool,
    a: OrgCtx,
    b: OrgCtx,
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.a.repo);
        let _ = std::fs::remove_dir_all(&self.b.repo);
    }
}

async fn setup(url: &str) -> Ctx {
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
    let a = OrgCtx {
        org_id: Uuid::new_v4(),
        team_id: Uuid::new_v4(),
        repo: std::env::temp_dir().join(format!("brainiac-okf-a-{}", Uuid::new_v4())),
    };
    let b = OrgCtx {
        org_id: Uuid::new_v4(),
        team_id: Uuid::new_v4(),
        repo: std::env::temp_dir().join(format!("brainiac-okf-b-{}", Uuid::new_v4())),
    };
    seed_org(&store, &a, "meridian").await;
    seed_org(&store, &b, "acme").await;
    Ctx { store, admin, a, b }
}

/// An org with the KB layer on and one enabled OKF target aimed at its repo.
async fn seed_org(store: &Store, org: &OrgCtx, name: &str) {
    let user_id = Uuid::new_v4();
    let p = org.principal();
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org.org_id, name)
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, org.team_id, org.org_id, "payments")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user_id, org.org_id, &format!("dev@{name}.test"))
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, org.team_id, user_id, "maintainer")
        .await
        .expect("member");
    publishing::set_kb_enabled(&mut tx, org.org_id, true)
        .await
        .expect("kb flag");
    publishing::insert_target(
        &mut tx,
        &PublishTarget {
            id: Uuid::new_v4(),
            org_id: org.org_id,
            kind: "okf".into(),
            config: serde_json::json!({
                "repo_path": org.repo.to_string_lossy(),
                "bundle_title": format!("{name} knowledge"),
            }),
            secret_ref: None,
            enabled: true,
        },
    )
    .await
    .expect("target");
    tx.commit().await.expect("commit");
}

/// `n` canonical, shipped, org-visible memories — a corpus healthy enough that
/// the publish breaker has no reason to trip.
async fn healthy_corpus(ctx: &Ctx, org: &OrgCtx, n: usize, prefix: &str) -> Vec<Uuid> {
    let p = org.principal();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let mut ids = Vec::new();
    for i in 0..n {
        let id = Uuid::new_v4();
        brainiac_store::memories::insert(
            &mut tx,
            &NewMemory {
                id,
                org_id: org.org_id,
                team_id: Some(org.team_id),
                owner_user_id: None,
                visibility: Visibility::Org,
                status: MemoryStatus::Canonical,
                kind: MemoryKind::Fact,
                title: None,
                content: format!("{prefix} fact number {i}"),
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
        ids.push(id);
    }
    tx.commit().await.expect("commit");
    ids
}

/// Anchor a memory to a canonical entity, so the page's `tags` come out of the
/// real entity graph rather than a literal.
async fn anchor(ctx: &Ctx, org: &OrgCtx, memory_id: Uuid, canonical_name: &str) {
    let p = org.principal();
    let mut tx = ctx.store.scoped_tx(&p).await.expect("tx");
    let entity_id = Uuid::new_v4();
    let canonical_id = Uuid::new_v4();
    brainiac_store::entities::insert_entity(
        &mut tx,
        entity_id,
        org.org_id,
        Some(org.team_id),
        canonical_name,
        "service",
        &[],
        None,
    )
    .await
    .expect("entity");
    brainiac_store::entities::insert_canonical(
        &mut tx,
        canonical_id,
        org.org_id,
        canonical_name,
        "service",
    )
    .await
    .expect("canonical");
    brainiac_store::entities::link(&mut tx, entity_id, canonical_id, 1.0, "exact", None)
        .await
        .expect("link");
    brainiac_store::memories::link_entity(&mut tx, memory_id, entity_id)
        .await
        .expect("memory_entities");
    tx.commit().await.expect("commit");
}

struct PageSpec<'a> {
    slug: &'a str,
    title: &'a str,
    kind: DocKind,
    visibility: Visibility,
    body: &'a str,
    cited: Vec<Uuid>,
}

/// A document with one published (current) revision.
async fn published_page(ctx: &Ctx, org: &OrgCtx, spec: &PageSpec<'_>) -> Uuid {
    let p = org.principal();
    // worker_tx: a TEAM page is not SELECT-able under the ordinary policy, and
    // Postgres applies the SELECT policy to an UPDATE's WHERE — so the "make
    // this revision current" update would silently no-op. Same reason as
    // brainiac-server/tests/publish_pg.rs.
    let mut tx = ctx.store.worker_tx(&p).await.expect("tx");
    let doc_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: org.org_id,
            team_id: Some(org.team_id),
            slug: spec.slug.into(),
            title: spec.title.into(),
            visibility: spec.visibility,
            doc_kind: spec.kind,
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
            org_id: org.org_id,
            content_md: spec.body.into(),
            composed_from: spec.cited.clone(),
            trigger: "manual".into(),
            policy_decision: RevisionPolicy::AutoPublished,
            claimed_updated_at: None,
        },
    )
    .await
    .expect("revision");
    tx.commit().await.expect("commit");
    doc_id
}

/// Org A's publishable world: two org pages (one with entity anchors, one
/// without), one team-private page that must never leave, and a backdated
/// earlier revision so `log.md` has more than one date to group.
async fn seed_org_a(ctx: &Ctx) -> Vec<Uuid> {
    let mems = healthy_corpus(ctx, &ctx.a, 10, "meridian").await;
    anchor(ctx, &ctx.a, mems[0], "PSP Gateway").await;
    anchor(ctx, &ctx.a, mems[1], "Meridian Payments").await;

    let gateway = published_page(
        ctx,
        &ctx.a,
        &PageSpec {
            slug: "psp-gateway",
            title: "PSP \"Gateway\"",
            kind: DocKind::EntityPage,
            visibility: Visibility::Org,
            body: "# PSP Gateway\n\nThe gateway retries with a 30 second cap.\n",
            cited: vec![mems[0], mems[1]],
        },
    )
    .await;
    published_page(
        ctx,
        &ctx.a,
        &PageSpec {
            slug: "retry-policy",
            title: "Retry policy",
            kind: DocKind::Runbook,
            visibility: Visibility::Org,
            body: "# Retry policy\n\nBack off exponentially and cap at 30 seconds.\n",
            cited: vec![mems[2]],
        },
    )
    .await;
    published_page(
        ctx,
        &ctx.a,
        &PageSpec {
            slug: "team-only-runbook",
            title: "Team only runbook",
            kind: DocKind::Runbook,
            visibility: Visibility::Team,
            body: &format!("# Team only\n\n{TEAM_CANARY} paged us at 3am.\n"),
            cited: vec![mems[3]],
        },
    )
    .await;

    // An older revision of the gateway page, backdated so the changelog has two
    // date groups to order. Written as admin: this is fixture surgery, not a
    // path the product takes.
    let old_rev = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO document_revisions
            (id, document_id, org_id, content_md, composed_from, trigger, policy_decision,
             created_at, published_at)
         VALUES ($1,$2,$3,$4,$5,'memory_change','needs_review',
                 timestamptz '2020-01-02 00:00:00Z', NULL)",
    )
    .bind(old_rev)
    .bind(gateway)
    .bind(ctx.a.org_id)
    .bind("# PSP Gateway\n\nAn older account.\n")
    .bind(serde_json::json!([mems[0]]))
    .execute(&ctx.admin)
    .await
    .expect("backdated revision");

    mems
}

async fn seed_org_b(ctx: &Ctx) {
    let mems = healthy_corpus(ctx, &ctx.b, 10, "acme").await;
    published_page(
        ctx,
        &ctx.b,
        &PageSpec {
            slug: "acme-billing",
            title: "Acme Billing",
            kind: DocKind::TopicPage,
            visibility: Visibility::Org,
            body: &format!("# Acme Billing\n\n{ORG_B_CANARY} bills monthly.\n"),
            cited: vec![mems[0]],
        },
    )
    .await;
}

// ────────────────────────────────── tests ──────────────────────────────────

#[tokio::test]
async fn okf_export_round_trips_and_every_field_resolves() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    let mems = seed_org_a(&ctx).await;

    // ── the real export path, not a re-implementation of it.
    let stats = brainiac_publish::publish_org(&ctx.store, ctx.a.org_id, CONSOLE)
        .await
        .expect("publish");
    assert_eq!(stats.pushed, 2, "both org-visible pages must publish");
    assert_eq!(stats.failed, 0);
    assert_eq!(
        stats.blocked, 0,
        "a healthy corpus must not trip the breaker"
    );

    // ── page 1: an entity page with real anchors behind its tags.
    let (fm, body) = parse_page(&read(&ctx.a.page("psp-gateway")));
    assert_eq!(fm.require("type"), "Entity Page");
    // The quote in the title survived as a YAML string, not as broken syntax.
    assert_eq!(fm.require("title"), "PSP \"Gateway\"");
    assert_eq!(
        fm.require("description"),
        "The gateway retries with a 30 second cap."
    );
    assert_eq!(
        fm.require("resource"),
        format!("{CONSOLE}/docs/psp-gateway"),
        "`resource` must point back at this page's own console URL"
    );
    assert!(
        fm.require("resource").ends_with("/psp-gateway"),
        "the resource slug must match the file name"
    );
    chrono::DateTime::parse_from_rfc3339(fm.require("timestamp"))
        .expect("`timestamp` must be RFC 3339 — OKF consumers sort on it");
    assert_eq!(fm.require("x_brainiac_policy"), "auto_published");
    assert_eq!(fm.require("x_brainiac_stale"), "false");

    let tags = fm.lists.get("tags").expect("anchored page must carry tags");
    assert_eq!(
        tags.iter().collect::<BTreeSet<_>>(),
        ["Meridian Payments".to_string(), "PSP Gateway".to_string()]
            .iter()
            .collect::<BTreeSet<_>>(),
        "tags must be the canonical entity names the cited memories anchor to"
    );

    // The provenance closure is exactly the revision's, and every id in it is a
    // memory that actually exists — a citation to nothing is worse than none.
    let cited: Vec<Uuid> = fm
        .lists
        .get("x_brainiac_cited_memories")
        .expect("provenance must ship")
        .iter()
        .map(|s| Uuid::parse_str(s).expect("cited memory id must be a uuid"))
        .collect();
    assert_eq!(
        cited.iter().collect::<BTreeSet<_>>(),
        [mems[0], mems[1]].iter().collect::<BTreeSet<_>>()
    );
    let mut tx = ctx.store.scoped_tx(&ctx.a.principal()).await.expect("tx");
    let resolved = brainiac_store::memories::get_by_ids(&mut tx, &cited)
        .await
        .expect("resolve cited memories");
    tx.commit().await.expect("commit");
    assert_eq!(
        resolved.len(),
        cited.len(),
        "a cited memory id does not resolve to a live memory"
    );

    // The body is the banner plus the composed page, in that order.
    assert!(
        body.trim_start().starts_with("> **Generated by Brainiac"),
        "the generated-content banner must lead the body:\n{body}"
    );
    assert!(body.contains(&format!("{CONSOLE}/docs/psp-gateway")));
    assert!(body.contains("The gateway retries with a 30 second cap."));

    // ── page 2: no anchors, so no `tags:` key at all. An empty list key would
    // read to a consumer as "this page has tags", which is a lie.
    let (fm2, body2) = parse_page(&read(&ctx.a.page("retry-policy")));
    assert_eq!(fm2.require("type"), "Runbook");
    assert_eq!(fm2.require("title"), "Retry policy");
    assert!(
        !fm2.lists.contains_key("tags"),
        "an unanchored page emitted an empty `tags` key: {fm2:?}"
    );
    assert!(body2.contains("Back off exponentially"));

    // ── the team page never left the building.
    assert!(
        !ctx.a.page("team-only-runbook").exists(),
        "a TEAM-visible page was written into an external bundle"
    );

    // ── index.md: the front door, and every link in it resolves.
    let index_text = read(&ctx.a.bundle().join("index.md"));
    let (index_fm, index_body) = parse_page(&index_text);
    assert_eq!(index_fm.require("okf_version"), "0.1");
    assert!(
        index_body.starts_with("# meridian knowledge\n"),
        "the target's configured bundle title must head the index:\n{index_body}"
    );
    let links = index_links(&index_text);
    let linked: BTreeSet<String> = links.iter().map(|(_, t)| t.clone()).collect();
    assert_eq!(
        linked,
        ["psp-gateway.md".to_string(), "retry-policy.md".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "index.md must list exactly the published pages"
    );
    for (title, target) in &links {
        assert!(!title.is_empty(), "an index entry has an empty title");
        assert!(
            ctx.a.bundle().join(target).exists(),
            "index.md links `{target}`, which is not in the bundle"
        );
    }
    assert!(
        index_body.contains("# Services & systems") && index_body.contains("# Runbooks"),
        "pages must be grouped by kind:\n{index_body}"
    );

    // ── log.md: date-grouped, newest first, one entry per revision.
    let log = read(&ctx.a.bundle().join("log.md"));
    assert!(
        !log.is_empty(),
        "log.md must not be empty for a live bundle"
    );
    let dates: Vec<&str> = log
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(str::trim)
        .collect();
    assert!(
        dates.len() >= 2,
        "both revision dates should be grouped:\n{log}"
    );
    assert_eq!(*dates.last().expect("a date"), "2020-01-02");
    let mut sorted = dates.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(dates, sorted, "log.md must be newest-first:\n{log}");
    for date in &dates {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("log heading must be a date");
    }
    assert!(log.contains("recomposed after a memory change, held for human review"));
    assert!(log.contains("manually recomposed, auto-published"));
    for (title, _) in &links {
        assert!(
            log.contains(title.as_str()),
            "log.md never mentions `{title}`:\n{log}"
        );
    }

    // ── the pointer files, at the REPO root, aimed at the bundle.
    let agents = read(&ctx.a.repo.join("AGENTS.md"));
    assert!(agents.contains("docs/okf/index.md"), "{agents}");
    assert!(agents.contains(CONSOLE));
    assert!(read(&ctx.a.repo.join("CLAUDE.md")).contains("docs/okf/index.md"));
    assert!(
        !ctx.a.bundle().join("AGENTS.md").exists(),
        "the pointer file belongs at the repo root, not inside the bundle"
    );

    // ── nothing team-private anywhere in what we wrote.
    for (path, text) in all_text_under(&ctx.a.repo) {
        assert!(
            !text.contains(TEAM_CANARY),
            "team-private content reached {}",
            path.display()
        );
    }
}

#[tokio::test]
async fn a_bundle_is_a_closed_world_per_org() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    seed_org_a(&ctx).await;
    seed_org_b(&ctx).await;

    let a = brainiac_publish::publish_org(&ctx.store, ctx.a.org_id, CONSOLE)
        .await
        .expect("publish a");
    let b = brainiac_publish::publish_org(&ctx.store, ctx.b.org_id, CONSOLE)
        .await
        .expect("publish b");
    assert_eq!(a.pushed, 2);
    assert_eq!(b.pushed, 1);

    let a_files = all_text_under(&ctx.a.repo);
    let b_files = all_text_under(&ctx.b.repo);
    assert!(!a_files.is_empty() && !b_files.is_empty());

    // Neither bundle knows the other exists — not the page, not the slug, not
    // the org id, not a stray canary in a changelog line.
    for (path, text) in &a_files {
        for needle in [ORG_B_CANARY, "acme-billing", "Acme Billing"] {
            assert!(
                !text.contains(needle),
                "org B's `{needle}` leaked into org A's bundle at {}",
                path.display()
            );
        }
        assert!(!text.contains(&ctx.b.org_id.to_string()));
    }
    for (path, text) in &b_files {
        for needle in [TEAM_CANARY, "psp-gateway", "PSP Gateway", "retry-policy"] {
            assert!(
                !text.contains(needle),
                "org A's `{needle}` leaked into org B's bundle at {}",
                path.display()
            );
        }
        assert!(!text.contains(&ctx.a.org_id.to_string()));
    }

    // And each index lists only its own org's pages.
    let a_links: BTreeSet<String> = index_links(&read(&ctx.a.bundle().join("index.md")))
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    let b_links: BTreeSet<String> = index_links(&read(&ctx.b.bundle().join("index.md")))
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    assert!(
        a_links.is_disjoint(&b_links),
        "two orgs' indexes share a page: {a_links:?} / {b_links:?}"
    );
    assert_eq!(b_links.len(), 1);
    assert!(!ctx.b.page("psp-gateway").exists());
}

#[tokio::test]
async fn re_exporting_an_unchanged_corpus_is_a_no_op_on_disk() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let ctx = setup(&url).await;
    seed_org_a(&ctx).await;

    let first = brainiac_publish::publish_org(&ctx.store, ctx.a.org_id, CONSOLE)
        .await
        .expect("publish");
    assert_eq!(first.pushed, 2);
    let before: BTreeMap<PathBuf, String> = all_text_under(&ctx.a.repo).into_iter().collect();

    let second = brainiac_publish::publish_org(&ctx.store, ctx.a.org_id, CONSOLE)
        .await
        .expect("republish");
    assert_eq!(second.pushed, 0, "an unchanged revision was pushed again");
    assert_eq!(second.unchanged, 2);
    assert_eq!(second.failed, 0);

    // The bundle-level artifacts regenerate every run — idempotently. A bundle
    // that churns on every tick turns a repo's git history into noise, and the
    // pointer block must not grow a ladder of duplicates.
    let after: BTreeMap<PathBuf, String> = all_text_under(&ctx.a.repo).into_iter().collect();
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the second export added or removed files"
    );
    for (path, text) in &before {
        assert_eq!(
            after.get(path),
            Some(text),
            "{} changed on an unchanged corpus",
            path.display()
        );
    }
    assert_eq!(
        read(&ctx.a.repo.join("AGENTS.md"))
            .matches(brainiac_publish::pointer::START)
            .count(),
        1,
        "the managed pointer block was duplicated"
    );
}
