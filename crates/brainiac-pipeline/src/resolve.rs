//! Resolve stage: raw entity → canonical linking (stage 4 of §3).
//!
//! Blocking by embedding similarity over canonical names, then a BYOM
//! adjudication in the ambiguous band. Thresholds deliberately bias toward
//! the review queue on sparse data (ARCHITECTURE.md §9 risk 3): the
//! zero-tolerance failure is a FALSE merge, not a missed one — an unlinked
//! entity just waits for a human.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_gateway::{ChatProvider, ChatRequest};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Similarity at/above which the name is trusted as the same thing without
/// asking a model (near-verbatim match).
pub const AUTO_LINK_SIMILARITY: f32 = 0.95;
/// Band lower bound: below this we don't even ask — clearly different.
pub const ADJUDICATION_FLOOR: f32 = 0.55;
/// Adjudicator confidence needed for an automatic link.
pub const ADJUDICATION_AUTO_CONFIDENCE: f32 = 0.85;

pub const ADJUDICATE_SYSTEM_PROMPT_V1: &str = "\
You adjudicate whether two names from different engineering teams refer to the SAME real-world thing.
Beware near-misses: a repository is not the model it trains; a v2 feature is not its deprecated v1;
a product is not the team that owns it; infrastructure is not the application code running on it.
Respond with ONLY JSON: {\"same\": true|false, \"confidence\": 0.0}";

#[derive(Debug, Deserialize)]
struct Adjudication {
    same: bool,
    #[serde(default)]
    confidence: f32,
}

#[derive(Debug, PartialEq)]
pub enum ResolveOutcome {
    /// Linked to an existing canonical (auto path).
    Linked {
        canonical_id: Uuid,
        method: &'static str,
    },
    /// Ambiguous — left unlinked for the human review queue.
    NeedsReview { best_candidate: Option<Uuid> },
    /// No plausible candidate — a fresh canonical was bootstrapped.
    NewCanonical { canonical_id: Uuid },
}

/// Fold this raw form (name + captured aliases) into the canonical's alias set,
/// so a canonical merge accumulates every surface form linked into it.
async fn record_aliases(
    conn: &mut PgConnection,
    canonical_id: Uuid,
    entity_name: &str,
    entity_aliases: &[String],
) -> Result<()> {
    let mut all: Vec<String> = Vec::with_capacity(entity_aliases.len() + 1);
    all.push(entity_name.to_string());
    all.extend(entity_aliases.iter().cloned());
    brainiac_store::entities::accumulate_canonical_aliases(conn, canonical_id, &all).await
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_entity(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    entity_id: Uuid,
    entity_name: &str,
    entity_kind: &str,
    entity_aliases: &[String],
) -> Result<ResolveOutcome> {
    // Alias-aware lexical fast-path: an exact hit on a canonical name or a
    // previously-captured alias is unambiguous — link with neither an embedding
    // round-trip nor a model call. This is what makes cross-team acronyms
    // ("PSP" ↔ "psp-gateway") resolve without hand-seeded aliases.
    let surface_forms: Vec<String> = std::iter::once(entity_name.to_string())
        .chain(entity_aliases.iter().cloned())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some((canonical_id, _kind)) =
        brainiac_store::entities::find_canonical_by_name_or_alias(conn, org_id, &surface_forms)
            .await?
    {
        brainiac_store::entities::link(conn, entity_id, canonical_id, 1.0, "alias_lexical", None)
            .await?;
        record_aliases(conn, canonical_id, entity_name, entity_aliases).await?;
        return Ok(ResolveOutcome::Linked {
            canonical_id,
            method: "alias_lexical",
        });
    }

    let query_vec = embedder.embed(entity_name).await?;

    // Lazy backfill: any canonical without a persisted embedding for this
    // version (pre-existing rows, or a freshly activated embedding version) is
    // embedded ONCE and stored. Steady state finds none, so resolution never
    // re-embeds canonicals live — the O(n) cost per source is gone.
    for (cid, cname) in
        brainiac_store::entities::canonicals_missing_embedding(conn, org_id, embedding_version)
            .await?
    {
        let v = embedder.embed(&cname).await?;
        brainiac_store::entities::upsert_canonical_embedding(conn, cid, embedding_version, &v)
            .await?;
    }

    // One similarity query against persisted embeddings replaces the live
    // re-embed loop over every canonical name.
    let best =
        brainiac_store::entities::nearest_canonical(conn, org_id, embedding_version, &query_vec, 1)
            .await?
            .into_iter()
            .next()
            .map(|(id, name, _kind, sim)| (id, name, sim));

    match best {
        Some((canonical_id, _, sim)) if sim >= AUTO_LINK_SIMILARITY => {
            brainiac_store::entities::link(
                conn,
                entity_id,
                canonical_id,
                sim,
                "embedding_block",
                None,
            )
            .await?;
            record_aliases(conn, canonical_id, entity_name, entity_aliases).await?;
            Ok(ResolveOutcome::Linked {
                canonical_id,
                method: "embedding_block",
            })
        }
        Some((canonical_id, canonical_name, sim)) if sim >= ADJUDICATION_FLOOR => {
            let resp = provider
                .complete(&ChatRequest {
                    system: ADJUDICATE_SYSTEM_PROMPT_V1.to_string(),
                    user: format!("Name A: {entity_name}\nName B: {canonical_name}"),
                    json_mode: true,
                    max_tokens: 128,
                })
                .await
                .context("adjudication call")?;
            let verdict: Adjudication = crate::extract::extract_json_object(&resp.text)
                .and_then(|j| serde_json::from_str(j).ok())
                .ok_or_else(|| anyhow::anyhow!("unparseable adjudication"))?;
            if verdict.same && verdict.confidence >= ADJUDICATION_AUTO_CONFIDENCE {
                brainiac_store::entities::link(
                    conn,
                    entity_id,
                    canonical_id,
                    verdict.confidence,
                    "llm_adjudicated",
                    None,
                )
                .await?;
                record_aliases(conn, canonical_id, entity_name, entity_aliases).await?;
                Ok(ResolveOutcome::Linked {
                    canonical_id,
                    method: "llm_adjudicated",
                })
            } else {
                // Ambiguous or negative: NO link is written. Unlinked entities
                // ARE the review queue.
                Ok(ResolveOutcome::NeedsReview {
                    best_candidate: Some(canonical_id),
                })
            }
        }
        _ => {
            // Bootstrap: this surface form is the first of its kind.
            let canonical_id = Uuid::new_v4();
            brainiac_store::entities::insert_canonical(
                conn,
                canonical_id,
                org_id,
                entity_name,
                entity_kind,
            )
            .await?;
            // Persist the canonical's name embedding at birth — the canonical
            // name equals this surface form, so query_vec is exactly it.
            brainiac_store::entities::upsert_canonical_embedding(
                conn,
                canonical_id,
                embedding_version,
                &query_vec,
            )
            .await?;
            // Seed the canonical's alias set with this form's captured aliases
            // (its name is already the canonical name).
            record_aliases(conn, canonical_id, entity_name, entity_aliases).await?;
            brainiac_store::entities::link(
                conn,
                entity_id,
                canonical_id,
                1.0,
                "embedding_block",
                None,
            )
            .await?;
            Ok(ResolveOutcome::NewCanonical { canonical_id })
        }
    }
}
