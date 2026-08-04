//! Blended retrieval ranking (ARCHITECTURE.md §4 stage 6: "order by relevance
//! and recency"). Pure, deterministic, Postgres-free — the store's assembly
//! stage calls [`blended_score`] once per surviving candidate.
//!
//! The fused RRF relevance stays the DOMINANT term. Recency, feedback and
//! project affinity are deliberately *tiebreak-scale* nudges: they reorder
//! candidates that are already near-tied on relevance (a fresh correction over a
//! stale near-duplicate, a repeatedly-helpful memory over an unrated one, this
//! repo's own decision over a generic org-wide note) without letting a
//! fresh-but-irrelevant memory climb over a strong lexical/vector hit. The
//! weights below are sized against the RRF gap between adjacent ranks near the
//! top of the list (`1/(k+r) − 1/(k+r+1) ≈ 2.7e-4` at k=60, r=1); each
//! individual nudge is at most ~one third of that gap, and the worst-case
//! combined swing ([`MAX_NUDGE_SWING`]) is ~1.5 gaps — enough to reorder
//! immediate neighbours, never enough to lift a candidate past a decisive
//! relevance difference.

/// Half-life of the recency term, in days: a memory this old contributes half
/// the freshness bonus of a brand-new one. Chosen between the procedural TTL
/// (howto, 180d) and the fact TTL (365d) — long enough that a decision made
/// last quarter still reads as "current", short enough that a year-old memory
/// has visibly decayed. Recency is a nudge, not a cliff; supersession
/// (temporal.rs) is what actually retires stale knowledge.
pub const RECENCY_HALF_LIFE_DAYS: f64 = 180.0;

/// Weight of the recency nudge — the maximum bonus (age 0) a memory can earn
/// for freshness. See [`MAX_NUDGE_SWING`] for how it composes with the others.
pub const RECENCY_WEIGHT: f64 = 1.0e-4;

/// Weight of the feedback nudge — the maximum magnitude (positive or negative)
/// a memory's net reader verdicts can move its score. Same tiebreak scale as
/// [`RECENCY_WEIGHT`].
pub const FEEDBACK_WEIGHT: f64 = 1.0e-4;

/// Weight of the project-affinity nudge — the bonus a memory earns for having
/// been written by the caller's OWN project (PROJECT-PLAN PR1's project axis,
/// finally reaching ranking).
///
/// **Why it exists.** Project locality was a binary gate: the lens admits "my
/// project + the org's conventions" and contributes nothing to the order, so a
/// payments team's own hard-won retry decision ranked identically to a generic
/// org-wide note that happened to embed as well. Locality is real evidence
/// about which of two comparable answers is the right one *here*.
///
/// **Why exactly this size**, by the same argument [`RECENCY_WEIGHT`] and
/// [`FEEDBACK_WEIGHT`] are sized by: one adjacent-rank RRF gap near the top of
/// the list is ≈`2.7e-4`, so a `1e-4` term can promote a candidate over its
/// immediate neighbour and no further. That is the whole claim being made —
/// "when two memories are comparably relevant, prefer this repo's" — and
/// deliberately NOT "prefer this repo's answer over a better one". A larger
/// weight would let locality overturn genuine relevance, which is the failure
/// mode this scale exists to prevent; a smaller one would be inert against the
/// recency term it must be able to argue with (a year-old decision from THIS
/// repo should be able to outrank a fresh generic org note, which needs
/// affinity ≥ recency).
///
/// It is *not* a penalty on org-shared knowledge: shared memories score exactly
/// as they did before (see [`ProjectAffinity`]), so nothing is excluded or
/// pushed below its relevance — only own-project rows are lifted within a tie.
///
/// **Recorded negative result — the eval cannot referee this term.** The
/// "Meridian" fixtures carry NO project stamps (`brainiac-eval::seed` writes
/// `project_id: None` on every memory, and the harness's principals have no
/// project), so every candidate is [`ProjectAffinity::NoLens`] and the whole
/// term is structurally inert under `eval --profile retrieval`: the run is
/// digit-for-digit the committed baseline, and no recalibration was needed or
/// made. Do not read that as evidence the weight is safe — it is evidence the
/// weight is UNTESTED there. Re-tuning this constant is a change the eval will
/// silently pass; the guard that actually holds it is the store-level
/// `project_affinity_lifts_own_project_rows_without_demoting_org_shared`, which
/// pins the delta to exactly this value. Giving the fixtures a project axis is
/// the prerequisite for ever moving this number on evidence.
pub const PROJECT_AFFINITY_WEIGHT: f64 = 1.0e-4;

/// The worst-case score difference two candidates of IDENTICAL relevance can
/// end up with on the nudges alone: fresh + adored + local vs stale + disputed +
/// org-shared. This is the executable form of the module's dominance claim —
/// [`blended_score`] can never reorder a pair whose relevance differs by more
/// than this.
///
/// At `4e-4` it is ≈1.5 adjacent-rank RRF gaps near the top of the list. That
/// grew from ≈1.1 gaps when project affinity was added. The growth is bounded
/// and deliberate, but note what it does and does not mean: the swing is the
/// worst case over a pair with IDENTICAL relevance, and no single nudge (each
/// ≤1e-4) can move a candidate across an adjacent RRF rank on its own. The
/// nudges break ties; they do not re-rank.
pub const MAX_NUDGE_SWING: f64 = RECENCY_WEIGHT + 2.0 * FEEDBACK_WEIGHT + PROJECT_AFFINITY_WEIGHT;

/// Net-verdict count at which the feedback nudge reaches ~76% of its ceiling
/// (`tanh(1)`). A handful of consistent verdicts saturates the term, so a
/// single loud memory cannot buy unbounded rank.
pub const FEEDBACK_SATURATION: f64 = 3.0;

/// Exponential recency decay in `[0, 1]`: `1.0` at `age_days = 0`, `0.5` at one
/// half-life, asymptotically `0`. Negative ages (a memory whose anchor time is
/// after the as-of instant — e.g. created later about a still-valid fact) are
/// clamped to fully fresh rather than amplified.
pub fn recency_decay(age_days: f64) -> f64 {
    let age = age_days.max(0.0);
    (-std::f64::consts::LN_2 * age / RECENCY_HALF_LIFE_DAYS).exp()
}

/// Decay applied to a graph-expansion hit's relevance relative to the direct
/// hit that anchored the expansion. A memory surfaced by walking the canonical
/// entity graph (1–2 hops from the query's own anchors) is real signal —
/// cross-team knowledge the query never lexically or semantically matched — but
/// it is one inferential step removed, so it should sit BELOW a direct hit of
/// the same strength. At 0.3 a graph hit is worth 30% of the strongest direct
/// hit's fused score: enough that a strongly-anchored (consensus) hit clears
/// the weak tail of the direct list (the whole point of expansion — cross-team
/// hits used to sink to 0), but not so much that graph hits swamp the
/// directly-relevant ones. Tuned against the eval gates: at 0.4 the extra graph
/// hits displaced graded memories in the temporal stratum past its regression
/// gate; 0.3 keeps every stratum green (temporal and cross-team both hold at
/// baseline) while still lifting graph hits off the floor.
pub const GRAPH_HOP_DECAY: f64 = 0.3;

/// Relevance for a graph-surfaced memory: the anchoring direct hit's fused
/// score, decayed by [`GRAPH_HOP_DECAY`]. Replaces the old hard-coded 0.0 that
/// made cross-team graph hits always sink below every direct hit. Feeds the
/// same [`blended_score`] as direct hits, so graph and direct results share one
/// ranking scale.
pub fn graph_relevance(anchor_score: f64) -> f64 {
    anchor_score * GRAPH_HOP_DECAY
}

/// Reader-verdict counts attached to one memory (all-time, RLS-scoped at the
/// query site). Mirrors [`crate`]'s store-side `Trust` without depending on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedbackSignal {
    pub helpful: i64,
    pub wrong: i64,
    pub outdated: i64,
}

impl FeedbackSignal {
    /// Net signal: helpful lifts, wrong/outdated sink. `outdated` and `wrong`
    /// weigh the same — both say "do not rank this as current truth".
    pub fn net(&self) -> i64 {
        self.helpful - self.wrong - self.outdated
    }
}

/// Saturating feedback score in `(-1, 1)`: `tanh(net / saturation)`. Symmetric
/// (a wrong verdict sinks as much as a helpful one lifts) and bounded so no
/// memory can dominate ranking by accumulating verdicts.
pub fn feedback_score(f: FeedbackSignal) -> f64 {
    (f.net() as f64 / FEEDBACK_SATURATION).tanh()
}

/// How a candidate relates to the project the caller is working in.
///
/// Three states rather than a `bool` so the inert case is *structurally*
/// distinct from "not local": an org-level caller has no "here" to be local to,
/// and that must not be confused with a candidate that lost the affinity tie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProjectAffinity {
    /// The caller has no project (an org-wide token, the eval harness, a
    /// console-side org view). The term is inert for EVERY candidate, so
    /// ranking is bit-for-bit what it was before affinity existed.
    #[default]
    NoLens,
    /// Org-shared knowledge (`project_id IS NULL`), or another project's row
    /// that the caller can nevertheless see. Earns nothing — and loses nothing.
    Shared,
    /// Written by the caller's own project. This repo's own decision.
    Own,
}

/// Affinity term in `[0, 1]`, mirroring [`recency_decay`] / [`feedback_score`]:
/// the shape lives here, the weight lives in [`PROJECT_AFFINITY_WEIGHT`].
///
/// Binary rather than graded on purpose. There is no meaningful "how local" —
/// a memory either came out of the work the caller is doing or it did not — and
/// a graded version would need a project-similarity notion that does not exist
/// and would smuggle project structure into what is supposed to be a tiebreak.
pub fn project_affinity_score(affinity: ProjectAffinity) -> f64 {
    match affinity {
        ProjectAffinity::Own => 1.0,
        // Both non-Own states score identically; the distinction they carry is
        // for the caller's own reasoning (and the inert-ness test), not the sum.
        ProjectAffinity::Shared | ProjectAffinity::NoLens => 0.0,
    }
}

/// The final ranking key for a candidate. `relevance` is the fused RRF score
/// (dominant); `age_days` is the memory's age at the query's as-of instant;
/// `feedback` is its net reader verdicts; `affinity` is whether the caller's own
/// project wrote it. The three nudges are tiebreak-scale by construction — see
/// [`MAX_NUDGE_SWING`] and the module docs.
pub fn blended_score(
    relevance: f64,
    age_days: f64,
    feedback: FeedbackSignal,
    affinity: ProjectAffinity,
) -> f64 {
    relevance
        + RECENCY_WEIGHT * recency_decay(age_days)
        + FEEDBACK_WEIGHT * feedback_score(feedback)
        + PROJECT_AFFINITY_WEIGHT * project_affinity_score(affinity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recency_decay_hits_half_at_one_half_life() {
        assert!(
            (recency_decay(0.0) - 1.0).abs() < 1e-12,
            "age 0 is fully fresh"
        );
        assert!(
            (recency_decay(RECENCY_HALF_LIFE_DAYS) - 0.5).abs() < 1e-9,
            "one half-life halves the bonus"
        );
        assert!(
            recency_decay(RECENCY_HALF_LIFE_DAYS * 4.0) < 0.07,
            "four half-lives is nearly gone"
        );
    }

    #[test]
    fn recency_decay_is_monotonic_and_clamps_negative_age() {
        assert_eq!(
            recency_decay(-5.0),
            1.0,
            "future-anchored age clamps to fresh"
        );
        assert!(recency_decay(10.0) > recency_decay(100.0));
        assert!(recency_decay(100.0) > recency_decay(1000.0));
    }

    #[test]
    fn feedback_is_symmetric_and_bounded() {
        let up = feedback_score(FeedbackSignal {
            helpful: 5,
            wrong: 0,
            outdated: 0,
        });
        let down = feedback_score(FeedbackSignal {
            helpful: 0,
            wrong: 3,
            outdated: 2,
        });
        assert!(up > 0.0 && up < 1.0);
        assert!((up + down).abs() < 1e-12, "±5 net is symmetric");
        // Bounded: a flood of verdicts saturates at the tanh ceiling (±1).
        let flood = feedback_score(FeedbackSignal {
            helpful: 1000,
            wrong: 0,
            outdated: 0,
        });
        assert!(flood > 0.99 && flood <= 1.0);
        assert_eq!(
            feedback_score(FeedbackSignal::default()),
            0.0,
            "no verdicts is neutral"
        );
    }

    #[test]
    fn net_treats_wrong_and_outdated_equally() {
        assert_eq!(
            FeedbackSignal {
                helpful: 2,
                wrong: 1,
                outdated: 1
            }
            .net(),
            0
        );
        assert_eq!(
            FeedbackSignal {
                helpful: 0,
                wrong: 2,
                outdated: 0
            }
            .net(),
            -2
        );
    }

    #[test]
    fn relevance_dominates_the_nudges() {
        // Invariant: a relevance gap wider than MAX_NUDGE_SWING can NEVER be
        // overcome, even with the worst-case pairing — strong candidate
        // stale+disputed+org-shared, weak candidate fresh+adored+local.
        let rel_weak = 1.0 / 61.0;
        let rel_strong = rel_weak + MAX_NUDGE_SWING * 1.5; // decisively wider
        let strong_stale = blended_score(
            rel_strong,
            RECENCY_HALF_LIFE_DAYS * 8.0,
            FeedbackSignal {
                helpful: 0,
                wrong: 5,
                outdated: 0,
            },
            ProjectAffinity::Shared,
        );
        let weak_fresh = blended_score(
            rel_weak,
            0.0,
            FeedbackSignal {
                helpful: 50,
                wrong: 0,
                outdated: 0,
            },
            ProjectAffinity::Own,
        );
        assert!(
            strong_stale > weak_fresh,
            "relevance stays dominant: {strong_stale} vs {weak_fresh}"
        );
    }

    #[test]
    fn own_project_outranks_org_shared_at_equal_relevance() {
        // The user moment: two comparably-relevant answers, one written by the
        // repo the agent is standing in. That one leads.
        let rel = 1.0 / 65.0;
        let local = blended_score(rel, 30.0, FeedbackSignal::default(), ProjectAffinity::Own);
        let shared = blended_score(
            rel,
            30.0,
            FeedbackSignal::default(),
            ProjectAffinity::Shared,
        );
        assert!(
            local > shared,
            "own project wins the tie: {local} vs {shared}"
        );
        // …and org-shared is not PENALISED: it scores exactly what it scored
        // before affinity existed (the NoLens value), so nothing is pushed
        // below its relevance.
        let no_lens = blended_score(
            rel,
            30.0,
            FeedbackSignal::default(),
            ProjectAffinity::NoLens,
        );
        assert_eq!(shared, no_lens, "shared is not demoted, only Own is lifted");
    }

    #[test]
    fn affinity_is_inert_without_a_project_lens() {
        // Criterion: unchanged behaviour for org-level callers. With NoLens on
        // every candidate the term contributes a constant 0, so the ranking key
        // is bit-for-bit the pre-affinity blend for every input.
        for (rel, age, fb) in [
            (1.0 / 61.0, 0.0, FeedbackSignal::default()),
            (
                1.0 / 108.0,
                400.0,
                FeedbackSignal {
                    helpful: 3,
                    wrong: 1,
                    outdated: 0,
                },
            ),
            (
                2.0 / 61.0,
                12.5,
                FeedbackSignal {
                    helpful: 0,
                    wrong: 4,
                    outdated: 2,
                },
            ),
        ] {
            let legacy =
                rel + RECENCY_WEIGHT * recency_decay(age) + FEEDBACK_WEIGHT * feedback_score(fb);
            assert_eq!(
                blended_score(rel, age, fb, ProjectAffinity::NoLens),
                legacy,
                "no project lens ⇒ the affinity term must vanish exactly"
            );
        }
    }

    #[test]
    fn affinity_can_argue_with_recency_but_never_with_relevance() {
        let rel = 1.0 / 65.0;
        // A year-old decision from THIS repo beats a brand-new generic org note
        // of the same relevance — the reason affinity is sized ≥ recency.
        let local_old = blended_score(rel, 365.0, FeedbackSignal::default(), ProjectAffinity::Own);
        let shared_fresh =
            blended_score(rel, 0.0, FeedbackSignal::default(), ProjectAffinity::Shared);
        assert!(
            local_old > shared_fresh,
            "local beats fresh-but-generic at equal relevance: {local_old} vs {shared_fresh}"
        );
        // But a genuinely more relevant org-shared memory still leads: one
        // adjacent-rank RRF gap is wider than the affinity term alone.
        let adjacent_gap = 1.0 / 61.0 - 1.0 / 62.0;
        assert!(
            PROJECT_AFFINITY_WEIGHT < adjacent_gap,
            "affinity alone must not span an adjacent rank"
        );
        let shared_better = blended_score(
            rel + adjacent_gap,
            30.0,
            FeedbackSignal::default(),
            ProjectAffinity::Shared,
        );
        let local_worse = blended_score(rel, 30.0, FeedbackSignal::default(), ProjectAffinity::Own);
        assert!(
            shared_better > local_worse,
            "a more relevant org convention still wins: {shared_better} vs {local_worse}"
        );
    }

    #[test]
    fn graph_hit_scores_below_its_anchor_but_can_beat_a_weak_direct() {
        // A strong (consensus, both-list) anchor.
        let anchor = 2.0 / 61.0;
        let g = graph_relevance(anchor);
        assert!(g < anchor, "a graph hit is worth less than its anchor");
        assert!(g > 0.0, "but no longer sinks to zero");
        // A weak direct hit deep in a single candidate list can be overtaken.
        let weak_direct = 1.0 / 108.0;
        assert!(
            g > weak_direct,
            "a strong anchor lifts its graph neighbor over weak directs: {g} vs {weak_direct}"
        );
    }

    #[test]
    fn fresh_outranks_stale_at_equal_relevance() {
        let rel = 1.0 / 65.0;
        let fresh = blended_score(rel, 1.0, FeedbackSignal::default(), ProjectAffinity::NoLens);
        let stale = blended_score(
            rel,
            400.0,
            FeedbackSignal::default(),
            ProjectAffinity::NoLens,
        );
        assert!(fresh > stale, "equal relevance → fresher wins the tie");
    }

    #[test]
    fn helpful_outranks_unrated_at_equal_relevance_and_age() {
        let rel = 1.0 / 65.0;
        let helpful = blended_score(
            rel,
            30.0,
            FeedbackSignal {
                helpful: 4,
                wrong: 0,
                outdated: 0,
            },
            ProjectAffinity::NoLens,
        );
        let unrated = blended_score(
            rel,
            30.0,
            FeedbackSignal::default(),
            ProjectAffinity::NoLens,
        );
        let disputed = blended_score(
            rel,
            30.0,
            FeedbackSignal {
                helpful: 0,
                wrong: 3,
                outdated: 1,
            },
            ProjectAffinity::NoLens,
        );
        assert!(helpful > unrated, "consistently-helpful wins the tie");
        assert!(unrated > disputed, "disputed sinks below neutral");
    }
}
