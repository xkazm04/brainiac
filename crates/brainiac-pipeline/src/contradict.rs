//! Contradiction stage (stage 5 of §3): for a new memory, find semantically
//! close memories sharing an entity anchor and ask the provider whether the
//! claims genuinely conflict. Over-flagging is the failure mode that kills
//! review queues, so the prompt names the coexist/dismiss outcomes
//! explicitly and the verdict must justify supersession direction.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::Memory;
use brainiac_gateway::{ChatProvider, ChatRequest};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

pub const CONTRADICT_SYSTEM_PROMPT_V1: &str = "\
Two knowledge statements from the same organization are given. Decide their relationship.
- supersede: they make INCOMPATIBLE claims about the same thing; one must replace the other.
- coexist: they look similar but apply to different scopes/things — both stay true.
- dismiss: they are simply different statements; no tension at all.
Respond with ONLY JSON:
{\"relation\":\"supersede|coexist|dismiss\",\"winner\":\"a|b|null\",\"reason\":\"...\"}";

#[derive(Debug, Deserialize)]
struct Verdict {
    relation: String,
    #[serde(default)]
    winner: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct ContradictStats {
    pub compared: usize,
    pub opened: usize,
}

/// Compare `memory` against its nearest visible neighbors (same embedding
/// space, shared entity anchors) and open contradiction rows for genuine
/// conflicts. Suggested resolution is recorded in the note; a human (or the
/// promote flow) applies it.
pub async fn run_contradict(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    memory: &Memory,
) -> Result<ContradictStats> {
    let mut stats = ContradictStats::default();

    // Candidates: nearest by vector, restricted to those sharing an anchor.
    let query_vec = embedder.embed(&memory.content).await?;
    let near = brainiac_store::memories::search_vector(
        conn,
        embedding_version,
        &query_vec,
        6,
        &Default::default(),
    )
    .await?;
    let anchor_ids = brainiac_store::entities::anchors_of_memories(conn, &[memory.id]).await?;
    let candidate_ids: Vec<Uuid> = near
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| *id != memory.id)
        .collect();
    if candidate_ids.is_empty() {
        return Ok(stats);
    }
    let candidates = brainiac_store::memories::get_by_ids(conn, &candidate_ids).await?;

    for other in candidates {
        // Entity-overlap filter.
        let other_anchors =
            brainiac_store::entities::anchors_of_memories(conn, &[other.id]).await?;
        if !other_anchors.iter().any(|a| anchor_ids.contains(a)) {
            continue;
        }
        stats.compared += 1;

        let resp = provider
            .complete(&ChatRequest {
                system: CONTRADICT_SYSTEM_PROMPT_V1.to_string(),
                user: format!("A: {}\nB: {}", other.content, memory.content),
                json_mode: true,
                max_tokens: 256,
            })
            .await
            .context("contradiction call")?;
        let Some(verdict) = crate::extract::extract_json_object(&resp.text)
            .and_then(|j| serde_json::from_str::<Verdict>(j).ok())
        else {
            continue; // unparseable verdict: skip, never guess
        };

        if verdict.relation == "supersede" {
            let direction = match verdict.winner.as_deref() {
                Some("a") => "a_over_b",
                _ => "b_over_a",
            };
            brainiac_store::governance::insert_contradiction(
                conn,
                org_id,
                other.id,
                memory.id,
                &resp.model_ref,
                &format!(
                    "suggested: supersede ({direction}) — {}",
                    verdict.reason.unwrap_or_default()
                ),
            )
            .await?;
            stats.opened += 1;
        }
        // coexist / dismiss: no row — silence is the correct output.
    }
    Ok(stats)
}
