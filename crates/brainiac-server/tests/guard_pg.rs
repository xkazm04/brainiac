//! The write-path guard (DATABASE_URL-gated).
//!
//! What is asserted, in the order that matters:
//!
//! - **A canonical pitfall anchored to the touched project reaches the edit.**
//!   This is the whole thesis: knowledge arrives without the agent asking.
//! - **A mandatory adopted standard arrives unconditionally**, and a
//!   *recommended* one does not — a guard that reports every preference is a
//!   guard that gets muted.
//! - **Monorepo path resolution goes through `find_by_remote`**, so an edit
//!   under one project's `path_prefix` resolves to that project and not a
//!   sibling sharing the remote.
//! - **It never crosses an org.** The preflight runs under `scoped_tx` like
//!   every other read here, so another org's principal sees nothing — the case
//!   worth a test because this endpoint is reachable by any `read` key.
//! - **An unregistered remote is not an error.** The org's cross-cutting
//!   standards still apply; only the project-anchored half goes missing.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{Enforcement, Principal, StandardOrigin};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_server::guard::{preflight, PreEditBody, PreflightError};
use brainiac_store::library::standards::NewStandard;
use brainiac_store::Store;
use uuid::Uuid;

#[tokio::test]
async fn preflight_delivers_project_knowledge_without_being_asked() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE standard_provenance, standard_versions, standards, project_repos, projects,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();
    let seeded = brainiac_eval::seed::seed_gold(&store, &fx, &embedder)
        .await
        .expect("seed");
    let org = stable_uuid(&fx.org.org);

    // A registered repo, split the monorepo way: one remote, two projects.
    let (payments, checkout) = (Uuid::new_v4(), Uuid::new_v4());
    brainiac_store::projects::create(&admin, payments, org, "payments")
        .await
        .expect("project");
    brainiac_store::projects::create(&admin, checkout, org, "checkout")
        .await
        .expect("project 2");
    brainiac_store::projects::add_repo(
        &admin,
        Uuid::new_v4(),
        org,
        payments,
        "github.com/meridian/monorepo",
        "services/payments",
    )
    .await
    .expect("repo");
    brainiac_store::projects::add_repo(
        &admin,
        Uuid::new_v4(),
        org,
        checkout,
        "github.com/meridian/monorepo",
        "apps/checkout",
    )
    .await
    .expect("repo 2");

    // Stamp an existing canonical pitfall onto `payments`. Reusing a seeded
    // memory means its embedding already exists, so the retrieval leg is
    // exercised for real rather than against a hand-inserted row with no vector.
    let row: (Uuid, String) = sqlx::query_as(
        "UPDATE memories SET project_id = $1
          WHERE id = (SELECT id FROM memories
                       WHERE org_id = $2 AND kind = 'pitfall' AND status = 'canonical'
                         AND deleted_at IS NULL
                       ORDER BY id LIMIT 1)
        RETURNING id, content",
    )
    .bind(payments)
    .bind(org)
    .fetch_one(&admin)
    .await
    .expect("a canonical pitfall must exist in the gold corpus");
    let (pitfall_id, pitfall_text) = row;

    // Two standards: one mandatory (must arrive), one recommended (must not).
    let mut conn = admin.acquire().await.expect("conn");
    let author = stable_uuid("user-payments-lead1");
    for (slug, enforcement) in [
        ("psp-idempotency-key", Enforcement::Mandatory),
        ("prefer-structured-logs", Enforcement::Recommended),
    ] {
        let id = Uuid::new_v4();
        brainiac_store::library::standards::insert_standard(
            &mut conn,
            &NewStandard {
                id,
                org_id: org,
                origin: StandardOrigin::Human,
                stack: "general".into(),
                category: "practice".into(),
                slug: slug.into(),
                statement: format!("statement for {slug}"),
                rationale: None,
                detail_md: None,
                enforcement,
                provenance: vec![(brainiac_core::StandardProvenanceKind::Memory, pitfall_id)],
                author: Some(author),
            },
        )
        .await
        .expect("insert standard");
        brainiac_store::library::standards::adopt_standard(&mut conn, id, author, false)
            .await
            .expect("adopt");
    }

    let principal = Principal {
        org_id: org,
        user_id: author,
        team_ids: vec![stable_uuid("team-payments")],
        project_id: None,
    };

    // ── the edit ────────────────────────────────────────────────────────
    let out = preflight(
        &store,
        &embedder,
        seeded.embedding_version,
        &principal,
        PreEditBody {
            remote: "git@github.com:Meridian/monorepo.git".into(), // ssh spelling, normalized
            changed_paths: vec!["services/payments/src/refund.rs".into()],
            tool: Some("Edit".into()),
            diff_excerpt: Some(pitfall_text.clone()),
        },
    )
    .await
    .expect("preflight");

    assert_eq!(
        out.decision, "allow",
        "the guard is advisory, never blocking"
    );
    assert_eq!(
        out.project_name.as_deref(),
        Some("payments"),
        "the path must resolve through the monorepo prefix, not the sibling project"
    );
    assert!(
        out.notes.iter().any(|n| n.memory_id == Some(pitfall_id)),
        "the canonical pitfall anchored to this project must reach the edit unasked"
    );
    assert!(
        out.notes
            .iter()
            .any(|n| n.slug.as_deref() == Some("psp-idempotency-key")),
        "a mandatory adopted standard applies to every edit"
    );
    assert!(
        !out.notes
            .iter()
            .any(|n| n.slug.as_deref() == Some("prefer-structured-logs")),
        "a recommended standard must NOT interrupt an edit — that is how a guard gets muted"
    );

    // ── the sibling project ─────────────────────────────────────────────
    let other = preflight(
        &store,
        &embedder,
        seeded.embedding_version,
        &principal,
        PreEditBody {
            remote: "github.com/meridian/monorepo".into(),
            changed_paths: vec!["apps/checkout/src/page.tsx".into()],
            tool: Some("Edit".into()),
            diff_excerpt: None,
        },
    )
    .await
    .expect("preflight 2");
    assert_eq!(other.project_name.as_deref(), Some("checkout"));

    // ── another org sees nothing of ours ────────────────────────────────
    let stranger = Principal {
        org_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        team_ids: vec![],
        project_id: None,
    };
    let leaked = preflight(
        &store,
        &embedder,
        seeded.embedding_version,
        &stranger,
        PreEditBody {
            remote: "github.com/meridian/monorepo".into(),
            changed_paths: vec!["services/payments/src/refund.rs".into()],
            tool: Some("Edit".into()),
            diff_excerpt: Some(pitfall_text),
        },
    )
    .await
    .expect("preflight 3");
    assert!(
        leaked.project_name.is_none(),
        "another org's repo registration must not resolve"
    );
    assert!(
        leaked.notes.is_empty(),
        "the guard must never carry one org's knowledge into another's edit"
    );

    // ── an unregistered remote is not an error ──────────────────────────
    let unregistered = preflight(
        &store,
        &embedder,
        seeded.embedding_version,
        &principal,
        PreEditBody {
            remote: "github.com/meridian/some-new-repo".into(),
            changed_paths: vec!["src/main.rs".into()],
            tool: Some("Write".into()),
            diff_excerpt: None,
        },
    )
    .await
    .expect("preflight 4");
    assert!(unregistered.project_name.is_none());
    assert!(
        unregistered
            .notes
            .iter()
            .any(|n| n.slug.as_deref() == Some("psp-idempotency-key")),
        "org-wide mandatory standards still apply to a repo nobody has registered yet"
    );

    // ── an unusable remote is refused, not guessed at ───────────────────
    let bad = preflight(
        &store,
        &embedder,
        seeded.embedding_version,
        &principal,
        PreEditBody {
            remote: "not a remote".into(),
            changed_paths: vec!["src/main.rs".into()],
            tool: None,
            diff_excerpt: None,
        },
    )
    .await;
    assert!(matches!(bad, Err(PreflightError::BadRemote)));
}
