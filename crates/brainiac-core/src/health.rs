//! Knowledge Health: the pillar math, as pure functions.
//!
//! This lives in core, alone, for one reason. The same numbers are used by two
//! callers with very different jobs:
//!
//! - the **leadership report** (`/v1/analytics/knowledge-health`), which shows a
//!   number to a human, and
//! - the **publish circuit breaker** (KB3), which decides whether a degraded
//!   corpus may keep broadcasting itself into the company's wiki.
//!
//! If those two ever computed "currency" differently, the org would be shown a
//! healthy report while the breaker acted on a different reality (or worse, the
//! reverse). A gate that disagrees with the dashboard it is named after is
//! indefensible. So the formulas exist exactly once, here, with no IO.

pub fn clamp100(v: i64) -> i64 {
    v.clamp(0, 100)
}

/// The org's own promotion-review SLO (ARCHITECTURE §7): median review under
/// 48h, or the flywheel dies.
pub const REVIEW_SLO_SECS: i64 = 48 * 3600;

// ── the library's own signals (LIBRARY-PLAN follow-up 2) ────────────────
//
// The Library's anti-rot mechanism is telemetry, not recomposition: the only
// test of a rule is whether practice follows it. These are the thresholds
// that turn "nobody uses this" from an anecdote into an item on a leader's
// report.
//
// Deliberately NOT folded into the composite score. Two reasons, both about
// honesty: (1) the four pillars are a number orgs track week over week, and
// silently redefining it the day someone enables mining would break every
// trend line it is compared against; (2) there is no calibration data yet —
// the same posture the page-read signals took ("measure first, calibrate the
// lever after there is data to calibrate against"). The signals and their
// attention items ARE the promise the Library made — a dead rule going red in
// front of a leader — and that promise needs no weight in a composite.

/// How long an artifact must go unused before it is a deprecation candidate.
/// Thirty days: a quarter is too slow to act on, a week catches every holiday.
pub const LIBRARY_DORMANT_DAYS: i64 = 30;

/// The gate's own SLO: how long a candidate may wait before triage is the
/// bottleneck. Two weeks, not the promotions queue's 48h — a rule proposal is
/// a policy question, and pretending an org can settle policy in two days
/// would make the number a lie that gets ignored.
pub const LIBRARY_GATE_SLO_SECS: i64 = 14 * 24 * 3600;

/// Is an adopted rule dormant? Age matters: a rule adopted yesterday with no
/// usage is NEW, not dead, and flagging it would teach maintainers to ignore
/// the signal. Only a rule the org has had time to use can be said to ignore.
pub fn rule_is_dormant(adopted_secs_ago: i64, uses_in_window: i64) -> bool {
    adopted_secs_ago > LIBRARY_DORMANT_DAYS * 24 * 3600 && uses_in_window == 0
}

/// Is the org contradicting itself? A cross-team conflict costs far more than an
/// intra-team one — nobody on either side can see it.
pub fn consistency_pillar(open_contradictions: i64, cross_team: i64) -> i64 {
    let intra = (open_contradictions - cross_team).max(0);
    clamp100(100 - (cross_team * 30 + intra * 10))
}

/// What share of the corpus is still true? (`stale` = deprecated or past
/// `valid_to`.) An empty corpus is vacuously current — it serves nothing false.
pub fn currency_pillar(total: i64, stale: i64) -> i64 {
    if total <= 0 {
        return 100;
    }
    clamp100(((total - stale) as f64 / total as f64 * 100.0).round() as i64)
}

/// How much of the together-picture the graph assembles: the share of canonical
/// entities carrying ≥2 teams' knowledge.
pub fn liquidity_pillar(canonical: i64, cross_team: i64) -> i64 {
    if canonical <= 0 {
        return 0;
    }
    clamp100((cross_team as f64 / canonical as f64 * 100.0).round() as i64)
}

/// Is the review queue actually being worked? Full marks for an empty queue;
/// degrade as the oldest item ages past the 48h SLO, and for sheer depth.
pub fn governance_pillar(backlog: i64, oldest_secs: i64) -> i64 {
    let age_penalty = ((oldest_secs as f64 / REVIEW_SLO_SECS as f64) * 40.0).round() as i64;
    let depth_penalty = (backlog * 3).min(40);
    clamp100(100 - age_penalty - depth_penalty)
}

/// The headline composite.
///
/// The cross-team cap is the part that matters: a plain weighted average lets a
/// large healthy corpus dilute an unreconciled cross-team contradiction down to
/// a ~10-point ding, so a report could read "Healthy" while two teams act on
/// incompatible truths. The cap makes ONE such conflict drop the org out of
/// Healthy, and each additional one bite hard.
pub fn composite_score(
    consistency: i64,
    currency: i64,
    liquidity: i64,
    governance: i64,
    cross_team_contradictions: i64,
) -> i64 {
    let weighted = (consistency as f64 * 0.35)
        + (currency as f64 * 0.25)
        + (governance as f64 * 0.20)
        + (liquidity as f64 * 0.20);
    let cross_cap = 100 - cross_team_contradictions * 22;
    clamp100((weighted.round() as i64).min(cross_cap))
}

pub fn grade_of(score: i64) -> &'static str {
    match score {
        85..=100 => "Healthy",
        70..=84 => "Watch",
        50..=69 => "At risk",
        _ => "Critical",
    }
}

// ── the publish circuit breaker (KB3) ───────────────────────────────────

/// Below this, the corpus is serving too many beliefs it no longer holds for us
/// to keep pushing it into someone's company wiki.
pub const PUBLISH_MIN_CURRENCY: i64 = 70;
/// Below this, the review queue has stalled badly enough that "canonical" no
/// longer means "a human looked at it recently".
pub const PUBLISH_MIN_GOVERNANCE: i64 = 50;

/// Why publishing is paused, or `None` when it may proceed.
///
/// The breaker deliberately reads only CURRENCY and GOVERNANCE — the two pillars
/// that say whether what we would publish is *still true* and whether anyone is
/// still *checking*. Liquidity is an org-shape metric (knowledge trapped in one
/// team is a problem, but publishing what we do have is not how it gets worse),
/// and consistency already blocks individual pages upstream: a page built on a
/// memory in an open contradiction recomposes on resolution anyway.
///
/// This is the mechanism that turns the health score from a report into an
/// actuator. Without it, an auto-published wiki is an amplifier: when review
/// stalls — the exact failure the UAT runs found, where the backlog kept being
/// served as truth and nothing went red — the org would broadcast stale beliefs
/// to everyone at machine speed. Pages hold their last published revision with a
/// "verification pending" stamp instead. Silence beats confident staleness.
pub fn publish_block_reason(currency: i64, governance: i64) -> Option<String> {
    if currency < PUBLISH_MIN_CURRENCY {
        return Some(format!(
            "currency {currency} is below the {PUBLISH_MIN_CURRENCY} publish floor — too much of \
             the corpus is deprecated or expired to keep pushing it into an external wiki"
        ));
    }
    if governance < PUBLISH_MIN_GOVERNANCE {
        return Some(format!(
            "governance {governance} is below the {PUBLISH_MIN_GOVERNANCE} publish floor — the \
             review queue has stalled, so 'canonical' no longer means a human checked it"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_corpus_is_vacuously_current() {
        assert_eq!(currency_pillar(0, 0), 100);
    }

    #[test]
    fn currency_tracks_the_stale_share() {
        assert_eq!(currency_pillar(100, 0), 100);
        assert_eq!(currency_pillar(100, 40), 60);
    }

    #[test]
    fn one_cross_team_contradiction_drops_the_org_out_of_healthy() {
        // The cardinal-sin cap: a perfect corpus with ONE unreconciled cross-team
        // conflict must not read "Healthy".
        let score = composite_score(consistency_pillar(1, 1), 100, 100, 100, 1);
        assert!(score < 85, "score {score} still reads Healthy");
        assert_ne!(grade_of(score), "Healthy");
    }

    #[test]
    fn a_stalled_review_queue_pauses_publishing() {
        // Nothing has been reviewed in a week: the breaker must trip.
        let gov = governance_pillar(30, 7 * 24 * 3600);
        assert!(publish_block_reason(100, gov).is_some());
    }

    #[test]
    fn a_rotting_corpus_pauses_publishing() {
        assert!(publish_block_reason(currency_pillar(100, 50), 100).is_some());
    }

    #[test]
    fn a_healthy_org_publishes() {
        assert!(publish_block_reason(95, 90).is_none());
    }

    #[test]
    fn a_new_rule_is_not_a_dead_rule() {
        let month = LIBRARY_DORMANT_DAYS * 24 * 3600;
        // Adopted yesterday, unused: new, not dormant. Flagging this would
        // teach maintainers that the signal cries wolf, and then the signal
        // is worth nothing when a rule really is dead.
        assert!(!rule_is_dormant(24 * 3600, 0));
        // Adopted last quarter, still unused: the org is ignoring it.
        assert!(rule_is_dormant(month + 1, 0));
        // Old but in use: alive, whatever its age.
        assert!(!rule_is_dormant(month * 10, 3));
        // Exactly at the boundary is still too young — strict inequality.
        assert!(!rule_is_dormant(month, 0));
    }

    #[test]
    fn the_gate_slo_is_slower_than_the_promotion_slo() {
        // A rule proposal is a policy question, not a memory promotion. If
        // this ever inverts, someone has confused the two queues. Checked at
        // compile time — the relationship is a fact about two constants, so
        // it should fail the build, not a test run.
        const { assert!(LIBRARY_GATE_SLO_SECS > REVIEW_SLO_SECS) };
    }
}
