//! Domain types mirroring ARCHITECTURE.md §2 (schema core).
//!
//! These are the in-process representations; the store crate maps them to
//! Postgres rows. Fixture ids are stable strings (`mem-pay-0042`) while
//! database ids are UUIDs — the eval harness owns that mapping, so nothing
//! here depends on fixture naming.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Lifecycle of a knowledge unit (ARCHITECTURE.md §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Raw,
    Candidate,
    Canonical,
    Deprecated,
    Rejected,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Candidate => "candidate",
            Self::Canonical => "canonical",
            Self::Deprecated => "deprecated",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "raw" => Some(Self::Raw),
            "candidate" => Some(Self::Candidate),
            "canonical" => Some(Self::Canonical),
            "deprecated" => Some(Self::Deprecated),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// Three-tier visibility model. RLS keys off this + team membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Team,
    Org,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Team => "team",
            Self::Org => "org",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "private" => Some(Self::Private),
            "team" => Some(Self::Team),
            "org" => Some(Self::Org),
            _ => None,
        }
    }
}

/// Knowledge-unit kind; promotion policy differentiates on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Decision,
    Pattern,
    Pitfall,
    Howto,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Decision => "decision",
            Self::Pattern => "pattern",
            Self::Pitfall => "pitfall",
            Self::Howto => "howto",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fact" => Some(Self::Fact),
            "decision" => Some(Self::Decision),
            "pattern" => Some(Self::Pattern),
            "pitfall" => Some(Self::Pitfall),
            "howto" => Some(Self::Howto),
            _ => None,
        }
    }

    /// Default freshness budget per kind (days) — how long extracted
    /// knowledge stays presumed-true before it needs re-verification.
    /// Procedures rot fastest; decisions stay binding longest.
    pub fn default_ttl_days(&self) -> u32 {
        match self {
            Self::Fact => 365,
            Self::Decision => 540,
            Self::Pattern => 540,
            Self::Pitfall => 365,
            Self::Howto => 180,
        }
    }
}

/// Who/what produced an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    Pipeline,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Pipeline => "pipeline",
        }
    }
}

// ---------------------------------------------------------------------------
// Identity / principal
// ---------------------------------------------------------------------------

/// The verified caller identity a request runs under. RLS derives from this;
/// retrieval and composition NEVER widen beyond it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub org_id: Uuid,
    pub user_id: Uuid,
    /// Teams the user belongs to (SCIM-synced in production; config-stubbed in v0).
    pub team_ids: Vec<Uuid>,
}

impl Principal {
    /// Can this principal read a knowledge unit with the given scoping?
    /// Single source of truth for the visibility rule — the SQL RLS policy is
    /// its mirror, and eval leak-tests exercise both paths.
    pub fn can_read(
        &self,
        org_id: Uuid,
        team_id: Option<Uuid>,
        owner_user_id: Option<Uuid>,
        visibility: Visibility,
    ) -> bool {
        if org_id != self.org_id {
            return false;
        }
        match visibility {
            Visibility::Org => true,
            Visibility::Team => team_id.is_some_and(|t| self.team_ids.contains(&t)),
            Visibility::Private => owner_user_id.is_some_and(|u| u == self.user_id),
        }
    }
}

// ---------------------------------------------------------------------------
// Provenance & sources
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub id: Uuid,
    pub org_id: Uuid,
    pub actor_kind: ActorKind,
    /// User id, agent name, or worker name.
    pub actor_id: String,
    /// e.g. `"qwen:qwen-max:2026-01"` when LLM-produced.
    pub model_ref: Option<String>,
    pub source_id: Option<Uuid>,
    pub pipeline_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    /// session_transcript | repo | doc | manual
    pub kind: String,
    pub external_ref: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Memories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub visibility: Visibility,
    pub status: MemoryStatus,
    pub kind: MemoryKind,
    /// Distilled natural-language statement.
    pub content: String,
    pub valid_from: Option<DateTime<Utc>>,
    /// `None` = still valid.
    pub valid_to: Option<DateTime<Utc>>,
    /// Forward pointer set on deprecation/supersession.
    pub superseded_by: Option<Uuid>,
    /// Extractor confidence 0..1.
    pub confidence: Option<f32>,
    pub provenance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Graph: entities, canonical entities, links, edges
// ---------------------------------------------------------------------------

/// Raw, team-scoped node. IMMUTABLE after creation — merges happen via links.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub name: String,
    /// service | repo | tech | feature | concept | team
    pub kind: String,
    pub aliases: Vec<String>,
    pub provenance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Org-level merge target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEntity {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub kind: String,
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// How an entity link was established (audit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkMethod {
    EmbeddingBlock,
    LlmAdjudicated,
    Human,
}

impl LinkMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EmbeddingBlock => "embedding_block",
            Self::LlmAdjudicated => "llm_adjudicated",
            Self::Human => "human",
        }
    }
}

/// Soft merge: reversible, auditable (`same_as` edge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLink {
    pub entity_id: Uuid,
    pub canonical_id: Uuid,
    pub confidence: f32,
    pub method: LinkMethod,
    pub confirmed_by: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub org_id: Uuid,
    pub src_entity: Uuid,
    pub dst_entity: Uuid,
    /// uses | depends_on | owns | deprecates | relates_to
    pub relation: String,
    /// Evidence: the memory this edge came from.
    pub memory_id: Option<Uuid>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Governance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    AutoApproved,
    NeedsReview,
    Denied,
}

impl PolicyDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AutoApproved => "auto_approved",
            Self::NeedsReview => "needs_review",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionStatus {
    Open,
    ResolvedSupersede,
    ResolvedCoexist,
    Dismissed,
}

impl ContradictionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ResolvedSupersede => "resolved_supersede",
            Self::ResolvedCoexist => "resolved_coexist",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "resolved_supersede" => Some(Self::ResolvedSupersede),
            "resolved_coexist" => Some(Self::ResolvedCoexist),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn principal_visibility_matrix() {
        let me = Principal {
            org_id: uuid(1),
            user_id: uuid(2),
            team_ids: vec![uuid(3)],
        };
        // org-visible in my org: yes
        assert!(me.can_read(uuid(1), None, None, Visibility::Org));
        // org-visible in ANOTHER org: never
        assert!(!me.can_read(uuid(9), None, None, Visibility::Org));
        // team-visible, my team: yes; other team: no; missing team: no
        assert!(me.can_read(uuid(1), Some(uuid(3)), None, Visibility::Team));
        assert!(!me.can_read(uuid(1), Some(uuid(4)), None, Visibility::Team));
        assert!(!me.can_read(uuid(1), None, None, Visibility::Team));
        // private: only the owner
        assert!(me.can_read(uuid(1), None, Some(uuid(2)), Visibility::Private));
        assert!(!me.can_read(uuid(1), None, Some(uuid(5)), Visibility::Private));
    }

    #[test]
    fn enum_round_trips() {
        for s in ["raw", "candidate", "canonical", "deprecated", "rejected"] {
            assert_eq!(
                MemoryStatus::parse(s).map(|v| v.as_str()),
                Some(s),
                "status {s}"
            );
        }
        for s in ["private", "team", "org"] {
            assert_eq!(Visibility::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["fact", "decision", "pattern", "pitfall", "howto"] {
            assert_eq!(MemoryKind::parse(s).map(|v| v.as_str()), Some(s));
        }
    }
}
