//! LIBRARY-PLAN L8: the org's adopted rules, rendered as a knowledge-base page.
//!
//! A standards page rides the entire document layer — dirty-marking,
//! revisions, the review gate, the health circuit breaker, the Confluence
//! target — with ONE difference, and it is the whole design:
//!
//! **It is projected, never composed.** Every other page hands its memories to
//! a model and asks for prose. A rule's statement is one sentence a named
//! human ratified; asking a model to re-word it would fork the org's own
//! commitment — the page would ask people to follow something subtly
//! different from what the gate approved. That is the same reason `detail_md`
//! is copied verbatim and never re-typed (KB-PLAN D3), pointed at the layer
//! where it matters most. So: no LLM, no temperature, no citation firewall to
//! police — there is nothing here for a model to add and everything for it to
//! get quietly wrong.
//!
//! What the page still inherits for free, because it is a real document:
//! - **Provenance**: `composed_from` is the union of the rules' evidence
//!   memories, so the dependency index marks the page dirty when the evidence
//!   underneath a rule moves — and a reader in Confluence is one click from
//!   the governed memory a human signed.
//! - **The breaker**: a rotting corpus stops publishing standards outward,
//!   exactly as it stops publishing anything else.
//! - **Visibility**: org-only, like every externally publishable page.

use anyhow::Result;
use brainiac_core::{Standard, StandardLifecycle};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A stack earns a page at this many adopted rules. Three: one rule is a note,
/// two is a pair, three is a body of judgment worth a page of its own.
pub const SCAFFOLD_MIN_RULES: i64 = 3;

/// The rendered page plus the evidence closure behind it.
pub struct RenderedStandards {
    pub markdown: String,
    /// Every memory cited by every rule on the page — the page's dependency
    /// set, so evidence moving marks it dirty like any other document.
    pub cited: Vec<Uuid>,
}

fn enforcement_note(e: &str) -> &'static str {
    match e {
        "mandatory" => "**Mandatory.**",
        "experimental" => "*Experimental — trial it, tell us how it went.*",
        _ => "Recommended.",
    }
}

/// Render one stack's adopted rules. Deterministic: the same rules always
/// produce byte-identical markdown, which is what lets the revision diff mean
/// "the org's judgment changed" rather than "the model phrased it differently
/// today".
pub async fn render_stack(conn: &mut PgConnection, stack: &str) -> Result<RenderedStandards> {
    let rules: Vec<Standard> = brainiac_store::library::list_standards(
        conn,
        Some(stack),
        Some(StandardLifecycle::Adopted),
    )
    .await?;

    let mut md = String::new();
    let mut cited: Vec<Uuid> = Vec::new();

    if rules.is_empty() {
        // The honest empty page, same as an uncomposed section: nobody has
        // ratified anything for this stack yet.
        md.push_str("(no rules adopted for this stack yet)\n");
        return Ok(RenderedStandards {
            markdown: md,
            cited,
        });
    }

    md.push_str(
        "Every rule below was ratified by a named human. Nothing reaches this page \
         until it passes that gate, and nothing on it was written by a model — each \
         statement is reproduced exactly as it was adopted.\n",
    );

    // Group by category so the page reads as a guide rather than a list. The
    // ordering is the store's (stack, category, slug) — stable, so a reader
    // returning to the page finds a rule where they left it.
    let mut current_category = String::new();
    for r in &rules {
        if r.category != current_category {
            md.push_str(&format!("\n## {}\n", r.category));
            current_category = r.category.clone();
        }
        md.push_str(&format!("\n### {}\n\n", r.statement));
        md.push_str(&format!("{}\n", enforcement_note(r.enforcement.as_str())));

        if let Some(rationale) = &r.rationale {
            md.push_str(&format!("\n{rationale}\n"));
        }
        // The examples, verbatim — the artifact survives the summary.
        if let Some(detail) = &r.detail_md {
            md.push_str(&format!("\n{detail}\n"));
        }

        // Why it exists: citations into the governed memories, or the decree.
        let prov = brainiac_store::library::provenance(conn, r.id).await?;
        let memories: Vec<Uuid> = prov
            .iter()
            .filter(|p| p.kind == brainiac_core::StandardProvenanceKind::Memory)
            .map(|p| p.ref_id)
            .collect();
        if !memories.is_empty() {
            let marks: Vec<String> = memories.iter().map(|id| format!("[m:{id}]")).collect();
            md.push_str(&format!("\n<sub>why: {}</sub>\n", marks.join(" ")));
            cited.extend(memories);
        } else if r.decreed_by.is_some() {
            // An evidence-free rule says so, on the page, in the open. A reader
            // is entitled to know which rules rest on judgement alone.
            md.push_str("\n<sub>why: decreed — adopted without prior evidence, signed by a maintainer</sub>\n");
        }
    }

    cited.sort();
    cited.dedup();
    Ok(RenderedStandards {
        markdown: md,
        cited,
    })
}

/// Create a draft standards page for any stack that has earned one and does
/// not have one yet. A DRAFT, like every scaffold: the machine decides a page
/// should exist, a human decides it is right (KB2).
pub async fn scaffold_standards_pages(
    conn: &mut PgConnection,
    org_id: Uuid,
    limit: i64,
) -> Result<Vec<Uuid>> {
    let rows = sqlx::query(
        "SELECT s.stack, count(*) AS rules
         FROM standards s
         WHERE s.org_id = $1 AND s.lifecycle = 'adopted'
           AND NOT EXISTS (
                 SELECT 1 FROM documents d
                 WHERE d.org_id = $1 AND d.doc_kind = 'standards_page'
                   AND d.slug = 'standards-' || s.stack
               )
         GROUP BY s.stack
         HAVING count(*) >= $2
         ORDER BY count(*) DESC
         LIMIT $3",
    )
    .bind(org_id)
    .bind(SCAFFOLD_MIN_RULES)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;

    let mut made = Vec::new();
    for row in rows {
        let stack: String = row.get("stack");
        let id = Uuid::new_v4();
        brainiac_store::documents::insert_document(
            conn,
            &brainiac_store::documents::NewDocument {
                id,
                org_id,
                team_id: None,
                slug: format!("standards-{stack}"),
                title: format!("{stack} standards"),
                // Org-visible: a standard is the org's shared judgment, and
                // this is the tier that may publish externally (KB-PLAN D5).
                visibility: brainiac_core::Visibility::Org,
                doc_kind: brainiac_core::DocKind::StandardsPage,
            },
        )
        .await?;
        // One composed section carrying the stack binding. The projector reads
        // `binding.stack`; there is no query and no model.
        brainiac_store::documents::insert_section(
            conn,
            &brainiac_store::documents::NewSection {
                id: Uuid::new_v4(),
                document_id: id,
                org_id,
                position: 0,
                heading: "The rules".into(),
                mode: brainiac_core::SectionMode::Composed,
                binding: Some(brainiac_core::SectionBinding {
                    stack: Some(stack.clone()),
                    ..Default::default()
                }),
                pinned_content: None,
            },
        )
        .await?;
        // Born dirty: a page with no revision must compose before it is worth
        // reviewing.
        brainiac_store::documents::mark_dirty(conn, id).await?;
        made.push(id);
    }
    Ok(made)
}
