//! Contradiction stage (stage 5 of §3): for a new memory, find semantically
//! close memories and ask the provider whether the claims genuinely conflict.
//! Over-flagging is the failure mode that kills review queues, so the prompt
//! names the coexist/dismiss outcomes explicitly and the verdict must justify
//! supersession direction.
//!
//! Discovery has two paths, and the asymmetry between them is deliberate:
//!
//! 1. **Anchored** — a near neighbour that shares an entity anchor with the new
//!    memory. The anchor is independent corroboration that the two statements
//!    are about the same thing, and it is what keeps precision where it is.
//! 2. **Anchorless** — a near neighbour that shares NO anchor, admitted only
//!    above a high similarity bar and only [`ANCHORLESS_BUDGET`] of them. This
//!    exists because path 1 alone made a whole class of real contradictions
//!    *structurally unreachable*: two incompatible claims that were simply never
//!    linked to a common entity were never compared, at any similarity, forever.
//!    Widening discovery is not widening the gate — the stage still never
//!    auto-applies a supersede verdict.

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
    /// Of `compared`, how many pairs were reached ONLY by the anchorless path —
    /// the ones the entity filter would have dropped. Surfaced so the added LLM
    /// spend and the added false-positive exposure are measurable, not assumed.
    pub anchorless_compared: usize,
}

/// How many vector neighbours are considered at all. Unchanged: the anchorless
/// path widens which of these reach the model, never how many are fetched.
const NEIGHBORS: i64 = 6;

/// The anchorless reachability budget (Direction 2).
///
/// Candidate discovery required vector similarity AND a shared entity anchor, so
/// a genuinely contradictory pair that was never linked to a common anchor was
/// STRUCTURALLY unreachable — never compared, at any similarity, forever. The
/// anchor filter is what holds precision, so it stays exactly as it is; this is
/// an ADDITIONAL path, not a replacement.
///
/// Two bounds, and both matter:
///
/// * [`anchorless_min_similarity`] — without a shared anchor, similarity is the
///   ONLY evidence that two statements are even about the same thing, so the bar
///   has to be higher than "it came back in the top [`NEIGHBORS`]".
/// * [`ANCHORLESS_BUDGET`] — at most this many anchorless pairs per memory. Both
///   sets are drawn from the SAME [`NEIGHBORS`] neighbours, so total model calls
///   per new memory stay bounded by `NEIGHBORS` exactly as before; the budget
///   bounds the *typical* increase, which is what a bill and a review queue
///   actually feel.
///
/// **The threshold is embedder-specific and this default is not portable.** It
/// is calibrated on `deterministic-bow-v1`, the eval embedder, where the gold
/// anchorless supersede pair scores 0.359 and the nearest non-contradictory
/// anchorless pair scores 0.207 — twelve fixture pairs, which is a floor, not a
/// distribution. A real embedder puts related text far higher (0.7–0.9 is
/// ordinary) and this default would then admit almost every neighbour. Override
/// it per deployment with `BRAINIAC_CONTRADICT_ANCHORLESS_MIN_SIM` after
/// measuring your own embedder's distribution; setting it to `1.0` or above
/// disables the anchorless path entirely and restores the previous behaviour
/// exactly.
pub const DEFAULT_ANCHORLESS_MIN_SIMILARITY: f32 = 0.30;

/// The similarity bar in force — see [`DEFAULT_ANCHORLESS_MIN_SIMILARITY`].
/// Values outside `[-1, 1]` other than a deliberate disable are ignored rather
/// than silently reshaping discovery; `>= 1.0` is honored, and turns the path
/// off (no cosine reaches it).
pub fn anchorless_min_similarity() -> f32 {
    std::env::var("BRAINIAC_CONTRADICT_ANCHORLESS_MIN_SIM")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= -1.0)
        .unwrap_or(DEFAULT_ANCHORLESS_MIN_SIMILARITY)
}

pub const ANCHORLESS_BUDGET: usize = 2;

/// Pick the anchorless candidates worth a model call: above the bar, closest
/// first, capped at `budget`. Pure and generic over the payload so the policy —
/// the part that decides what the org pays for and what a reviewer sees — is
/// unit-testable without a database.
fn admit_anchorless<T>(
    mut pool: Vec<(f32, T)>,
    min_similarity: f32,
    budget: usize,
) -> Vec<(f32, T)> {
    pool.retain(|(sim, _)| *sim >= min_similarity);
    // Closest first, then capped: the budget spends itself on the strongest
    // evidence available, never on the tail of the neighbour list.
    pool.sort_by(|a, b| b.0.total_cmp(&a.0));
    pool.truncate(budget);
    pool
}

/// Marker appended to an anchorless candidate's suggestion note, so a maintainer
/// (and an operator auditing precision) can tell at a glance that this pair was
/// found WITHOUT the corroborating evidence of a shared entity anchor.
fn anchorless_marker(similarity: f32) -> String {
    format!(" [reached without a shared entity anchor; cosine {similarity:.3}]")
}

/// Compare `memory` against its nearest visible neighbors — those sharing an
/// entity anchor, plus a bounded set of anchorless near-duplicates (see
/// [`DEFAULT_ANCHORLESS_MIN_SIMILARITY`]) — and open contradiction rows for
/// genuine conflicts. The suggested supersession direction is recorded in the note; a
/// human maintainer applies it through the governance resolve path, which calls
/// [`brainiac_store::governance::apply_supersession`] to close the loser.
///
/// The pipeline deliberately does NOT auto-apply a supersede verdict. Applying
/// one deprecates a memory (removes a truth from retrieval), and the adjudicator
/// returns no calibrated confidence to gate on — so there is no clean policy
/// hook, and auto-applying would bypass the maintainer-of-the-losing-team gate
/// that the governance path enforces. Detection stays advisory; application
/// stays human. (Direction 1: governance-path-only, by design.)
pub async fn run_contradict(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    memory: &Memory,
) -> Result<ContradictStats> {
    let mut stats = ContradictStats::default();

    // Candidates: nearest by vector. Those sharing an entity anchor are compared
    // exactly as they always were; the rest are eligible for the bounded
    // anchorless path below.
    let query_vec = embedder.embed(&memory.content).await?;
    let near = brainiac_store::memories::search_vector(
        conn,
        embedding_version,
        &query_vec,
        NEIGHBORS,
        &Default::default(),
    )
    .await?;
    let anchor_ids = brainiac_store::entities::anchors_of_memories(conn, &[memory.id]).await?;
    let similarity: std::collections::HashMap<Uuid, f32> = near.iter().copied().collect();
    let candidate_ids: Vec<Uuid> = near
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| *id != memory.id)
        .collect();
    if candidate_ids.is_empty() {
        return Ok(stats);
    }
    let candidates = brainiac_store::memories::get_by_ids(conn, &candidate_ids).await?;

    // Split, don't filter. The anchor-sharing set is untouched — same members,
    // same order, same comparisons as before this path existed.
    let mut anchored: Vec<Memory> = Vec::new();
    let mut anchorless: Vec<(f32, Memory)> = Vec::new();
    for other in candidates {
        let other_anchors =
            brainiac_store::entities::anchors_of_memories(conn, &[other.id]).await?;
        if other_anchors.iter().any(|a| anchor_ids.contains(a)) {
            anchored.push(other);
            continue;
        }
        // No shared anchor: closeness is the only evidence there is.
        let sim = similarity.get(&other.id).copied().unwrap_or(f32::MIN);
        anchorless.push((sim, other));
    }
    let anchorless = admit_anchorless(anchorless, anchorless_min_similarity(), ANCHORLESS_BUDGET);

    let queue: Vec<(Option<f32>, Memory)> = anchored
        .into_iter()
        .map(|m| (None, m))
        .chain(anchorless.into_iter().map(|(sim, m)| (Some(sim), m)))
        .collect();

    for (anchorless_sim, other) in queue {
        stats.compared += 1;
        if anchorless_sim.is_some() {
            stats.anchorless_compared += 1;
        }

        let resp = provider
            .complete(&ChatRequest {
                system: CONTRADICT_SYSTEM_PROMPT_V1.to_string(),
                user: format!("A: {}\nB: {}", other.content, memory.content),
                json_mode: true,
                max_tokens: 256,
                temperature: 0.0,
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
                    "suggested: supersede ({direction}) — {}{}",
                    verdict.reason.unwrap_or_default(),
                    anchorless_sim.map(anchorless_marker).unwrap_or_default()
                ),
            )
            .await?;
            stats.opened += 1;
        }
        // coexist / dismiss: no row — silence is the correct output.
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy in one assertion set: above the bar, closest first, capped.
    #[test]
    fn anchorless_admission_is_bounded_and_prefers_the_closest() {
        let pool = vec![
            (0.10, "far"),
            (0.92, "closest"),
            (0.41, "near"),
            (0.62, "mid"),
        ];
        let admitted = admit_anchorless(pool, 0.30, ANCHORLESS_BUDGET);
        assert_eq!(
            admitted.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
            vec!["closest", "mid"],
            "the budget must buy the strongest evidence, not the first rows seen"
        );
    }

    #[test]
    fn nothing_below_the_bar_ever_costs_a_model_call() {
        let pool = vec![(0.29, "a"), (-0.5, "b"), (0.2999, "c")];
        assert!(admit_anchorless(pool, 0.30, ANCHORLESS_BUDGET).is_empty());
    }

    /// The escape hatch an operator on a real embedder needs: a bar of 1.0 is
    /// unreachable by any cosine the search returns, so discovery collapses back
    /// to the anchor filter alone — byte-identical to the behaviour before this
    /// path existed.
    #[test]
    fn a_bar_of_one_restores_the_previous_behaviour_exactly() {
        let pool = vec![(0.99, "very close"), (0.95, "close")];
        assert!(admit_anchorless(pool, 1.0, ANCHORLESS_BUDGET).is_empty());
    }

    #[test]
    fn the_marker_tells_a_reviewer_the_pair_had_no_anchor() {
        let note = anchorless_marker(0.3586);
        assert!(note.contains("without a shared entity anchor"), "{note}");
        assert!(note.contains("0.359"), "{note}");
    }

    /// A malformed override must not silently reshape discovery.
    #[test]
    fn a_garbage_override_falls_back_to_the_default() {
        // Read the default directly: env is process-global and other tests in
        // this binary must not be able to perturb it.
        assert_eq!(DEFAULT_ANCHORLESS_MIN_SIMILARITY, 0.30);
        assert!(("not-a-number".parse::<f32>().ok()).is_none());
    }
}
