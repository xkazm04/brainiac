//! Resolve stage: raw entity → canonical linking (stage 4 of §3).
//!
//! Blocking by embedding similarity over canonical names, then a BYOM
//! adjudication in the ambiguous band. Thresholds deliberately bias toward
//! the review queue on sparse data (ARCHITECTURE.md §9 risk 3): the
//! zero-tolerance failure is a FALSE merge, not a missed one — an unlinked
//! entity just waits for a human.

use anyhow::{Context, Result};
use brainiac_core::embed::{cosine, Embedder};
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

pub async fn resolve_entity(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    org_id: Uuid,
    entity_id: Uuid,
    entity_name: &str,
    entity_kind: &str,
) -> Result<ResolveOutcome> {
    let canonicals = brainiac_store::entities::list_canonicals(conn, org_id).await?;
    let query_vec = embedder.embed(entity_name);

    let mut best: Option<(Uuid, String, f32)> = None;
    for (id, name, _kind) in &canonicals {
        let sim = cosine(&query_vec, &embedder.embed(name));
        if best.as_ref().is_none_or(|(_, _, s)| sim > *s) {
            best = Some((*id, name.clone(), sim));
        }
    }

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
