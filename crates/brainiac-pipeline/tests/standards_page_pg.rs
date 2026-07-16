//! L8: the Library's judgment renders as a knowledge-base page.
//!
//! What must hold, and why each one is load-bearing:
//!
//! - **Adopted only.** A page that published proposals would put unratified
//!   opinions in the org's mouth on the org's wiki — the exact failure the
//!   gate exists to prevent, amplified by a publishing channel.
//! - **Verbatim.** The rendered statement is byte-identical to the ratified
//!   one. No model is called; a rule re-worded by a machine is a different
//!   rule, and the org never agreed to it.
//! - **Deterministic.** The same rules render byte-identically twice, so a
//!   revision diff means "the org's judgment changed" and never "the model
//!   phrased it differently today".
//! - **It cannot rot.** Retiring a rule marks the page dirty by itself, and
//!   the retired rule leaves on the next render.
//! - **Provenance survives.** The page's citations are the rules' evidence
//!   memories, so a reader is one click from the memory a human signed — and
//!   the dependency index keeps working.

use brainiac_core::{Enforcement, StandardOrigin, StandardProvenanceKind, Visibility};
use brainiac_pipeline::standards_page::{render_stack, scaffold_standards_pages};
use brainiac_store::{library, memories, orgs};
use uuid::Uuid;

fn mem(id: Uuid, org: Uuid, content: &str) -> memories::NewMemory {
    memories::NewMemory {
        id,
        org_id: org,
        team_id: None,
        owner_user_id: None,
        visibility: Visibility::Org,
        status: brainiac_core::MemoryStatus::Canonical,
        kind: brainiac_core::MemoryKind::Pitfall,
        title: None,
        lifecycle: brainiac_core::Lifecycle::Shipped,
        detail_md: None,
        content: content.to_string(),
        language: "en".into(),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
        confidence: Some(0.9),
        provenance_id: None,
    }
}

#[tokio::test]
async fn standards_render_verbatim_adopted_only_and_cannot_rot() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences, memory_feedback,
                  document_reads, document_dependencies, document_revisions,
                  document_sections, documents,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let org = Uuid::new_v4();
    let maintainer = Uuid::new_v4();
    let mut conn = admin.acquire().await.expect("conn");
    orgs::upsert_org(&mut conn, org, "meridian")
        .await
        .expect("org");

    // The evidence a rule cites.
    let evidence = Uuid::new_v4();
    memories::insert(
        &mut conn,
        &mem(evidence, org, "the June psp-gateway incident"),
    )
    .await
    .expect("evidence");

    // The ratified sentence — the exact bytes that must reach the page.
    const RATIFIED: &str = "Request handlers never unwrap; they map errors to typed responses.";
    const EXAMPLES: &str = "```rust\nlet body = parse(&raw)?;\n```";

    let mk = |id: Uuid, slug: &str, statement: &str, prov: Vec<(StandardProvenanceKind, Uuid)>| {
        library::NewStandard {
            id,
            org_id: org,
            origin: StandardOrigin::Human,
            stack: "rust".into(),
            category: "errors".into(),
            slug: slug.into(),
            statement: statement.into(),
            rationale: Some("Learned the hard way.".into()),
            detail_md: Some(EXAMPLES.into()),
            enforcement: Enforcement::Mandatory,
            provenance: prov,
            author: Some(maintainer),
        }
    };

    let (adopted, retire_me, at_gate) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    library::insert_standard(
        &mut conn,
        &mk(
            adopted,
            "no-unwrap",
            RATIFIED,
            vec![(StandardProvenanceKind::Memory, evidence)],
        ),
    )
    .await
    .expect("s1");
    library::insert_standard(
        &mut conn,
        &mk(
            retire_me,
            "doomed-rule",
            "This one will be retired.",
            vec![(StandardProvenanceKind::Memory, evidence)],
        ),
    )
    .await
    .expect("s2");
    library::insert_standard(
        &mut conn,
        &mk(
            at_gate,
            "not-yet",
            "A proposal nobody has ratified.",
            vec![(StandardProvenanceKind::Memory, evidence)],
        ),
    )
    .await
    .expect("s3");
    for id in [adopted, retire_me] {
        assert!(library::adopt_standard(&mut conn, id, maintainer, false)
            .await
            .expect("adopt"));
    }

    // ── the render ───────────────────────────────────────────────────────
    let out = render_stack(&mut conn, "rust").await.expect("render");

    assert!(
        out.markdown.contains(RATIFIED),
        "the ratified sentence must reach the page byte-identical — a rule re-worded by a \
         machine is a different rule, and the org never agreed to it:\n{}",
        out.markdown
    );
    assert!(out.markdown.contains(EXAMPLES), "examples travel verbatim");
    assert!(
        !out.markdown.contains("A proposal nobody has ratified"),
        "a PROPOSAL must never reach the page: publishing unratified opinion as the org's \
         standard is the gate's whole reason to exist:\n{}",
        out.markdown
    );
    assert!(
        out.markdown.contains(&format!("[m:{evidence}]")),
        "the page cites the memory behind the rule — a reader is one click from the evidence"
    );
    assert_eq!(
        out.cited,
        vec![evidence],
        "the dependency set is the evidence closure"
    );

    // Deterministic: the same rules, twice, byte for byte.
    let again = render_stack(&mut conn, "rust").await.expect("render again");
    assert_eq!(
        out.markdown, again.markdown,
        "a second render must be byte-identical, or a revision diff means 'the model had a \
         different day' rather than 'the org changed its mind'"
    );

    // ── scaffolding: three adopted rules earn a page ─────────────────────
    // Two adopted so far — below the bar.
    let made = scaffold_standards_pages(&mut conn, org, 5)
        .await
        .expect("scaffold");
    assert!(
        made.is_empty(),
        "two rules is a pair, not a body of judgment"
    );

    let third = Uuid::new_v4();
    library::insert_standard(
        &mut conn,
        &mk(
            third,
            "third-rule",
            "The third rule.",
            vec![(StandardProvenanceKind::Memory, evidence)],
        ),
    )
    .await
    .expect("s4");
    assert!(library::adopt_standard(&mut conn, third, maintainer, false)
        .await
        .expect("adopt"));

    let made = scaffold_standards_pages(&mut conn, org, 5)
        .await
        .expect("scaffold");
    assert_eq!(made.len(), 1, "the stack earned its page");
    let page = brainiac_store::documents::get_document(&mut conn, made[0])
        .await
        .expect("get")
        .expect("page");
    assert_eq!(page.slug, "standards-rust");
    assert_eq!(page.doc_kind, brainiac_core::DocKind::StandardsPage);
    assert_eq!(
        page.status,
        brainiac_core::DocStatus::Draft,
        "the machine decides a page should exist; a human decides it is right"
    );
    assert_eq!(
        page.visibility,
        Visibility::Org,
        "a standard is the org's shared judgment — and org tier is what may publish outward"
    );
    assert!(
        page.dirty_at.is_some(),
        "born dirty: it must render before it is worth reviewing"
    );

    // Scaffolding is idempotent — a second sweep does not mint a second page.
    let again = scaffold_standards_pages(&mut conn, org, 5)
        .await
        .expect("scaffold again");
    assert!(again.is_empty());

    // ── it cannot rot: retiring a rule marks the page stale by itself ────
    sqlx::query("UPDATE documents SET dirty_at = NULL WHERE id = $1")
        .bind(page.id)
        .execute(&admin)
        .await
        .expect("clean");
    assert!(
        library::deprecate_standard(&mut conn, retire_me, maintainer)
            .await
            .expect("deprecate")
    );
    let after = brainiac_store::documents::get_document(&mut conn, page.id)
        .await
        .expect("get")
        .expect("page");
    assert!(
        after.dirty_at.is_some(),
        "retiring a rule must mark its stack's page stale WITHOUT anyone remembering to — \
         otherwise the wiki keeps publishing a rule the org retired"
    );
    let out = render_stack(&mut conn, "rust").await.expect("render");
    assert!(
        !out.markdown.contains("This one will be retired"),
        "the retired rule must leave the page:\n{}",
        out.markdown
    );

    // ── a decreed rule says so, in the open ──────────────────────────────
    let decreed = Uuid::new_v4();
    library::insert_standard(
        &mut conn,
        &library::NewStandard {
            id: decreed,
            org_id: org,
            origin: StandardOrigin::Human,
            stack: "rust".into(),
            category: "style".into(),
            slug: "just-because".into(),
            statement: "Spaces.".into(),
            rationale: None,
            detail_md: None,
            enforcement: Enforcement::Recommended,
            provenance: vec![],
            author: Some(maintainer),
        },
    )
    .await
    .expect("decreed");
    assert!(
        library::adopt_standard(&mut conn, decreed, maintainer, true)
            .await
            .expect("decree")
    );
    let out = render_stack(&mut conn, "rust").await.expect("render");
    assert!(
        out.markdown.contains("decreed"),
        "a rule resting on judgement alone must say so on the page — a reader is entitled to \
         know which rules have no evidence under them:\n{}",
        out.markdown
    );
}

#[tokio::test]
async fn a_library_first_org_is_visited_by_the_compose_sweep() {
    // F-9: the compose sweep chooses which orgs to visit, and the standards-page
    // scaffold's trigger (>=3 adopted rules) is evaluated INSIDE that visit. An
    // org that adopts rules but has no graph and no pages must still be visited,
    // or its standards page never scaffolds — the Library's KB projection would
    // silently depend on the Memory graph being populated. This is a
    // Library-first org: adopted standards, zero canonical entities, zero
    // documents. Before the fix, orgs_with_compose_work skipped it entirely.
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE library_usage_events, skill_versions, skills, standard_provenance,
                  standard_versions, standards, practice_divergences, memory_feedback,
                  document_reads, document_dependencies, document_revisions,
                  document_sections, documents,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let org = Uuid::new_v4();
    let maintainer = Uuid::new_v4();
    let mut conn = admin.acquire().await.expect("conn");
    orgs::upsert_org(&mut conn, org, "library-first")
        .await
        .expect("org");

    // Three adopted rules, each with memory provenance so adoption is plain
    // (no decree) — and NOTHING else: no memories in a graph, no entities, no
    // documents.
    let ev = Uuid::new_v4();
    memories::insert(&mut conn, &mem(ev, org, "the incident behind the rule"))
        .await
        .expect("evidence");
    for i in 0..3 {
        let id = Uuid::new_v4();
        library::insert_standard(
            &mut conn,
            &library::NewStandard {
                id,
                org_id: org,
                origin: StandardOrigin::Human,
                stack: "rust".into(),
                category: "errors".into(),
                slug: format!("rule-{i}"),
                statement: format!("Rule number {i}, in one sentence."),
                rationale: None,
                detail_md: None,
                enforcement: Enforcement::Recommended,
                provenance: vec![(StandardProvenanceKind::Memory, ev)],
                author: Some(maintainer),
            },
        )
        .await
        .expect("insert");
        assert!(library::adopt_standard(&mut conn, id, maintainer, false)
            .await
            .expect("adopt"));
    }

    // Ground truth: this org has no entities and no documents — the two things
    // the sweep used to key on.
    let entities: i64 =
        sqlx::query_scalar("SELECT count(*) FROM canonical_entities WHERE org_id = $1")
            .bind(org)
            .fetch_one(&admin)
            .await
            .expect("entities");
    let docs: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE org_id = $1")
        .bind(org)
        .fetch_one(&admin)
        .await
        .expect("docs");
    assert_eq!(
        (entities, docs),
        (0, 0),
        "the setup must be a library-first org"
    );

    // THE FIX: the sweep's org selection must include this org, on the strength
    // of its adopted standards alone.
    let visited = brainiac_pipeline::compose::orgs_with_compose_work(&admin)
        .await
        .expect("orgs");
    assert!(
        visited.contains(&org),
        "an org with adopted rules but no graph must still be visited by the compose sweep, \
         or its standards page never scaffolds (F-9)"
    );

    // And the payoff: visiting it, the scaffold now makes the page.
    let made = scaffold_standards_pages(&mut conn, org, 5)
        .await
        .expect("scaffold");
    assert_eq!(
        made.len(),
        1,
        "the library-first org earns its standards page"
    );
    let page = brainiac_store::documents::get_document(&mut conn, made[0])
        .await
        .expect("get")
        .expect("page");
    assert_eq!(page.slug, "standards-rust");
    assert_eq!(page.doc_kind, brainiac_core::DocKind::StandardsPage);
}
