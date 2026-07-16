//! Library-layer domain types (docs/LIBRARY-PLAN.md LB0, migration 0028).
//!
//! Split from `types.rs` so the normative vocabulary lives in one place; the
//! store's `library` module is the only writer, every enum mirrors a CHECK
//! constraint in the migration, and `lib.rs` re-exports everything so callers
//! keep the flat `brainiac_core::Standard` paths.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Library layer (docs/LIBRARY-PLAN.md LB0): the normative vocabulary ──────
//
// The rule is the atom (L1): a Standard is ONE rule with a typed identity
// (stack → category → slug), a one-sentence statement, and a lifecycle. Skills
// are versioned bundles (L4). Everything here mirrors a checked constraint in
// migration 0028 — parse() rejecting a value the database would reject too.

/// How strongly a standard binds. Informational in v1 — the Library never
/// blocks a merge (L-never #3) — but agents read it to weight self-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Mandatory,
    #[default]
    Recommended,
    Experimental,
}

impl Enforcement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mandatory => "mandatory",
            Self::Recommended => "recommended",
            Self::Experimental => "experimental",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mandatory" => Some(Self::Mandatory),
            "recommended" => Some(Self::Recommended),
            "experimental" => Some(Self::Experimental),
            _ => None,
        }
    }
}

/// A standard's lifecycle. `Proposed` is a candidate awaiting the gate;
/// leaving it requires a named human (`adopted_by`) — the database refuses
/// otherwise, not just the API. `Rejected` is a candidate a maintainer said
/// no to — kept, not deleted, because the mining sweep dedups against it
/// (rejection is knowledge; LB3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardLifecycle {
    #[default]
    Proposed,
    Adopted,
    Deprecated,
    Rejected,
}

impl StandardLifecycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Adopted => "adopted",
            Self::Deprecated => "deprecated",
            Self::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "proposed" => Some(Self::Proposed),
            "adopted" => Some(Self::Adopted),
            "deprecated" => Some(Self::Deprecated),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// What a provenance row points at: the memory (incident, decision) or the
/// practice divergence that motivated the rule. The only OTHER legal origin is
/// a decree carrying the decreeing human's id on the standard itself — there
/// is deliberately no third kind (L-never #4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardProvenanceKind {
    Memory,
    Divergence,
}

impl StandardProvenanceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Divergence => "divergence",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "memory" => Some(Self::Memory),
            "divergence" => Some(Self::Divergence),
            _ => None,
        }
    }
}

/// Who created a standard: a human at the console, the mining sweep, or an
/// agent proposing mid-session. Triage renders this — a maintainer deciding
/// whether to trust a rule must see who is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardOrigin {
    #[default]
    Human,
    Sweep,
    Agent,
}

impl StandardOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Sweep => "sweep",
            Self::Agent => "agent",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "human" => Some(Self::Human),
            "sweep" => Some(Self::Sweep),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Standard {
    pub id: Uuid,
    pub org_id: Uuid,
    pub origin: StandardOrigin,
    pub stack: String,
    pub category: String,
    pub slug: String,
    pub statement: String,
    pub rationale: Option<String>,
    /// Good/bad examples — same vocabulary as `Memory::detail_md`: copied
    /// verbatim onto every surface, never re-typed by a model.
    pub detail_md: Option<String>,
    pub enforcement: Enforcement,
    pub lifecycle: StandardLifecycle,
    /// The named human who ratified it out of `proposed`.
    pub adopted_by: Option<Uuid>,
    pub adopted_at: Option<DateTime<Utc>>,
    /// Set only for an evidence-free rule a named human signed for.
    pub decreed_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardProvenance {
    pub standard_id: Uuid,
    pub kind: StandardProvenanceKind,
    pub ref_id: Uuid,
}

/// A skill's catalog maturity. Only `Published` versions are ever served to
/// agents — a draft nobody signed must not reach a coding agent (the same
/// refusal the document layer makes for unsigned pages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMaturity {
    #[default]
    Draft,
    Published,
    Deprecated,
}

impl SkillMaturity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Deprecated => "deprecated",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "published" => Some(Self::Published),
            "deprecated" => Some(Self::Deprecated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub maturity: SkillMaturity,
    pub current_version: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

/// One immutable version of a skill bundle (the open agent-skill format:
/// manifest front-matter + markdown body + auxiliary resources).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub semver: String,
    pub manifest: serde_json::Value,
    pub content_md: String,
    pub resources: serde_json::Value,
    /// The named human who published it; `None` is a draft, never served.
    pub published_by: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// What kind of Library artifact a usage event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryArtifactKind {
    Standard,
    Skill,
}

impl LibraryArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Skill => "skill",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "standard" => Some(Self::Standard),
            "skill" => Some(Self::Skill),
            _ => None,
        }
    }
}

/// A usage signal. `Fetch` = pulled by an agent/console; `Check` = an agent
/// compared its work against a standard; `Apply` = a skill was actually run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryUsageEvent {
    Fetch,
    Check,
    Apply,
}

impl LibraryUsageEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fetch => "fetch",
            Self::Check => "check",
            Self::Apply => "apply",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fetch" => Some(Self::Fetch),
            "check" => Some(Self::Check),
            "apply" => Some(Self::Apply),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_vocabulary_round_trips_and_matches_migration_checks() {
        // Every string here must stay equal to the corresponding CHECK
        // constraint list in migration 0028 — parse() and the database must
        // reject the same values.
        for s in ["mandatory", "recommended", "experimental"] {
            assert_eq!(Enforcement::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["proposed", "adopted", "deprecated", "rejected"] {
            assert_eq!(StandardLifecycle::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["memory", "divergence"] {
            assert_eq!(
                StandardProvenanceKind::parse(s).map(|v| v.as_str()),
                Some(s)
            );
        }
        for s in ["draft", "published", "deprecated"] {
            assert_eq!(SkillMaturity::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["standard", "skill"] {
            assert_eq!(LibraryArtifactKind::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["human", "sweep", "agent"] {
            assert_eq!(StandardOrigin::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["fetch", "check", "apply"] {
            assert_eq!(LibraryUsageEvent::parse(s).map(|v| v.as_str()), Some(s));
        }
        // The safe defaults: a rule enters as proposed/recommended, a skill as draft.
        assert_eq!(StandardLifecycle::default(), StandardLifecycle::Proposed);
        assert_eq!(Enforcement::default(), Enforcement::Recommended);
        assert_eq!(SkillMaturity::default(), SkillMaturity::Draft);
        // There is no third provenance kind — a "decree" is a signed column on
        // the standard, not a provenance row (L-never #4 relies on this).
        assert_eq!(StandardProvenanceKind::parse("decree"), None);
    }
}
