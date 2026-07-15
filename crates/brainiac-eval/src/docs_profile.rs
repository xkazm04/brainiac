//! The `docs` profile (EVAL.md §2.6) — does a REAL model, composing a REAL page
//! from the Meridian corpus, produce something an org could safely publish?
//!
//! The compose integration tests (`compose_pg.rs`) already pin the deterministic
//! invariants against a mock. They cannot answer the only question that decides
//! whether auto-publish is defensible with a live provider: *what does an actual
//! LLM do when handed the org's memories and told to cite them?* This profile
//! answers it, on five axes, and three of them are gates rather than scores:
//!
//! | metric | why it is scored this way |
//! |---|---|
//! | **leak** | ZERO TOLERANCE, build failure. An org page carrying a team's private runbook is not a quality regression, it is a breach. No coverage number redeems it. |
//! | **pin preservation** | Boolean. The instant a composer "improves" a human's prose, nobody trusts a pinned section again. |
//! | **staleness propagation** | Boolean. This is the product's central claim — supersede a memory and the page fixes itself. If it fails, the wiki rots like every other wiki. |
//! | **hallucination rate** | Rate, gated at 0 for anything AUTO-PUBLISHED. A reviewed revision may carry an unbacked sentence (a human is about to read it); an auto-published one may not. |
//! | **coverage** | Rate. How much of what the org knows actually reached the page. Softly gated — a model that writes tersely is worse, not dangerous. |
//!
//! Leak detection is deliberately belt-and-braces: a forbidden memory fails the
//! run if its id appears in the provenance closure *or* if its content shows up
//! in the prose semantically (a model can leak a fact by paraphrasing it without
//! ever citing it — id-checking alone would call that clean).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{DocKind, SectionBinding, SectionMode, Visibility};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::ProviderRouter;
use brainiac_pipeline::worker;
use brainiac_store::documents::{NewDocument, NewSection};
use brainiac_store::Store;
use serde::{Deserialize, Serialize};

use crate::seed;

/// Cosine floor for "this claim is present in the page" / "this forbidden fact
/// leaked". Same threshold family as the extraction profile: paraphrases of a
/// statement land above it, unrelated statements well below.
pub const MATCH_THRESHOLD: f64 = 0.70;

#[derive(Debug, Clone, Serialize)]
pub struct DocsReport {
    pub fixture_version: String,
    pub embedding_model: String,
    pub provider: String,
    pub match_threshold: f64,
    // ── the gates ────────────────────────────────────────────────────────
    /// Forbidden memories that reached a page. MUST be empty.
    pub leaks: Vec<Leak>,
    /// Pinned sections that did not survive regeneration byte-identically.
    pub pin_violations: Vec<String>,
    /// Pages whose staleness case did not propagate (supersede → dirty →
    /// recompose reflects the new belief).
    pub staleness_failures: Vec<String>,
    /// Sentences in AUTO-PUBLISHED revisions with no citation backing them.
    pub auto_published_hallucinations: usize,
    // ── the scores ───────────────────────────────────────────────────────
    pub claims_required: usize,
    pub claims_covered: usize,
    pub coverage: f64,
    /// Uncited prose sentences / all prose sentences, across every revision.
    pub hallucination_rate: f64,
    /// Sections that must mark their knowledge unshipped and did (KB-PLAN D2).
    pub unshipped_marked: usize,
    pub unshipped_required: usize,
    pub per_document: Vec<DocScore>,
    /// The claims no page covered — the actionable output.
    pub misses: Vec<Miss>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Leak {
    pub document: String,
    pub memory: String,
    /// `provenance` (cited it) or `prose` (paraphrased it without citing).
    pub via: String,
    pub similarity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocScore {
    pub document: String,
    pub policy: String,
    pub claims_required: usize,
    pub claims_covered: usize,
    pub prose_sentences: usize,
    pub uncited_sentences: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Miss {
    pub document: String,
    pub claim: String,
    pub best_similarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsBaseline {
    pub embedding_model: String,
    pub provider: String,
    pub fixture_version: String,
    pub coverage: f64,
    pub hallucination_rate: f64,
}

/// Composition is a generation task, so it inherits the same run-to-run variance
/// as extraction (see `extraction_profile::RATE_DELTA` — recall spanned 0.25–0.54
/// across identical configs). Coverage gets the same wide band. The gates that
/// actually protect the org — leak, pin, staleness, auto-published hallucination
/// — are NOT rates and get no tolerance at all.
const RATE_DELTA: f64 = 0.15;

impl DocsBaseline {
    pub fn from_report(r: &DocsReport) -> Self {
        Self {
            embedding_model: r.embedding_model.clone(),
            provider: r.provider.clone(),
            fixture_version: r.fixture_version.clone(),
            coverage: r.coverage,
            hallucination_rate: r.hallucination_rate,
        }
    }
}

/// Hard gates first (they are absolute), then the soft rate comparison.
/// Empty = pass.
pub fn regression_failures(r: &DocsReport, b: &DocsBaseline) -> Vec<String> {
    let mut f = Vec::new();

    // ── absolute gates: no baseline, no tolerance, no negotiation ────────
    for leak in &r.leaks {
        f.push(format!(
            "LEAK (build failure): forbidden memory {} reached page {} via {} (sim {:.2})",
            leak.memory, leak.document, leak.via, leak.similarity
        ));
    }
    for p in &r.pin_violations {
        f.push(format!(
            "PIN VIOLATION (build failure): human-owned prose was altered on page {p}"
        ));
    }
    for s in &r.staleness_failures {
        f.push(format!(
            "STALENESS (build failure): page {s} did not pick up the superseding belief — \
             the wiki is rotting"
        ));
    }
    if r.auto_published_hallucinations > 0 {
        f.push(format!(
            "HALLUCINATION (build failure): {} uncited claim(s) in AUTO-PUBLISHED revisions — \
             a page published itself while stating something no memory supports",
            r.auto_published_hallucinations
        ));
    }

    // ── soft rates: config-matched comparison against the committed baseline ─
    if r.embedding_model != b.embedding_model || r.provider != b.provider {
        f.push(format!(
            "config mismatch: run={}/{} baseline={}/{} — composition quality is provider-specific; \
             recalibrate instead of comparing across configs",
            r.provider, r.embedding_model, b.provider, b.embedding_model
        ));
        return f;
    }
    if r.coverage < b.coverage - RATE_DELTA {
        f.push(format!(
            "coverage regressed: {:.3} < baseline {:.3} − {:.2}",
            r.coverage, b.coverage, RATE_DELTA
        ));
    }
    if r.hallucination_rate > b.hallucination_rate + RATE_DELTA {
        f.push(format!(
            "hallucination rate regressed: {:.3} > baseline {:.3} + {:.2}",
            r.hallucination_rate, b.hallucination_rate, RATE_DELTA
        ));
    }
    f
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Prose sentences of a composed page: the units a claim the MODEL made can
/// hide in.
///
/// Headings, fenced code, the evidence `<sub>` footers and the explicit
/// empty-section marker are excluded — they are scaffolding we generate, not
/// claims the model made, and counting them would dilute the hallucination rate
/// with text that cannot be wrong.
///
/// PINNED CONTENT IS EXCLUDED TOO, and this is not a technicality. A pinned
/// section is a human's own words; it carries no citations by design. Counting
/// it as "uncited prose" would charge the model for a human's sentence — the
/// first run of this profile did exactly that and reported a 0.133 hallucination
/// rate for a page whose model output was in fact fully cited. A metric that
/// blames the wrong author is worse than no metric: it would have sent someone
/// hunting for a hallucination that never happened.
fn prose_sentences(md: &str, pinned: &[String]) -> Vec<String> {
    let mut md = md.to_string();
    for p in pinned {
        md = md.replace(p.trim(), "");
    }
    prose_sentences_raw(&md)
}

fn prose_sentences_raw(md: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence
            || t.is_empty()
            || t.starts_with('#')
            || t.starts_with("<sub>")
            || t == "(no knowledge captured yet)"
        {
            continue;
        }
        for s in t.split_inclusive(['.', '!', '?']) {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            // A fragment that is nothing but citation tokens belongs to the
            // sentence it trails. "The cap is 30s. [m:abc]" otherwise splits into
            // a claim with no `[m:` (counted UNCITED) plus an 8-char fragment that
            // the length filter drops — so a correctly-cited page tripped the hard
            // auto-published-hallucination gate purely on citation placement.
            if strip_citations(s).trim().is_empty() {
                if let Some(prev) = out.last_mut() {
                    prev.push(' ');
                    prev.push_str(s);
                    continue;
                }
            }
            // A fragment with no letters is punctuation debris, not a claim.
            if s.len() > 15 && s.chars().any(|c| c.is_alphabetic()) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// Every text segment the page RENDERS — the scan set for the leak gate.
///
/// Deliberately NOT `prose_sentences`. That set exists for the *hallucination*
/// metric, where scaffolding must be excluded so the model is not charged for text
/// it did not author. A breach detector has the opposite requirement: a forbidden
/// fact is a leak whether it lands in a sentence, a `#` heading, a fenced code
/// block, an `<sub>` evidence footer, or human-pinned prose. Reusing the prose set
/// left every one of those as a silent evasion path past a gate the module calls
/// "ZERO TOLERANCE … not a quality regression, it is a breach".
///
/// Fences are UNWRAPPED (keep the body, drop the ``` markers) rather than skipped;
/// heading and `<sub>` markers are stripped but their text is kept.
fn leak_scan_segments(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") || t.is_empty() || t == "(no knowledge captured yet)" {
            continue;
        }
        let t = t.trim_start_matches('#').trim();
        let t = t.strip_prefix("<sub>").unwrap_or(t);
        let t = t.strip_suffix("</sub>").unwrap_or(t).trim();
        if t.is_empty() {
            continue;
        }
        for s in t.split_inclusive(['.', '!', '?']) {
            let s = s.trim();
            if s.len() > 15 && s.chars().any(|c| c.is_alphabetic()) {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn strip_citations(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("[m:") {
        out.push_str(&rest[..i]);
        match rest[i..].find(']') {
            Some(j) => rest = &rest[i + j + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Run the `docs` profile: seed the Meridian gold corpus, build each gold page,
/// compose it with the REAL provider, then score.
pub async fn run(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
    providers: &ProviderRouter,
) -> Result<DocsReport> {
    anyhow::ensure!(
        !fx.documents.documents.is_empty(),
        "no composition gold in this fixture tree (documents/pages.yaml) — nothing to score"
    );

    // Seed the corpus (canonical gold memories + entities + embeddings).
    let seeded = seed::seed_gold(store, fx, embedder).await?;
    let org_id = stable_uuid(&fx.org.org);
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // ── build the pages from gold ────────────────────────────────────────
    // The profile re-seeds the tenant on every run (it is DESTRUCTIVE by
    // contract), but `seed_gold` does not know about pages. Drop this org's
    // documents first — sections/revisions/dependencies cascade — so a second
    // run does not collide with the first run's ids, and so we never score a
    // page composed against a corpus that no longer exists.
    let mut tx = store.scoped_tx(&principal).await?;
    sqlx::query("DELETE FROM documents")
        .execute(&mut *tx)
        .await?;
    for d in &fx.documents.documents {
        let doc_id = stable_uuid(&d.id);
        brainiac_store::documents::insert_document(
            &mut tx,
            &NewDocument {
                id: doc_id,
                org_id,
                team_id: Some(stable_uuid(&d.team)),
                slug: d.slug.clone(),
                title: d.title.clone(),
                visibility: Visibility::parse(&d.visibility).unwrap_or(Visibility::Org),
                doc_kind: DocKind::parse(&d.doc_kind).unwrap_or_default(),
            },
        )
        .await?;
        for (i, s) in d.sections.iter().enumerate() {
            let mode = SectionMode::parse(&s.mode).context("section mode")?;
            let binding = s.bindings.as_ref().map(|b| SectionBinding {
                entities: b.entities.iter().map(|e| stable_uuid(e)).collect(),
                kinds: b
                    .kinds
                    .iter()
                    .filter_map(|k| brainiac_core::MemoryKind::parse(k))
                    .collect(),
                lifecycle: b
                    .lifecycle
                    .iter()
                    .filter_map(|l| brainiac_core::Lifecycle::parse(l))
                    .collect(),
                query: b.query.clone(),
                max_items: 12,
            });
            brainiac_store::documents::insert_section(
                &mut tx,
                &NewSection {
                    id: stable_uuid(&format!("{}-s{i}", d.id)),
                    document_id: doc_id,
                    org_id,
                    position: i as i32,
                    heading: s.heading.clone(),
                    mode,
                    binding,
                    pinned_content: s.pinned_content.clone(),
                },
            )
            .await?;
        }
        brainiac_store::documents::mark_dirty(&mut tx, doc_id).await?;
    }
    tx.commit().await?;

    // ── compose with the REAL provider ───────────────────────────────────
    worker::compose_tick(
        store,
        providers,
        embedder,
        seeded.embedding_version,
        org_id,
        100,
    )
    .await?;

    let provider = providers
        .for_stage(brainiac_gateway::Stage::Compose)
        .model_ref();

    let mut report = DocsReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        provider,
        match_threshold: MATCH_THRESHOLD,
        leaks: Vec::new(),
        pin_violations: Vec::new(),
        staleness_failures: Vec::new(),
        auto_published_hallucinations: 0,
        claims_required: 0,
        claims_covered: 0,
        coverage: 0.0,
        hallucination_rate: 0.0,
        unshipped_marked: 0,
        unshipped_required: 0,
        per_document: Vec::new(),
        misses: Vec::new(),
    };

    let mut prose_total = 0usize;
    let mut uncited_total = 0usize;

    for d in &fx.documents.documents {
        let doc_id = stable_uuid(&d.id);
        let mut tx = store.scoped_tx(&principal).await?;
        let rev = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
            .await?
            .into_iter()
            .next()
            .with_context(|| format!("page {} produced no revision", d.id))?;
        tx.commit().await?;

        let md = rev.content_md.clone();
        let pinned: Vec<String> = d
            .sections
            .iter()
            .filter_map(|s| s.pinned_content.clone())
            .collect();
        let sentences = prose_sentences(&md, &pinned);
        let uncited = sentences.iter().filter(|s| !s.contains("[m:")).count();
        prose_total += sentences.len();
        uncited_total += uncited;
        if rev.policy_decision == brainiac_core::RevisionPolicy::AutoPublished {
            report.auto_published_hallucinations += uncited;
        }

        // ── coverage: is each required claim actually on the page? ────────
        let plain: Vec<String> = sentences.iter().map(|s| strip_citations(s)).collect();
        let sent_vecs = if plain.is_empty() {
            Vec::new()
        } else {
            embedder
                .embed_batch(&plain.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .await?
        };

        let mut required = 0usize;
        let mut covered = 0usize;
        for s in &d.sections {
            for claim in &s.must_cover {
                required += 1;
                let cv = embedder.embed(claim).await?;
                let best = sent_vecs
                    .iter()
                    .map(|sv| cosine(&cv, sv))
                    .fold(0.0f64, f64::max);
                if best >= MATCH_THRESHOLD {
                    covered += 1;
                } else {
                    report.misses.push(Miss {
                        document: d.id.clone(),
                        claim: claim.clone(),
                        best_similarity: best,
                    });
                }
            }
            // KB-PLAN D2: not-yet-shipped knowledge must be MARKED, not merely
            // stated. A page that renders a roadmap decision as current
            // architecture is the most common way a wiki lies.
            if s.must_mark_unshipped {
                report.unshipped_required += 1;
                let lower = md.to_lowercase();
                if lower.contains("not yet")
                    || lower.contains("planned")
                    || lower.contains("will ")
                    || lower.contains("in progress")
                {
                    report.unshipped_marked += 1;
                }
            }
        }

        // ── leak gate: id in the closure, OR content paraphrased ANYWHERE ──
        // Scan every segment the page renders — headings, fenced code, <sub>
        // footers and pinned prose included. The hallucination sentence set
        // (`sent_vecs`) deliberately drops all of those, so reusing it here left a
        // forbidden fact restated in a heading or a config snippet completely
        // invisible to a gate that is supposed to be zero-tolerance.
        let leak_segments = leak_scan_segments(&md);
        let leak_plain: Vec<String> = leak_segments.iter().map(|s| strip_citations(s)).collect();
        let leak_vecs = if leak_plain.is_empty() {
            Vec::new()
        } else {
            embedder
                .embed_batch(&leak_plain.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .await?
        };
        for fm in &d.forbidden_memories {
            let fmid = stable_uuid(fm);
            if rev.composed_from.contains(&fmid) {
                report.leaks.push(Leak {
                    document: d.id.clone(),
                    memory: fm.clone(),
                    via: "provenance".into(),
                    similarity: 1.0,
                });
                continue;
            }
            // The subtler leak: the model restates a forbidden fact without
            // citing it. Id-checking alone would call this page clean.
            let Some(gold) = fx.memories.memories.iter().find(|m| &m.id == fm) else {
                continue;
            };
            let gv = embedder.embed(&gold.content).await?;
            let best = leak_vecs
                .iter()
                .map(|sv| cosine(&gv, sv))
                .fold(0.0f64, f64::max);
            if best >= MATCH_THRESHOLD {
                report.leaks.push(Leak {
                    document: d.id.clone(),
                    memory: fm.clone(),
                    via: "rendered".into(),
                    similarity: best,
                });
            }
        }

        // ── pin preservation ─────────────────────────────────────────────
        for s in &d.sections {
            if let Some(pinned) = &s.pinned_content {
                if !md.contains(pinned.trim()) {
                    report.pin_violations.push(d.id.clone());
                }
            }
        }

        report.per_document.push(DocScore {
            document: d.id.clone(),
            policy: rev.policy_decision.as_str().to_string(),
            claims_required: required,
            claims_covered: covered,
            prose_sentences: sentences.len(),
            uncited_sentences: uncited,
        });
        report.claims_required += required;
        report.claims_covered += covered;
    }

    // ── staleness propagation: the product's central claim ───────────────
    for d in &fx.documents.documents {
        let Some(sc) = &d.staleness_case else {
            continue;
        };
        if !sc.expect_dirty {
            continue;
        }
        let doc_id = stable_uuid(&d.id);
        let old = stable_uuid(&sc.supersede.old);
        let new = stable_uuid(&sc.supersede.new);

        // Publish the current revision first: an unpublished page has nothing to
        // go stale, so the test would be vacuous.
        let mut tx = store.scoped_tx(&principal).await?;
        if let Some(rev) = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
            .await?
            .into_iter()
            .next()
        {
            brainiac_store::documents::approve_revision(
                &mut tx,
                rev.id,
                stable_uuid(&fx.org.users[0].id),
                chrono::Utc::now(),
            )
            .await?;
        }
        // A maintainer resolves it. NOBODY TOUCHES THE PAGE.
        brainiac_store::governance::apply_supersession(
            &mut tx,
            org_id,
            old,
            new,
            Some(stable_uuid(&fx.org.users[0].id)),
            "docs-eval-staleness-case",
        )
        .await?;
        tx.commit().await?;

        let stats = worker::compose_tick(
            store,
            providers,
            embedder,
            seeded.embedding_version,
            org_id,
            100,
        )
        .await?;

        let mut tx = store.scoped_tx(&principal).await?;
        let rev = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
            .await?
            .into_iter()
            .next();
        tx.commit().await?;

        let propagated = stats.composed > 0
            && rev
                .as_ref()
                .is_some_and(|r| !r.composed_from.contains(&old));
        if !propagated {
            report.staleness_failures.push(d.id.clone());
        }
    }

    report.coverage = if report.claims_required == 0 {
        1.0
    } else {
        report.claims_covered as f64 / report.claims_required as f64
    };
    report.hallucination_rate = if prose_total == 0 {
        0.0
    } else {
        uncited_total as f64 / prose_total as f64
    };
    report.misses.sort_by(|a, b| {
        a.best_similarity
            .partial_cmp(&b.best_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_sentences_ignore_scaffolding_we_generated() {
        let md = "# Page\n\n## Section\n\nThe retry cap is 30 seconds [m:x].\n\n```yaml\nk: v\n```\n\n<sub>[m:x]</sub>\n";
        let s = prose_sentences(md, &[]);
        assert_eq!(s.len(), 1, "{s:?}");
        assert!(s[0].contains("retry cap"));
    }

    #[test]
    fn empty_section_marker_is_not_a_claim() {
        assert!(prose_sentences("## H\n\n(no knowledge captured yet)\n", &[]).is_empty());
    }

    #[test]
    fn a_citation_after_the_period_still_backs_its_sentence() {
        // The false positive: "…30 seconds. [m:abc]" split into a claim with no
        // `[m:` (counted uncited) and an 8-char citation fragment the length
        // filter dropped — failing the HARD auto-published-hallucination gate on a
        // page that was correctly cited.
        let s = prose_sentences("## H\n\nThe refund retry cap is 30 seconds. [m:abc]\n", &[]);
        assert_eq!(s.len(), 1, "{s:?}");
        assert!(
            s[0].contains("[m:"),
            "a trailing citation must attach to its claim: {s:?}"
        );
        // Multiple trailing citations, and the in-sentence form, both still work.
        let multi = prose_sentences("The cap moved to 45 seconds. [m:a] [m:b]\n", &[]);
        assert_eq!(multi.len(), 1, "{multi:?}");
        assert!(multi[0].contains("[m:a]") && multi[0].contains("[m:b]"), "{multi:?}");
        let inline = prose_sentences("The cap is 30 seconds [m:x] per the runbook.\n", &[]);
        assert!(inline.iter().all(|s| s.contains("[m:")), "{inline:?}");
    }

    #[test]
    fn leak_scan_sees_what_the_prose_set_drops() {
        // The evasion paths: a forbidden fact restated in a heading, inside a
        // fenced code block, or in an <sub> footer. prose_sentences drops all of
        // them (correctly — it exists to not blame the model for scaffolding), so
        // the leak gate must NOT reuse it.
        let md = "# The refund retry cap is 45 seconds\n\n\
                  ```yaml\nretry_cap_seconds: 45 for the refund worker\n```\n\n\
                  <sub>escalate to the payments on-call rotation first</sub>\n";
        assert!(
            prose_sentences(md, &[]).is_empty(),
            "precondition: the prose set drops all of this"
        );
        let scan = leak_scan_segments(md);
        let joined = scan.join(" | ");
        assert!(joined.contains("refund retry cap"), "heading text: {joined}");
        assert!(joined.contains("retry_cap_seconds"), "fence body: {joined}");
        assert!(joined.contains("payments on-call"), "<sub> text: {joined}");
    }

    #[test]
    fn leak_scan_still_ignores_fence_markers_and_filler() {
        let scan = leak_scan_segments("```\n```\n\n(no knowledge captured yet)\n\n#\n");
        assert!(scan.is_empty(), "{scan:?}");
    }

    #[test]
    fn a_humans_pinned_prose_is_not_the_models_hallucination() {
        // The bug the first real run exposed: pinned prose carries no citations
        // by design, and charging the model for it reported a hallucination
        // that never happened.
        const PINNED: &str =
            "Owned by team-payments. Page on #pay-oncall before changing retry behaviour.";
        let md = format!(
            "# P\n\n## What we know\n\nThe cap is 30 seconds [m:x].\n\n## Ownership\n\n{PINNED}\n"
        );
        let with_pin = prose_sentences(&md, &[PINNED.to_string()]);
        assert!(
            with_pin.iter().all(|s| s.contains("[m:")),
            "human prose was counted as an uncited model claim: {with_pin:?}"
        );
        // Without the exclusion it WOULD be counted — this is the regression guard.
        assert!(prose_sentences(&md, &[]).iter().any(|s| !s.contains("[m:")));
    }

    #[test]
    fn strip_citations_leaves_the_claim() {
        assert_eq!(
            strip_citations("The cap is 30s [m:abc] and rising [m:def]."),
            "The cap is 30s  and rising ."
        );
    }

    #[test]
    fn a_leak_fails_the_run_regardless_of_a_perfect_baseline() {
        // The whole point of the gate: no score redeems a breach.
        let r = DocsReport {
            fixture_version: "v1".into(),
            embedding_model: "e".into(),
            provider: "p".into(),
            match_threshold: 0.7,
            leaks: vec![Leak {
                document: "doc-psp-gateway".into(),
                memory: "mem-pay-0065".into(),
                via: "prose".into(),
                similarity: 0.91,
            }],
            pin_violations: vec![],
            staleness_failures: vec![],
            auto_published_hallucinations: 0,
            claims_required: 2,
            claims_covered: 2,
            coverage: 1.0,
            hallucination_rate: 0.0,
            unshipped_marked: 0,
            unshipped_required: 0,
            per_document: vec![],
            misses: vec![],
        };
        let b = DocsBaseline {
            embedding_model: "e".into(),
            provider: "p".into(),
            fixture_version: "v1".into(),
            coverage: 1.0,
            hallucination_rate: 0.0,
        };
        let f = regression_failures(&r, &b);
        assert!(f.iter().any(|m| m.contains("LEAK")), "{f:?}");
    }
}
