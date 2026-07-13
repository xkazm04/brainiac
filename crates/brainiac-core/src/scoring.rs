//! Blended retrieval ranking (ARCHITECTURE.md §4 stage 6: "order by relevance
//! and recency"). Pure, deterministic, Postgres-free — the store's assembly
//! stage calls [`blended_score`] once per surviving candidate.
//!
//! The fused RRF relevance stays the DOMINANT term. Recency and feedback are
//! deliberately *tiebreak-scale* nudges: they reorder candidates that are
//! already near-tied on relevance (a fresh correction over a stale
//! near-duplicate, a repeatedly-helpful memory over an unrated one) without
//! letting a fresh-but-irrelevant memory climb over a strong lexical/vector
//! hit. The weights below are sized against the RRF gap between adjacent ranks
//! near the top of the list (`1/(k+r) − 1/(k+r+1) ≈ 2.7e-4` at k=60, r=1), so
//! the combined nudge is at most ~one adjacent-rank gap.

/// Half-life of the recency term, in days: a memory this old contributes half
/// the freshness bonus of a brand-new one. Chosen between the procedural TTL
/// (howto, 180d) and the fact TTL (365d) — long enough that a decision made
/// last quarter still reads as "current", short enough that a year-old memory
/// has visibly decayed. Recency is a nudge, not a cliff; supersession
/// (temporal.rs) is what actually retires stale knowledge.
pub const RECENCY_HALF_LIFE_DAYS: f64 = 180.0;

/// Weight of the recency nudge — the maximum bonus (age 0) a memory can earn
/// for freshness. The total swing two candidates can differ by on the nudges
/// alone is `RECENCY_WEIGHT + 2·FEEDBACK_WEIGHT` (fresh+helpful vs
/// stale+disputed); at 1e-4 that is 3e-4 ≈ one adjacent-rank RRF gap near the
/// top of the list, so the nudges reorder near-ties without ever overcoming a
/// decisive relevance difference.
pub const RECENCY_WEIGHT: f64 = 1.0e-4;

/// Weight of the feedback nudge — the maximum magnitude (positive or negative)
/// a memory's net reader verdicts can move its score. Same tiebreak scale as
/// [`RECENCY_WEIGHT`].
pub const FEEDBACK_WEIGHT: f64 = 1.0e-4;

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

/// The final ranking key for a candidate. `relevance` is the fused RRF score
/// (dominant); `age_days` is the memory's age at the query's as-of instant;
/// `feedback` is its net reader verdicts. The two nudges are tiebreak-scale by
/// construction (see module docs).
pub fn blended_score(relevance: f64, age_days: f64, feedback: FeedbackSignal) -> f64 {
    relevance
        + RECENCY_WEIGHT * recency_decay(age_days)
        + FEEDBACK_WEIGHT * feedback_score(feedback)
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
        // Invariant: a relevance gap wider than the maximum nudge swing
        // (RECENCY_WEIGHT + 2·FEEDBACK_WEIGHT) can NEVER be overcome, even with
        // the worst-case pairing — strong candidate stale+disputed, weak
        // candidate fresh+adored.
        let max_swing = RECENCY_WEIGHT + 2.0 * FEEDBACK_WEIGHT;
        let rel_weak = 1.0 / 61.0;
        let rel_strong = rel_weak + max_swing * 1.5; // decisively wider than any nudge
        let strong_stale = blended_score(
            rel_strong,
            RECENCY_HALF_LIFE_DAYS * 8.0,
            FeedbackSignal {
                helpful: 0,
                wrong: 5,
                outdated: 0,
            },
        );
        let weak_fresh = blended_score(
            rel_weak,
            0.0,
            FeedbackSignal {
                helpful: 50,
                wrong: 0,
                outdated: 0,
            },
        );
        assert!(
            strong_stale > weak_fresh,
            "relevance stays dominant: {strong_stale} vs {weak_fresh}"
        );
    }

    #[test]
    fn fresh_outranks_stale_at_equal_relevance() {
        let rel = 1.0 / 65.0;
        let fresh = blended_score(rel, 1.0, FeedbackSignal::default());
        let stale = blended_score(rel, 400.0, FeedbackSignal::default());
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
        );
        let unrated = blended_score(rel, 30.0, FeedbackSignal::default());
        let disputed = blended_score(
            rel,
            30.0,
            FeedbackSignal {
                helpful: 0,
                wrong: 3,
                outdated: 1,
            },
        );
        assert!(helpful > unrated, "consistently-helpful wins the tie");
        assert!(unrated > disputed, "disputed sinks below neutral");
    }
}
