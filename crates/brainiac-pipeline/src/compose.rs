//! Compose stage: canonical memories → a document revision (ARCHITECTURE.md §8).
//!
//! The whole design rests on one inversion. A normal wiki asks a human to
//! remember to update a page when the world changes. This asks the *page* to be
//! nothing but a query over what the org currently believes — so when the belief
//! changes, the page has already changed. Nothing here is a source of truth.
//!
//! Three firewalls make that safe enough to auto-publish, and each one exists
//! because the alternative is a specific, known failure:
//!
//! 1. **Visibility cap** — the memories offered to the model are filtered to
//!    what the *page's* audience may see, before the prompt is built. A
//!    team-private fact must be unable to reach an org page even if the model
//!    tries. (EVAL §2.6 treats a leak as a build failure, not a score.)
//! 2. **Citation firewall** — the model may only cite `[m:uuid]`s from the set
//!    it was handed. An invented id is stripped, and a paragraph left with no
//!    valid citation is an unbacked claim. Both force review; neither can
//!    auto-publish.
//! 3. **Deterministic evidence** — `detail_md` artifacts (code, config, tables)
//!    are appended by US, verbatim from the memory, never re-typed by the model.
//!    A model that paraphrases a config snippet has produced a plausible lie;
//!    copying is not a task worth an LLM's discretion.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{
    DocKind, Document, DocumentSection, Lifecycle, Memory, MemoryKind, MemoryStatus,
    RevisionPolicy, SectionBinding, SectionMode, Visibility,
};
use brainiac_gateway::{ChatProvider, ChatRequest};
use brainiac_store::retrieval::{RetrievalFilters, RetrievalRequest};
use sqlx::PgConnection;
use std::collections::HashSet;
use uuid::Uuid;

/// Versioned prompt. The rules are ordered by what actually goes wrong: models
/// invent citations, then they smuggle in world knowledge, then they hedge.
pub const COMPOSE_SYSTEM_PROMPT_V1: &str = "\
You write one section of an engineering knowledge-base page from the organization's own memories.

You are given numbered MEMORIES, each with an id. Write the section body in markdown.

ABSOLUTE RULES:
- Every factual sentence MUST cite the memory it came from, inline, as [m:<id>] at the end of the sentence.
- You may ONLY state what the memories state. No outside knowledge, no plausible filler, no advice.
- If a memory is marked NOT-YET-SHIPPED, say so explicitly in the sentence (e.g. \"planned:\" / \"not yet in production\").
- If the memories do not support a section at all, output exactly: (no knowledge captured yet)
- Do NOT invent memory ids. Do NOT reproduce code/config blocks — they are attached separately.
- No preamble, no heading (the heading is added for you), no closing summary. Body only.

Write densely and plainly, like an engineer briefing a colleague who is about to touch this system.";

/// What one composed section produced.
struct ComposedSection {
    markdown: String,
    cited: Vec<Uuid>,
    /// A paragraph with no valid citation, or a citation the model invented.
    /// Either one blocks auto-publish.
    unbacked: bool,
}

pub struct ComposeOutcome {
    pub content_md: String,
    /// The provenance closure — exactly the memories this markdown rests on.
    pub composed_from: Vec<Uuid>,
    pub policy: RevisionPolicy,
    /// Why the policy landed where it did — surfaced in the review queue so a
    /// maintainer knows what to look at instead of re-reading the whole page.
    pub policy_reason: String,
    pub model_ref: Option<String>,
    /// What caused this revision: `memory_change` | `manual` | `schedule`.
    pub trigger: String,
}

/// The visibility firewall. A memory may enter a page only if the page's
/// audience is entitled to it — checked in code, on top of RLS, because this is
/// the invariant we cannot afford to get wrong exactly once.
fn admits(doc: &Document, m: &Memory) -> bool {
    // The project cap (PROJECT-PLAN PR4). A project-stamped page composes from
    // its OWN project's memories plus org-shared ones (the inclusive lens);
    // another project's claim on a shared entity must never leak into it. An
    // unstamped page has no project constraint — every page that existed
    // before PR4 composes exactly as it always did.
    let project_ok = match doc.project_id {
        None => true,
        Some(p) => m.project_id.is_none() || m.project_id == Some(p),
    };
    if !project_ok {
        return false;
    }
    match doc.visibility {
        // An org page may carry only org-wide knowledge.
        Visibility::Org => m.visibility == Visibility::Org,
        // A team page may carry org knowledge plus its OWN team's knowledge —
        // never another team's.
        Visibility::Team => {
            m.visibility == Visibility::Org
                || (m.visibility == Visibility::Team && m.team_id == doc.team_id)
        }
        // Private pages are not a product concept; compose nothing rather than
        // guess at an audience.
        Visibility::Private => false,
    }
}

/// Pull the memories a composed section binds to.
///
/// Canonical-only, always: a page is what the org has *signed*, not what it is
/// still arguing about. Candidates live in the review queue, not the wiki.
async fn bound_memories(
    conn: &mut PgConnection,
    pool: &sqlx::PgPool,
    embedder: &dyn Embedder,
    embedding_version: i32,
    doc: &Document,
    binding: &SectionBinding,
) -> Result<Vec<Memory>> {
    let mut out: Vec<Memory> = Vec::new();

    // Entity-anchored bindings are the backbone of an entity_page: they don't
    // depend on phrasing the way a search query does.
    if !binding.entities.is_empty() {
        // A binding may name either a RAW entity (a team's own node) or a
        // CANONICAL one (the merged hub — what auto-scaffolding binds to).
        // Memories anchor to raw entities, so a canonical id must be expanded
        // through entity_links or an entity page would compose to nothing —
        // silently, which is the worst way for it to fail.
        let anchors = expand_entity_anchors(conn, &binding.entities).await?;
        let mems =
            brainiac_store::memories::for_entities(conn, &anchors, (binding.max_items * 3) as i64)
                .await?;
        out.extend(mems);
    }

    // A time-windowed binding (digests, migration 0027): the org's recently
    // changed canonical memories, newest first. A SOURCE like the other two —
    // the filter chain below still applies, so visibility/kind/lifecycle rules
    // hold for a digest exactly as for any page.
    if let Some(days) = binding.window_days {
        let mems =
            brainiac_store::memories::recent_canonical(conn, days, (binding.max_items * 3) as i64)
                .await?;
        out.extend(mems);
    }

    // A free-text binding adds topical memories the entity graph would miss.
    if !binding.query.trim().is_empty() {
        let req = RetrievalRequest {
            query: binding.query.clone(),
            k: binding.max_items * 2,
            as_of: None,
            filters: RetrievalFilters {
                kinds: binding.kinds.clone(),
                min_status: Some(MemoryStatus::Canonical),
                ..Default::default()
            },
        };
        let hits = brainiac_store::retrieval::search(conn, pool, embedder, embedding_version, &req)
            .await?;
        out.extend(hits.into_iter().map(|h| h.memory));
    }

    // Filter, dedupe, cap. Order matters: visibility first, so a memory that
    // must not be here cannot survive any later step.
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut kept: Vec<Memory> = Vec::new();
    for m in out {
        if !seen.insert(m.id) {
            continue;
        }
        if !admits(doc, &m) {
            continue;
        }
        if m.status != MemoryStatus::Canonical {
            continue;
        }
        // Superseded/expired beliefs are exactly what a rotting wiki serves.
        if m.superseded_by.is_some() {
            continue;
        }
        if !binding.kinds.is_empty() && !binding.kinds.contains(&m.kind) {
            continue;
        }
        if !binding.lifecycle.is_empty() && !binding.lifecycle.contains(&m.lifecycle) {
            continue;
        }
        kept.push(m);
    }
    kept.truncate(binding.max_items);
    Ok(kept)
}

/// Resolve a binding's entity ids to the RAW entity ids memories actually anchor
/// to: pass raw ids through, and expand any canonical id to every raw entity
/// linked under it. Cheap, and it makes a binding tolerant of which kind of id
/// the author (human or scaffolder) happened to have.
async fn expand_entity_anchors(conn: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<Uuid>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT entity_id FROM entity_links WHERE canonical_id = ANY($1)
         UNION
         SELECT id FROM entities WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(|r| r.get("entity_id")).collect())
}

/// Render the memory set the model is allowed to use. Lifecycle is stated in
/// the prompt, not implied — the model cannot mark what it was not told.
fn render_memories(mems: &[Memory]) -> String {
    let mut s = String::new();
    for m in mems {
        let life = match m.lifecycle {
            Lifecycle::Shipped => "",
            Lifecycle::InFlight => " [NOT-YET-SHIPPED: decided, in progress]",
            Lifecycle::Proposed => " [NOT-YET-SHIPPED: proposed only]",
        };
        s.push_str(&format!(
            "- id={} kind={}{} :: {}\n",
            m.id,
            m.kind.as_str(),
            life,
            m.content
        ));
    }
    s
}

/// The citation firewall: drop citations the model invented, and report whether
/// any prose paragraph is left standing on nothing.
///
/// "Unbacked" is deliberately coarse — a paragraph of prose with no surviving
/// citation. It cannot catch a sentence that cites a real memory while saying
/// something that memory does not support (that is what the `docs` eval's
/// LLM-judged hallucination metric is for). It catches the cheap, common
/// failure, and it never auto-publishes what it cannot vouch for.
fn enforce_citations(md: &str, allowed: &HashSet<Uuid>) -> (String, Vec<Uuid>, bool) {
    let mut cleaned = String::with_capacity(md.len());
    let mut cited: Vec<Uuid> = Vec::new();
    let mut invented = false;

    let mut rest = md;
    while let Some(start) = rest.find("[m:") {
        let after = &rest[start + 3..];
        let Some(end) = after.find(']') else {
            break;
        };
        cleaned.push_str(&rest[..start]);
        let raw = after[..end].trim();
        match raw.parse::<Uuid>() {
            Ok(id) if allowed.contains(&id) => {
                cleaned.push_str(&format!("[m:{id}]"));
                if !cited.contains(&id) {
                    cited.push(id);
                }
            }
            // An id the model made up, or one it was never given. Strip the
            // marker — leaving it would make an unbacked claim LOOK sourced,
            // which is worse than an obviously unsourced one.
            _ => invented = true,
        }
        rest = &after[end + 1..];
    }
    cleaned.push_str(rest);

    // Any prose paragraph with no citation left is an unbacked claim. Headings,
    // list scaffolding, code fences and the explicit empty marker don't count.
    let mut unbacked = invented;
    for para in cleaned.split("\n\n") {
        let p = para.trim();
        if p.is_empty()
            || p.starts_with('#')
            || p.starts_with("```")
            || p.starts_with('|')
            || p == "(no knowledge captured yet)"
        {
            continue;
        }
        if !p.contains("[m:") {
            unbacked = true;
        }
    }
    (cleaned, cited, unbacked)
}

/// Append the artifacts (`detail_md`) of the memories this section actually
/// cited — copied verbatim from the corpus, never regenerated. This is where
/// KB-PLAN D3 pays off: the reader gets the real config, not a paraphrase of it.
fn evidence_blocks(mems: &[Memory], cited: &[Uuid]) -> String {
    let mut s = String::new();
    for m in mems {
        if !cited.contains(&m.id) {
            continue;
        }
        let Some(detail) = m.detail_md.as_ref() else {
            continue;
        };
        let detail = detail.trim();
        if detail.is_empty() {
            continue;
        }
        s.push_str(&format!("\n{detail}\n\n<sub>[m:{}]</sub>\n", m.id));
    }
    s
}

#[allow(clippy::too_many_arguments)]
async fn compose_section(
    conn: &mut PgConnection,
    pool: &sqlx::PgPool,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    doc: &Document,
    section: &DocumentSection,
    model_ref: &mut Option<String>,
) -> Result<ComposedSection> {
    // Pinned prose is human-owned. It passes through untouched — byte-identical
    // across every regeneration, forever. The eval gates on exactly that.
    if section.mode == SectionMode::Pinned {
        return Ok(ComposedSection {
            markdown: section.pinned_content.clone().unwrap_or_default(),
            cited: Vec::new(),
            unbacked: false,
        });
    }

    let binding = section
        .binding
        .clone()
        .context("composed section without a binding (schema check should prevent this)")?;

    // A standards section is PROJECTED, not composed (LIBRARY-PLAN L8). The
    // model is never called: a rule's statement is a sentence a named human
    // ratified, and re-wording it would fork the org's own commitment. The
    // render is deterministic, so a revision diff means "the org's judgment
    // changed" — never "the model phrased it differently today".
    if let Some(stack) = binding.stack.as_deref() {
        let rendered = crate::standards_page::render_stack(conn, stack).await?;
        return Ok(ComposedSection {
            markdown: rendered.markdown,
            cited: rendered.cited,
            // Nothing to police: every sentence came from the Library, not a
            // model. There is no such thing as an unbacked claim here.
            unbacked: false,
        });
    }

    let mems = bound_memories(conn, pool, embedder, embedding_version, doc, &binding).await?;

    if mems.is_empty() {
        // An honest empty section beats invented filler. It also tells the org
        // something true: nobody has captured knowledge here yet.
        return Ok(ComposedSection {
            markdown: "(no knowledge captured yet)".into(),
            cited: Vec::new(),
            unbacked: false,
        });
    }

    let user = format!(
        "PAGE: {}\nSECTION: {}\n\nMEMORIES:\n{}",
        doc.title,
        section.heading,
        render_memories(&mems)
    );
    let resp = provider
        .complete(&ChatRequest {
            system: COMPOSE_SYSTEM_PROMPT_V1.to_string(),
            user,
            json_mode: false,
            max_tokens: 900,
            temperature: 0.0,
        })
        .await?;
    if model_ref.is_none() {
        *model_ref = Some(resp.model_ref.clone());
    }

    let allowed: HashSet<Uuid> = mems.iter().map(|m| m.id).collect();
    let (clean, cited, unbacked) = enforce_citations(resp.text.trim(), &allowed);
    let markdown = format!("{clean}{}", evidence_blocks(&mems, &cited));

    Ok(ComposedSection {
        markdown,
        cited,
        unbacked,
    })
}

/// The first rung of the diagrams ladder (KB-PLAN D9 / follow-up #1a): a
/// DETERMINISTIC mermaid neighborhood for entity pages, compiled from the
/// entity/edge graph by code. No model proposes an edge — every arrow IS a row
/// in `edges` — so the zero-hallucination bar that defers LLM-authored diagrams
/// is met by construction. Renders `None` when the entity has no edges: an
/// empty diagram would be decoration, and D9's whole point is that diagrams are
/// language here, not decoration.
///
/// Visibility: edges and entity NAMES are org-level rows by schema (RLS scopes
/// them to the org; the visibility ladder lives on memories), so an org-visible
/// entity page may render them. No memory content enters the diagram.
async fn mermaid_neighborhood(conn: &mut PgConnection, doc: &Document) -> Result<Option<String>> {
    use sqlx::Row;
    if doc.doc_kind != DocKind::EntityPage {
        return Ok(None);
    }
    // Scaffolded entity pages carry their anchor in the slug (`entity-{uuid}`).
    // A hand-made page with a different slug simply gets no diagram.
    let Some(canonical_id) = doc
        .slug
        .strip_prefix("entity-")
        .and_then(|s| s.parse::<Uuid>().ok())
    else {
        return Ok(None);
    };
    let center: Option<String> = sqlx::query("SELECT name FROM canonical_entities WHERE id = $1")
        .bind(canonical_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|r| r.get("name"));
    let Some(center) = center else {
        return Ok(None);
    };
    let rows = sqlx::query(
        "WITH mine AS (SELECT entity_id FROM entity_links WHERE canonical_id = $1)
         SELECT DISTINCT e.relation,
                (e.src_entity IN (SELECT entity_id FROM mine)) AS outgoing,
                CASE WHEN e.src_entity IN (SELECT entity_id FROM mine)
                     THEN dst.name ELSE src.name END AS neighbor
         FROM edges e
         JOIN entities src ON src.id = e.src_entity
         JOIN entities dst ON dst.id = e.dst_entity
         WHERE (e.src_entity IN (SELECT entity_id FROM mine)
             OR e.dst_entity IN (SELECT entity_id FROM mine))
           AND (e.valid_to IS NULL OR e.valid_to > now())
         ORDER BY neighbor, e.relation
         LIMIT 24",
    )
    .bind(canonical_id)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let edges: Vec<(String, String, bool)> = rows
        .iter()
        .map(|r| (r.get("neighbor"), r.get("relation"), r.get("outgoing")))
        .collect();
    Ok(Some(render_mermaid(&center, &edges)))
}

/// Pure renderer: mermaid `graph LR` with opaque node ids and quoted labels, so
/// entity names never have to be valid mermaid identifiers. `edges` are
/// (neighbor name, relation, outgoing?) with the page's entity as the anchor.
fn render_mermaid(center: &str, edges: &[(String, String, bool)]) -> String {
    // Mermaid quoted labels break on double quotes; nothing else needs escaping.
    let label = |s: &str| s.replace('"', "'");
    let mut out = String::from("```mermaid\ngraph LR\n");
    out.push_str(&format!("  n0[\"{}\"]\n", label(center)));
    let mut node_of: Vec<String> = Vec::new();
    let mut lines = Vec::new();
    for (neighbor, relation, outgoing) in edges {
        let idx = match node_of.iter().position(|n| n == neighbor) {
            Some(i) => i,
            None => {
                node_of.push(neighbor.clone());
                out.push_str(&format!("  n{}[\"{}\"]\n", node_of.len(), label(neighbor)));
                node_of.len() - 1
            }
        };
        let rel = label(relation);
        lines.push(if *outgoing {
            format!("  n0 -->|{rel}| n{}\n", idx + 1)
        } else {
            format!("  n{} -->|{rel}| n0\n", idx + 1)
        });
    }
    for l in lines {
        out.push_str(&l);
    }
    out.push_str("```\n");
    out
}

/// Compose every section of a page into one revision, and decide whether it may
/// publish itself.
///
/// The caller MUST pass a transaction scoped to a principal that can see no more
/// than the page's audience (see `compose_principal`); [`admits`] is the second
/// line of that defence, not the first.
pub async fn compose_document(
    conn: &mut PgConnection,
    pool: &sqlx::PgPool,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    doc: &Document,
    trigger: &str,
) -> Result<ComposeOutcome> {
    let sections = brainiac_store::documents::sections(conn, doc.id).await?;
    let previous = brainiac_store::documents::current_revision(conn, doc.id).await?;

    let mut body = String::new();
    let mut composed_from: Vec<Uuid> = Vec::new();
    let mut any_unbacked = false;
    let mut model_ref: Option<String> = None;

    body.push_str(&format!("# {}\n", doc.title));

    for section in &sections {
        let out = compose_section(
            conn,
            pool,
            provider,
            embedder,
            embedding_version,
            doc,
            section,
            &mut model_ref,
        )
        .await?;
        body.push_str(&format!("\n## {}\n\n{}\n", section.heading, out.markdown));
        for id in out.cited {
            if !composed_from.contains(&id) {
                composed_from.push(id);
            }
        }
        any_unbacked |= out.unbacked;
    }

    // Deterministic diagram, appended by CODE after the model's sections — the
    // same trust boundary as `evidence_blocks`. It lives inside a fenced block,
    // so neither the citation firewall nor the eval's prose scan mistakes it
    // for an uncited claim.
    if let Some(diagram) = mermaid_neighborhood(conn, doc).await? {
        body.push_str(&format!("\n## Neighborhood\n\n{diagram}"));
    }

    // ── policy (ARCHITECTURE §8.2) ──────────────────────────────────────
    // Auto-publish is a privilege a revision earns. It is granted only when the
    // page is an INCREMENT on something a human already blessed and every claim
    // is traceable. Everything else is a maintainer's call.
    let (policy, reason) = if any_unbacked {
        (
            RevisionPolicy::NeedsReview,
            "a claim on this page is not backed by a cited memory".to_string(),
        )
    } else {
        match &previous {
            // First publication of a page is structurally new by definition —
            // a human names it into existence. Nothing auto-publishes itself
            // into a wiki from nothing.
            None => (
                RevisionPolicy::NeedsReview,
                "first revision of this page — a human publishes it into existence".to_string(),
            ),
            Some(prev) => {
                let dropped: Vec<Uuid> = prev
                    .composed_from
                    .iter()
                    .copied()
                    .filter(|id| !composed_from.contains(id))
                    .collect();
                if dropped.is_empty() {
                    (
                        RevisionPolicy::AutoPublished,
                        "additive: every previously published claim survives and all claims are cited"
                            .to_string(),
                    )
                } else if doc.doc_kind == DocKind::Digest {
                    // The one kind where dropped claims ARE the design: a
                    // digest is a WINDOW, not an account. An item leaving it
                    // is time passing, not a belief being retracted — the
                    // belief still stands in the corpus and on its real pages.
                    // Requiring a human to re-sign the digest every time the
                    // week rolled would train them to rubber-stamp it, which
                    // is worse than no gate. Unbacked claims still force
                    // review (checked above), like everywhere else.
                    (
                        RevisionPolicy::AutoPublished,
                        format!(
                            "digest window rolled: {} item(s) aged out; every current claim is cited",
                            dropped.len()
                        ),
                    )
                } else {
                    // A claim the org had published has disappeared. That is
                    // either a supersession working exactly as designed, or the
                    // retrieval silently losing knowledge. A human tells them
                    // apart; the machine must not guess.
                    (
                        RevisionPolicy::NeedsReview,
                        format!(
                            "{} previously published claim(s) no longer appear on this page",
                            dropped.len()
                        ),
                    )
                }
            }
        }
    };

    Ok(ComposeOutcome {
        content_md: body,
        composed_from,
        policy,
        policy_reason: reason,
        model_ref,
        trigger: trigger.to_string(),
    })
}

// ── entity-page auto-scaffolding (§8.4) ─────────────────────────────────
//
// "The wiki grows where the knowledge actually is, instead of where someone
// remembered to create a page." A canonical entity that has accumulated real,
// cross-team knowledge is, by definition, something the org keeps needing to
// explain to itself — and nobody ever gets around to writing that page.

/// A canonical entity earns a page when it carries at least this many canonical
/// ORG-VISIBLE memories across at least [`SCAFFOLD_MIN_TEAMS`] teams.
///
/// The thresholds are the whole safety argument. Scaffold too eagerly and the
/// KB fills with stub pages that say nothing — which is how a wiki teaches its
/// readers to stop visiting. Requiring knowledge from two teams is the sharper
/// half of the test: a fact only one team knows is that team's business, but a
/// thing two teams have both had to learn about is precisely what an org-wide
/// page is for.
pub const SCAFFOLD_MIN_MEMORIES: i64 = 4;
pub const SCAFFOLD_MIN_TEAMS: i64 = 2;

/// Every org the compose sweep must visit each tick: any org that has a
/// document (to recompose / publish), a canonical entity (a candidate for
/// entity-page scaffolding), OR adopted standards (a candidate for
/// standards-page scaffolding, LIBRARY-PLAN L8).
///
/// The last clause fixes a real coupling bug. The standards-page scaffold's own
/// trigger — three adopted rules on a stack — is evaluated *inside* this sweep.
/// Without the standards union, an org that adopts rules but has no graph and no
/// pages is never visited, so its standards page never scaffolds: the Library's
/// KB projection silently depended on the Memory graph being populated. A
/// Library-first org (rules before entities) got an empty KB forever. Found by
/// the ChainSonar field test (load/chainsonar/runs/2026-07-16/report.md, F-9).
///
/// Runs on the RLS-bypassing admin pool — it is a cross-org operator query.
pub async fn orgs_with_compose_work(pool: &sqlx::PgPool) -> Result<Vec<Uuid>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT DISTINCT org_id FROM documents
         UNION
         SELECT DISTINCT org_id FROM canonical_entities
         UNION
         SELECT DISTINCT org_id FROM standards WHERE lifecycle = 'adopted'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get("org_id")).collect())
}

/// Create `entity_page`s for canonical entities that have crossed the threshold
/// and don't have one yet. Idempotent: the slug is derived from the entity, and
/// an existing page is skipped rather than duplicated.
///
/// Scaffolds a DRAFT with composed sections. It does not publish — the first
/// revision still needs a human, exactly like every other page. The machine
/// decides *that a page should exist*; a person decides *that it is right*.
pub async fn scaffold_entity_pages(
    conn: &mut PgConnection,
    org_id: Uuid,
    limit: i64,
) -> Result<Vec<Uuid>> {
    use sqlx::Row;

    let rows = sqlx::query(
        "SELECT c.id, c.name,
                count(DISTINCT m.id)      AS memories,
                count(DISTINCT m.team_id) AS teams
         FROM canonical_entities c
         JOIN entity_links l   ON l.canonical_id = c.id
         JOIN memory_entities me ON me.entity_id = l.entity_id
         JOIN memories m       ON m.id = me.memory_id
         WHERE m.status = 'canonical'
           AND m.visibility = 'org'
           AND m.superseded_by IS NULL
           AND NOT EXISTS (
                 SELECT 1 FROM documents d
                 WHERE d.doc_kind = 'entity_page' AND d.slug = 'entity-' || c.id::text
               )
         GROUP BY c.id, c.name
         HAVING count(DISTINCT m.id) >= $1 AND count(DISTINCT m.team_id) >= $2
         ORDER BY count(DISTINCT m.id) DESC
         LIMIT $3",
    )
    .bind(SCAFFOLD_MIN_MEMORIES)
    .bind(SCAFFOLD_MIN_TEAMS)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;

    let mut created = Vec::new();
    for r in rows {
        let canonical_id: Uuid = r.get("id");
        let name: String = r.get("name");
        let doc_id = Uuid::new_v4();

        brainiac_store::documents::insert_document(
            conn,
            &brainiac_store::documents::NewDocument {
                id: doc_id,
                org_id,
                // No owning team: the page exists precisely BECAUSE the
                // knowledge crosses team lines. Publishing it is any
                // maintainer's call.
                team_id: None,
                slug: format!("entity-{canonical_id}"),
                title: name.clone(),
                visibility: Visibility::Org,
                doc_kind: brainiac_core::DocKind::EntityPage,
                // Auto-scaffolded pages are org-wide; project pages are authored (PR4).
                project_id: None,
            },
        )
        .await?;

        // The sections encode the questions an engineer actually arrives with,
        // in the order they ask them — and the lifecycle split (KB-PLAN D2)
        // keeps "how it works" from quietly absorbing "how we intend it to
        // work", which is the most common way a wiki starts lying.
        let sections = [
            (
                "How it works today",
                vec![Lifecycle::Shipped],
                vec![MemoryKind::Fact, MemoryKind::Decision, MemoryKind::Pattern],
            ),
            (
                "Pitfalls",
                vec![Lifecycle::Shipped],
                vec![MemoryKind::Pitfall],
            ),
            (
                "How to work with it",
                vec![Lifecycle::Shipped],
                vec![MemoryKind::Howto],
            ),
            (
                "On its way (not yet shipped)",
                vec![Lifecycle::InFlight, Lifecycle::Proposed],
                vec![],
            ),
        ];
        for (i, (heading, lifecycle, kinds)) in sections.into_iter().enumerate() {
            brainiac_store::documents::insert_section(
                conn,
                &brainiac_store::documents::NewSection {
                    id: Uuid::new_v4(),
                    document_id: doc_id,
                    org_id,
                    position: i as i32,
                    heading: heading.to_string(),
                    mode: SectionMode::Composed,
                    binding: Some(SectionBinding {
                        entities: vec![canonical_id],
                        kinds,
                        lifecycle,
                        query: name.clone(),
                        window_days: None,
                        stack: None,
                        max_items: 10,
                    }),
                    pinned_content: None,
                },
            )
            .await?;
        }
        brainiac_store::documents::mark_dirty(conn, doc_id).await?;
        created.push(doc_id);
        tracing::info!(entity = %name, document = %doc_id, "entity page scaffolded");
    }
    Ok(created)
}

// ── the proactive digest (KB-PLAN follow-up #3, UAT P1.5) ───────────────────
//
// The design note that shaped this: a digest is ALSO A PROJECTION. "What
// changed this week" is a page whose binding is a time window, recomposed on
// cadence by the same worker, reviewed through the same gate, and read through
// the same `doc_get` an agent already has — session-start push is the agent
// reading `digest-weekly` when it boots. No parallel generator to rot.

/// The digest's window, and the activity floor that earns one. A digest over a
/// quiet corpus would read "(no knowledge captured yet)" every week — noise
/// that teaches readers to skip it, the same failure mode the entity-page
/// thresholds exist to prevent.
pub const DIGEST_WINDOW_DAYS: i64 = 7;
pub const DIGEST_MIN_RECENT: i64 = 3;
/// Recompose cadence: how stale the newest revision may get before the sweep
/// re-dirties the page. A day keeps a weekly window honest without composing
/// on every tick.
pub const DIGEST_REFRESH_SECS: i64 = 24 * 3600;

/// The org's digest slug — one weekly digest per org, idempotent by slug.
pub const DIGEST_SLUG: &str = "digest-weekly";

/// Create the org's weekly digest page if the corpus has earned one and it does
/// not exist yet. Like entity scaffolding: the machine decides that the page
/// should exist, a human still signs its first revision into existence.
pub async fn scaffold_digest(conn: &mut PgConnection, org_id: Uuid) -> Result<Option<Uuid>> {
    use sqlx::Row;
    let exists = sqlx::query("SELECT 1 FROM documents WHERE org_id = $1 AND slug = $2")
        .bind(org_id)
        .bind(DIGEST_SLUG)
        .fetch_optional(&mut *conn)
        .await?
        .is_some();
    if exists {
        return Ok(None);
    }
    // Only org-visible changes count toward the floor: that is all an org
    // digest may show, so team-private churn must not earn a page that would
    // then compose empty.
    let recent: i64 = sqlx::query(
        "SELECT count(*) AS n FROM memories
         WHERE org_id = $1 AND status = 'canonical' AND visibility = 'org'
           AND superseded_by IS NULL AND deleted_at IS NULL
           AND updated_at > now() - make_interval(days => $2::int)",
    )
    .bind(org_id)
    .bind(DIGEST_WINDOW_DAYS)
    .fetch_one(&mut *conn)
    .await?
    .get("n");
    if recent < DIGEST_MIN_RECENT {
        return Ok(None);
    }

    let doc_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        conn,
        &brainiac_store::documents::NewDocument {
            id: doc_id,
            org_id,
            team_id: None,
            slug: DIGEST_SLUG.into(),
            title: "This week in the knowledge base".into(),
            visibility: Visibility::Org,
            doc_kind: DocKind::Digest,
            // Auto-scaffolded pages are org-wide; project pages are authored (PR4).
            project_id: None,
        },
    )
    .await?;
    brainiac_store::documents::insert_section(
        conn,
        &brainiac_store::documents::NewSection {
            id: Uuid::new_v4(),
            document_id: doc_id,
            org_id,
            position: 0,
            heading: "What changed this week".into(),
            mode: SectionMode::Composed,
            binding: Some(SectionBinding {
                window_days: Some(DIGEST_WINDOW_DAYS),
                max_items: 12,
                ..Default::default()
            }),
            pinned_content: None,
        },
    )
    .await?;
    brainiac_store::documents::mark_dirty(conn, doc_id).await?;
    tracing::info!(org = %org_id, document = %doc_id, "weekly digest scaffolded");
    Ok(Some(doc_id))
}

/// Re-dirty digest pages whose newest revision has aged past the refresh
/// cadence. A windowed page goes stale by TIME PASSING — no memory-change
/// trigger will ever fire for an item silently aging out of the window, so the
/// sweep has to supply the tick. Returns how many pages were marked.
pub async fn refresh_digests(conn: &mut PgConnection, org_id: Uuid) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE documents d
         SET dirty_at = COALESCE(dirty_at, now()), updated_at = now()
         WHERE d.org_id = $1 AND d.doc_kind = 'digest' AND d.dirty_at IS NULL
           AND NOT EXISTS (
                 SELECT 1 FROM document_revisions r
                 WHERE r.document_id = d.id
                   AND r.created_at > now() - make_interval(secs => $2::float8)
               )",
    )
    .bind(org_id)
    .bind(DIGEST_REFRESH_SECS as f64)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// The principal composition runs as.
///
/// A synthetic user with NO team memberships: under the `memories_read` RLS
/// policy that yields org-visible memories only, so an org page's composition
/// *physically cannot* read a team-private fact — the same enforcement path a
/// user query takes, not a parallel one we could forget to update.
///
/// Team pages are composed under the worker scope (org + team tiers) and capped
/// in code by [`admits`], because RLS resolves team membership from
/// `team_members`, which a synthetic user is not in.
pub fn compose_principal(org_id: Uuid) -> brainiac_core::Principal {
    brainiac_core::Principal {
        org_id,
        user_id: Uuid::from_bytes(*b"brainiac-compose"),
        team_ids: vec![],
        project_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ── the deterministic mermaid neighborhood (D9 rung a) ──────────────────

    #[test]
    fn mermaid_uses_opaque_ids_and_quoted_labels() {
        // Entity names are user data: spaces, slashes, quotes. They go in
        // labels, never in node identifiers.
        let md = render_mermaid(
            "payments \"core\" service",
            &[
                ("refund-worker".into(), "depends_on".into(), true),
                ("ledger/v2".into(), "writes_to".into(), true),
                ("refund-worker".into(), "alerts".into(), false),
            ],
        );
        assert!(md.starts_with("```mermaid\ngraph LR\n"), "{md}");
        assert!(md.trim_end().ends_with("```"), "{md}");
        assert!(md.contains("n0[\"payments 'core' service\"]"), "{md}");
        // The repeated neighbor gets ONE node and two edges.
        assert_eq!(md.matches("[\"refund-worker\"]").count(), 1, "{md}");
        assert!(md.contains("n0 -->|depends_on| n1"), "{md}");
        assert!(md.contains("n1 -->|alerts| n0"), "{md}");
        assert!(md.contains("n0 -->|writes_to| n2"), "{md}");
    }

    #[test]
    fn mermaid_block_is_invisible_to_the_prose_scan_shape() {
        // The whole diagram lives inside one fence: nothing in it can ever be
        // counted as an uncited claim by anything that skips fenced blocks.
        let md = render_mermaid("a", &[("b".into(), "r".into(), true)]);
        let fences = md.matches("```").count();
        assert_eq!(fences, 2, "one opening and one closing fence: {md}");
    }

    fn mem(id: Uuid, vis: Visibility, team: Option<Uuid>) -> Memory {
        Memory {
            id,
            org_id: Uuid::nil(),
            team_id: team,
            owner_user_id: None,
            visibility: vis,
            status: MemoryStatus::Canonical,
            kind: brainiac_core::MemoryKind::Fact,
            content: "x".into(),
            lifecycle: Lifecycle::Shipped,
            detail_md: None,
            valid_from: None,
            valid_to: None,
            superseded_by: None,
            confidence: None,
            provenance_id: None,
            project_id: None,
            created_at: Utc::now(),
        }
    }

    fn doc(vis: Visibility, team: Option<Uuid>) -> Document {
        Document {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            team_id: team,
            slug: "s".into(),
            title: "T".into(),
            visibility: vis,
            doc_kind: Default::default(),
            status: Default::default(),
            current_revision: None,
            project_id: None,
            dirty_at: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn org_page_refuses_team_private_knowledge() {
        // The invariant EVAL §2.6 treats as a build failure, in one assertion.
        let t = Uuid::new_v4();
        let page = doc(Visibility::Org, None);
        assert!(!admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Team, Some(t))
        ));
        assert!(!admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Private, None)
        ));
        assert!(admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Org, Some(t))
        ));
    }

    #[test]
    fn team_page_takes_its_own_team_and_org_but_not_a_sibling_team() {
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let page = doc(Visibility::Team, Some(mine));
        assert!(admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Team, Some(mine))
        ));
        assert!(!admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Team, Some(theirs))
        ));
        assert!(admits(
            &page,
            &mem(Uuid::new_v4(), Visibility::Org, Some(theirs))
        ));
    }

    /// PROJECT-PLAN PR4: the project cap in one assertion set. A stamped page
    /// takes its own project + org-shared, never a sibling project; an
    /// unstamped page composes exactly as before the dimension existed.
    #[test]
    fn project_page_takes_its_own_project_and_org_shared_never_a_sibling() {
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let with_project = |project: Option<Uuid>| {
            let mut m = mem(Uuid::new_v4(), Visibility::Org, None);
            m.project_id = project;
            m
        };
        let mut page = doc(Visibility::Org, None);
        page.project_id = Some(mine);
        assert!(admits(&page, &with_project(Some(mine))));
        assert!(admits(&page, &with_project(None)), "org-shared always enters");
        assert!(
            !admits(&page, &with_project(Some(theirs))),
            "a sibling project's claim must not leak into this page"
        );

        let unstamped = doc(Visibility::Org, None);
        assert!(
            admits(&unstamped, &with_project(Some(theirs))),
            "an unstamped page has no project constraint (back-compat)"
        );
    }

    #[test]
    fn invented_citations_are_stripped_and_block_auto_publish() {
        let real = Uuid::new_v4();
        let allowed: HashSet<Uuid> = [real].into_iter().collect();
        let md = format!(
            "The retry cap is 30s [m:{real}].\n\nKafka is the bus [m:{}].",
            Uuid::new_v4() // never handed to the model
        );
        let (clean, cited, unbacked) = enforce_citations(&md, &allowed);
        assert_eq!(cited, vec![real]);
        assert!(unbacked, "an invented citation must block auto-publish");
        assert_eq!(
            clean.matches("[m:").count(),
            1,
            "fake marker survived: {clean}"
        );
    }

    #[test]
    fn an_uncited_paragraph_is_unbacked() {
        let real = Uuid::new_v4();
        let allowed: HashSet<Uuid> = [real].into_iter().collect();
        let md = format!("Cited claim [m:{real}].\n\nConfident freestanding claim.");
        let (_, _, unbacked) = enforce_citations(&md, &allowed);
        assert!(unbacked);
    }

    #[test]
    fn a_fully_cited_page_is_clean() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let allowed: HashSet<Uuid> = [a, b].into_iter().collect();
        let md = format!("## H\n\nOne [m:{a}].\n\nTwo [m:{b}].\n\n```yaml\nk: v\n```");
        let (_, cited, unbacked) = enforce_citations(&md, &allowed);
        assert!(!unbacked);
        assert_eq!(cited.len(), 2);
    }

    #[test]
    fn empty_section_marker_is_not_an_unbacked_claim() {
        let (_, cited, unbacked) =
            enforce_citations("(no knowledge captured yet)", &HashSet::new());
        assert!(!unbacked);
        assert!(cited.is_empty());
    }

    #[test]
    fn evidence_is_copied_verbatim_never_regenerated() {
        let id = Uuid::new_v4();
        let mut m = mem(id, Visibility::Org, None);
        m.detail_md = Some("```yaml\nretry:\n  max_backoff: 30s\n```".into());
        let out = evidence_blocks(&[m], &[id]);
        assert!(out.contains("max_backoff: 30s"));
        assert!(out.contains(&format!("[m:{id}]")));
    }

    #[test]
    fn uncited_memories_contribute_no_evidence() {
        let id = Uuid::new_v4();
        let mut m = mem(id, Visibility::Org, None);
        m.detail_md = Some("secret-ish artifact".into());
        assert!(evidence_blocks(&[m], &[]).is_empty());
    }
}
