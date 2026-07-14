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
    Document, DocumentSection, Lifecycle, Memory, MemoryStatus, RevisionPolicy, SectionBinding,
    SectionMode, Visibility,
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
        let mems = brainiac_store::memories::for_entities(
            conn,
            &binding.entities,
            (binding.max_items * 3) as i64,
        )
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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
            dirty_at: None,
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
