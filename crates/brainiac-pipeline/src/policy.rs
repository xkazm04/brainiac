//! Promotion policy engine (PLAN.md deviation 2: typed Rust rules behind a
//! seam shaped for Cedar). Rules mirror the §2.5 intents; every evaluation
//! returns the rule id that fired for the promotions audit trail.

use brainiac_core::{Memory, MemoryKind, MemoryStatus, PolicyDecision, Visibility};

pub struct PolicyEngine;

impl PolicyEngine {
    /// Decide a `from → to` promotion for a memory.
    pub fn evaluate(&self, memory: &Memory, to: MemoryStatus) -> (PolicyDecision, &'static str) {
        // Anything → canonical ALWAYS requires a human maintainer.
        if to == MemoryStatus::Canonical {
            return (PolicyDecision::NeedsReview, "canonical_requires_maintainer");
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
            valid_from: None,
            valid_to: None,
            superseded_by: None,
            confidence: Some(conf),
            provenance_id: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn canonical_always_needs_review() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.99);
        let (d, rule) = e.evaluate(&m, MemoryStatus::Canonical);
        assert_eq!(d, PolicyDecision::NeedsReview);
        assert_eq!(rule, "canonical_requires_maintainer");
    }

    #[test]
    fn high_conf_team_pitfall_auto_promotes() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.95);
        let (d, rule) = e.evaluate(&m, MemoryStatus::Candidate);
        assert_eq!(d, PolicyDecision::AutoApproved);
        assert_eq!(rule, "pitfall_high_conf_auto_candidate");
    }

    #[test]
    fn low_conf_pitfall_needs_review() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Team, 0.5);
        let (d, _) = e.evaluate(&m, MemoryStatus::Candidate);
        assert_eq!(d, PolicyDecision::NeedsReview);
    }

    #[test]
    fn org_visible_pitfall_not_auto() {
        let e = PolicyEngine;
        let m = mem(MemoryKind::Pitfall, Visibility::Org, 0.99);
        let (d, _) = e.evaluate(&m, MemoryStatus::Candidate);
        assert_eq!(d, PolicyDecision::NeedsReview);
    }
}
