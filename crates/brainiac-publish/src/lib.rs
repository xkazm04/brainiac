//! brainiac-publish — pushing the knowledge base OUTWARD (KB3; ARCHITECTURE §8.5).
//!
//! The strategic bet of this crate, in one sentence: a team should not have to
//! abandon the wiki it already reads in order to stop it rotting. So Confluence
//! is not the competitor we replace — it is a render target we keep honest.
//!
//! Four invariants hold for every target, and each one is a refusal:
//!
//! 1. **One-way.** Pages are pushed, never pulled. A published page carries a
//!    generated-content banner and links back to the console. Direct edits in the
//!    external tool are overwritten on the next compose. Harvesting them back is
//!    a later increment (Level 2) — because bidirectional sync would recreate the
//!    two-sources-of-truth problem the whole document layer exists to eliminate.
//!
//! 2. **`org` visibility only.** External publish leaves RLS behind entirely, so
//!    only org-visible pages may be pushed. Team and private knowledge renders in
//!    the console and nowhere else. A leaked team-private runbook in a company
//!    wiki is not a bug report — it is an unrecoverable trust event, and no
//!    feature is worth risking one.
//!
//! 3. **Health-gated.** A degrading corpus stops publishing rather than
//!    broadcasting. See [`brainiac_store::publishing::publish_gate`].
//!
//! 4. **No credentials in the database.** A target stores the NAME of an env var,
//!    never the token. A database dump must never contain a PAT that can write to
//!    a customer's wiki.

pub mod confluence;
pub mod git;
pub mod okf;
pub mod pointer;
pub mod render;

use anyhow::{Context, Result};
use async_trait::async_trait;
use brainiac_core::Document;
use brainiac_store::publishing::{self, PublishTarget};
use brainiac_store::Store;
use uuid::Uuid;

/// A page, rendered and ready to leave the building.
pub struct PageToPublish<'a> {
    pub document: &'a Document,
    /// Markdown INCLUDING the generated-content banner (and the stale stamp, if
    /// the breaker has paused this org).
    pub markdown: &'a str,
    /// The external system's handle from last time, if we have published before.
    pub external_ref: Option<&'a str>,
    /// Structured metadata for targets that emit a machine-readable format
    /// (the OKF target). Plain-markdown targets ignore it.
    pub meta: &'a PageMeta,
}

/// What a machine-readable target needs to know about a page beyond its
/// markdown. Computed once per page by [`publish_org`] — targets never query
/// the store themselves, so every target sees the same facts.
#[derive(Debug, Default, Clone)]
pub struct PageMeta {
    /// One-line summary derived from the revision body (OKF `description`).
    pub description: Option<String>,
    /// Canonical entity names the revision's memories anchor to (OKF `tags`).
    pub tags: Vec<String>,
    /// RFC 3339 time the live revision was published (OKF `timestamp`).
    pub timestamp: Option<String>,
    /// Console URL of the page (OKF `resource` — the URI of the governed asset
    /// this file is a projection of).
    pub resource: Option<String>,
    /// The revision's provenance closure: the exact memory ids it was composed
    /// from. Carried into extension frontmatter — no other wiki format ships
    /// its evidence, and this is where ours travels.
    pub cited_memories: Vec<Uuid>,
    /// How the live revision went live: `auto_published` | `needs_review`.
    pub policy_decision: String,
    /// An underlying memory changed and the page has not recomposed yet.
    pub stale: bool,
}

/// One page's entry in the bundle-level artifacts (OKF `index.md` / `log.md`).
#[derive(Debug, Clone)]
pub struct BundleEntry {
    pub slug: String,
    pub title: String,
    pub doc_kind: String,
    pub description: Option<String>,
    /// Recent revision history, newest first.
    pub log: Vec<LogEntry>,
}

/// One revision, as the bundle changelog tells it.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// ISO 8601 date (YYYY-MM-DD) the revision was composed.
    pub date: String,
    pub page_title: String,
    /// What prompted the recompose: `memory_change` | `manual` | `schedule`.
    pub trigger: String,
    /// `auto_published` | `needs_review` at composition time.
    pub policy: String,
}

/// Everything a target may need after the per-page loop: the whole publishable
/// bundle, in one place, so `index.md` reflects pages that were UNCHANGED this
/// run as faithfully as pages that were pushed.
pub struct BundleToPublish<'a> {
    pub console_url: &'a str,
    pub entries: &'a [BundleEntry],
}

/// One outbound target. Git and Confluence are two implementations; Notion or
/// Backstage would be a third without touching a caller.
#[async_trait]
pub trait Publisher: Send + Sync {
    fn kind(&self) -> &'static str;
    /// Push the page. Returns the external handle to remember (a Confluence page
    /// id, a git path) so the next publish updates rather than duplicates.
    async fn publish(&self, page: &PageToPublish<'_>) -> Result<Option<String>>;
    /// Called once per run, after every page has been offered, with the whole
    /// publishable bundle. Targets that keep bundle-level artifacts — OKF's
    /// `index.md` and `log.md`, the agent pointer files — regenerate them here,
    /// idempotently. Default: nothing, which is right for Confluence.
    async fn finish(&self, _bundle: &BundleToPublish<'_>) -> Result<()> {
        Ok(())
    }
}

/// Build the publisher for a configured target, reading its credential from the
/// env var the target NAMES (never from the database).
pub fn publisher_for(target: &PublishTarget) -> Result<Box<dyn Publisher>> {
    match target.kind.as_str() {
        "git" => Ok(Box::new(git::GitPublisher::from_config(&target.config)?)),
        "okf" => Ok(Box::new(okf::OkfPublisher::from_config(&target.config)?)),
        "confluence" => {
            let secret_ref = target
                .secret_ref
                .as_deref()
                .context("confluence target has no secret_ref naming its PAT env var")?;
            let token = std::env::var(secret_ref).with_context(|| {
                format!("env var `{secret_ref}` (this target's PAT) is not set")
            })?;
            Ok(Box::new(confluence::ConfluencePublisher::from_config(
                &target.config,
                token,
            )?))
        }
        other => anyhow::bail!("unknown publish target kind `{other}`"),
    }
}

#[derive(Debug, Default, Clone)]
pub struct PublishStats {
    pub pushed: usize,
    /// Already live at this revision — nothing to do.
    pub unchanged: usize,
    /// Held back by the health circuit breaker.
    pub blocked: usize,
    /// Not org-visible: it stays in the console, by design.
    pub withheld_visibility: usize,
    pub failed: usize,
}

/// Publish an org's knowledge base to every enabled target.
///
/// Order of checks is the order of consequence: the org must have opted in, the
/// corpus must be healthy enough to be worth broadcasting, and only then does a
/// page's own visibility decide whether it may leave.
pub async fn publish_org(store: &Store, org_id: Uuid, console_url: &str) -> Result<PublishStats> {
    let mut stats = PublishStats::default();
    let principal = brainiac_core::Principal {
        org_id,
        user_id: Uuid::from_bytes(*b"brainiac-publish"),
        team_ids: vec![],
        project_id: None,
    };

    // 1. Did this org ask for any of this? (KB-PLAN D6 — optional, off by default.)
    let mut tx = store.scoped_tx(&principal).await?;
    if !publishing::kb_enabled(&mut tx, org_id).await? {
        return Ok(stats);
    }
    let targets = publishing::enabled_targets(&mut tx).await?;
    if targets.is_empty() {
        return Ok(stats);
    }

    // 2. The circuit breaker. Checked ONCE per org, before any page is rendered:
    //    a corpus that is not fit to publish is not fit to publish page by page.
    let gate = publishing::publish_gate(&mut tx, org_id).await?;
    let docs = brainiac_store::documents::list_documents(&mut tx).await?;
    tx.commit().await?;

    if let Some(reason) = &gate.blocked {
        // Loud, and not once per page: an operator must be able to see this in a
        // log without it drowning them.
        tracing::warn!(
            org = %org_id,
            currency = gate.currency,
            governance = gate.governance,
            reason = %reason,
            "PUBLISHING PAUSED — pages hold their last published revision"
        );
        stats.blocked = docs.len();
        return Ok(stats);
    }

    let mut bundle: Vec<BundleEntry> = Vec::new();
    for doc in &docs {
        // 3. The visibility rule (KB-PLAN D5). Publishing leaves RLS behind, so
        //    only org-wide pages may go — and the compose stage already
        //    guaranteed an org page contains only org-visible memories.
        if doc.visibility != brainiac_core::Visibility::Org {
            stats.withheld_visibility += 1;
            continue;
        }
        // Never publish a draft: only what a named human signed.
        let Some(current_rev) = doc.current_revision else {
            continue;
        };

        let mut tx = store.scoped_tx(&principal).await?;
        let revision = brainiac_store::documents::current_revision(&mut tx, doc.id).await?;
        let Some(revision) = revision else {
            tx.commit().await?;
            continue;
        };

        // Page metadata for machine-readable targets, computed once so every
        // target sees the same facts. Two cheap queries per page; pages are few.
        let history = brainiac_store::documents::revisions(&mut tx, doc.id, 10).await?;
        let tags = brainiac_store::entities::canonical_names_of_memories(
            &mut tx,
            &revision.composed_from,
            16,
        )
        .await?;
        let meta = PageMeta {
            description: render::derive_description(&revision.content_md),
            tags,
            timestamp: Some(
                revision
                    .published_at
                    .unwrap_or(revision.created_at)
                    .to_rfc3339(),
            ),
            resource: Some(render::page_url(console_url, &doc.slug)),
            cited_memories: revision.composed_from.clone(),
            policy_decision: revision.policy_decision.as_str().to_string(),
            stale: doc.dirty_at.is_some(),
        };
        bundle.push(BundleEntry {
            slug: doc.slug.clone(),
            title: doc.title.clone(),
            doc_kind: doc.doc_kind.as_str().to_string(),
            description: meta.description.clone(),
            log: history
                .iter()
                .map(|r| LogEntry {
                    date: r.created_at.format("%Y-%m-%d").to_string(),
                    page_title: doc.title.clone(),
                    trigger: r.trigger.clone(),
                    policy: r.policy_decision.as_str().to_string(),
                })
                .collect(),
        });

        for target in &targets {
            let prior = publishing::publication(&mut tx, doc.id, target.id).await?;
            // Idempotent: the same revision is already live there.
            if prior.as_ref().is_some_and(|p| p.revision_id == current_rev) {
                stats.unchanged += 1;
                continue;
            }

            let published_at = revision
                .published_at
                .unwrap_or(revision.created_at)
                .format("%Y-%m-%d")
                .to_string();
            let markdown = format!(
                "{}{}",
                render::banner_md(console_url, &doc.slug, &published_at),
                revision.content_md
            );

            let publisher = match publisher_for(target) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(target = %target.kind, error = %e, "publish target misconfigured");
                    stats.failed += 1;
                    continue;
                }
            };
            let page = PageToPublish {
                document: doc,
                markdown: &markdown,
                external_ref: prior.as_ref().and_then(|p| p.external_ref.as_deref()),
                meta: &meta,
            };
            match publisher.publish(&page).await {
                Ok(external_ref) => {
                    publishing::record_publication(
                        &mut tx,
                        org_id,
                        doc.id,
                        target.id,
                        current_rev,
                        external_ref.as_deref(),
                    )
                    .await?;
                    stats.pushed += 1;
                    tracing::info!(page = %doc.slug, target = %target.kind, "page published");
                }
                Err(e) => {
                    tracing::error!(page = %doc.slug, target = %target.kind, error = %e, "publish failed");
                    stats.failed += 1;
                }
            }
        }
        tx.commit().await?;
    }

    // Bundle-level artifacts, once per target, after every page was offered:
    // OKF's index.md must list the pages that were UNCHANGED this run too, and
    // regenerating it is idempotent. Skipped entirely when nothing is
    // publishable — an empty index would claim a bundle that does not exist.
    if !bundle.is_empty() {
        let b = BundleToPublish {
            console_url,
            entries: &bundle,
        };
        for target in &targets {
            // A misconfigured target already logged and counted in the loop.
            let Ok(publisher) = publisher_for(target) else {
                continue;
            };
            if let Err(e) = publisher.finish(&b).await {
                tracing::error!(target = %target.kind, error = %e, "bundle artifacts failed");
                stats.failed += 1;
            }
        }
    }
    Ok(stats)
}
