//! Promotion policy engine (PLAN.md deviation 2: typed Rust rules behind a
//! seam shaped for Cedar). Rules mirror the §2.5 intents; every evaluation
//! returns the rule id that fired for the promotions audit trail.

use brainiac_core::{Memory, MemoryKind, MemoryStatus, PolicyDecision, Visibility};

pub struct PolicyEngine;

/// Pipeline signals that bear on a promotion decision but are not fields of the
/// memory itself. Kept as a struct so new cross-cutting signals (e.g. provenance
/// trust) can be threaded in without touching every call site again.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyContext {
    /// Contradictions opened against this memory during this run. A fresh memory
    /// that conflicts with existing knowledge must not auto-promote into the
    /// retrievable tier before a human resolves the conflict.
    pub open_contradictions: usize,
}

impl PolicyEngine {
    /// Decide a `from → to` promotion for a memory, given surrounding pipeline
    /// context (contradictions just opened against it, etc.).
    pub fn evaluate(
        &self,
        memory: &Memory,
        to: MemoryStatus,
        ctx: &PolicyContext,
    ) -> (PolicyDecision, &'static str) {
        // Anything → canonical ALWAYS requires a human maintainer.
        if to == MemoryStatus::Canonical {
            return (PolicyDecision::NeedsReview, "canonical_requires_maintainer");
        }
        // A memory that just opened a contradiction against existing knowledge is
        // never auto-promoted: contradiction detection is advisory, but retrieval
        // serves candidate memories, so auto-promoting a conflicting (possibly
        // hallucinated or stale) high-confidence claim would circulate poisoned
        // knowledge org-wide before any human resolves the queued contradiction.
        if ctx.open_contradictions > 0 {
            return (PolicyDecision::NeedsReview, "contradiction_pending");
        }
        // High-confidence team-visible pitfalls auto-promote raw → candidate:
        // the cheapest, highest-value knowledge to circulate early.
        if to == MemoryStatus::Candidate
            && memory.status == MemoryStatus::Raw
            && memory.kind == MemoryKind::Pitfall
            && memory.visibility == Visibility::Team
            && memory.confidence.unwrap_or(0.0) >= 0.9
        {
            return (
                PolicyDecision::AutoApproved,
                "pitfall_high_conf_auto_candidate",
            );
        }
        // Very-high-confidence decisions likewise (explicitly stated in the
        // source, not inferred).
        if to == MemoryStatus::Candidate
            && memory.status == MemoryStatus::Raw
            && memory.kind == MemoryKind::Decision
            && memory.confidence.unwrap_or(0.0) >= 0.95
        {
            return (
                PolicyDecision::AutoApproved,
                "decision_explicit_auto_candidate",
            );
        }
        (PolicyDecision::NeedsReview, "default_needs_review")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn mem(kind: MemoryKind, vis: Visibility, conf: f32) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            team_id: None,
            owner_user_id: None,
            visibility: vis,
            status: MemoryStatus::Raw,
            kind,
            content: "x".into(),
            lifecycle: Default::default(),
            detail_md: None,
            valid_from: None,
            valid_to: None,
            superseded_by: None,
            confidence: Some(conf),
            provenance_id: None,
            project_id: None,
            created_at: Utc::now(),
        }
    }

    const CLEAN: PolicyContext = PolicyContext {
        open_contradictions: 0,
    };

    #[test]
    fn canonical_always_needs_review() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.99);
        let (d, rule) = e.evaluate(&m, MemoryStatus::Canonical, &CLEAN);
        assert_eq!(d, PolicyDecision::NeedsReview);
        assert_eq!(rule, "canonical_requires_maintainer");
    }

    #[test]
    fn high_conf_team_pitfall_auto_promotes() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.95);
        let (d, rule) = e.evaluate(&m, MemoryStatus::Candidate, &CLEAN);
        assert_eq!(d, PolicyDecision::AutoApproved);
        assert_eq!(rule, "pitfall_high_conf_auto_candidate");
    }

    #[test]
    fn low_conf_pitfall_needs_review() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.5);
        let (d, _) = e.evaluate(&m, MemoryStatus::Candidate, &CLEAN);
        assert_eq!(d, PolicyDecision::NeedsReview);
    }

    #[test]
    fn org_visible_pitfall_not_auto() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Org, 0.99);
        let (d, _) = e.evaluate(&m, MemoryStatus::Candidate, &CLEAN);
        assert_eq!(d, PolicyDecision::NeedsReview);
    }

    #[test]
    fn an_open_contradiction_blocks_auto_promotion() {
        let e = PolicyEngine;
        // Same memory that WOULD auto-promote when clean...
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.99);
        let ctx = PolicyContext {
            open_contradictions: 1,
        };
        let (d, rule) = e.evaluate(&m, MemoryStatus::Candidate, &ctx);
        assert_eq!(
            d,
            PolicyDecision::NeedsReview,
            "conflict must hold for review"
        );
        assert_eq!(rule, "contradiction_pending");
    }
}
