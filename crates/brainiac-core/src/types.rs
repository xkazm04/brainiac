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

/// Where a memory's claim sits relative to shipped reality (KB-PLAN D2).
///
/// Orthogonal to [`MemoryStatus`] (governance: has a human signed it?) and to
/// temporal validity (when did the belief hold?). Neither can answer the
/// question a reader of a generated page asks first — *is this how the system
/// works today, or how we intend it to work?* A decision to adopt Kafka is
/// canonical, currently valid, and still describes nothing that exists in
/// production. Docs that blur the two are the most common way a wiki lies.
///
/// [`Self::Shipped`] is the default: pre-existing memories, and anything the
/// transcript does not mark as intent, describe the world as captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// In product: the claim describes deployed, observable reality.
    #[default]
    Shipped,
    /// On its way: decided and underway, not yet fully in production.
    InFlight,
    /// Considered/decided in principle, no implementation started.
    Proposed,
}

impl Lifecycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shipped => "shipped",
            Self::InFlight => "in_flight",
            Self::Proposed => "proposed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shipped" => Some(Self::Shipped),
            "in_flight" => Some(Self::InFlight),
            "proposed" => Some(Self::Proposed),
            _ => None,
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "human" => Some(Self::Human),
            "agent" => Some(Self::Agent),
            "pipeline" => Some(Self::Pipeline),
            _ => None,
        }
    }
}

/// Kind of a graph entity (ARCHITECTURE.md §2). The legal values used to live
/// only in a comment on [`Entity::kind`]/[`CanonicalEntity::kind`] (raw
/// `String`s), so extraction stored typo'd kinds silently; this is the typed
/// vocabulary the extraction firewall validates against. Wire/DB stay strings
/// via [`Self::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Service,
    Repo,
    Tech,
    Feature,
    /// The permissive fallback: an unrecognized or unspecified entity kind is
    /// coerced here rather than stored raw, so no typo ever reaches the DB.
    #[default]
    Concept,
    Team,
}

impl EntityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Repo => "repo",
            Self::Tech => "tech",
            Self::Feature => "feature",
            Self::Concept => "concept",
            Self::Team => "team",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "service" => Some(Self::Service),
            "repo" => Some(Self::Repo),
            "tech" => Some(Self::Tech),
            "feature" => Some(Self::Feature),
            "concept" => Some(Self::Concept),
            "team" => Some(Self::Team),
            _ => None,
        }
    }
}

/// Directed relation between two entities (ARCHITECTURE.md §2). Formerly a raw
/// `String` on [`Edge::relation`] with legal values in a comment; the
/// extraction firewall now parses against this and DROPS unknown relations
/// (a bogus edge is worse than a coerced entity kind — it invents graph
/// structure). Wire/DB stay strings via [`Self::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRelation {
    Uses,
    DependsOn,
    Owns,
    Deprecates,
    RelatesTo,
}

impl EdgeRelation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uses => "uses",
            Self::DependsOn => "depends_on",
            Self::Owns => "owns",
            Self::Deprecates => "deprecates",
            Self::RelatesTo => "relates_to",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "uses" => Some(Self::Uses),
            "depends_on" => Some(Self::DependsOn),
            "owns" => Some(Self::Owns),
            "deprecates" => Some(Self::Deprecates),
            "relates_to" => Some(Self::RelatesTo),
            _ => None,
        }
    }
}

/// Origin of a [`Source`] (ARCHITECTURE.md §2). The values actually written by
/// the ingest surfaces are `session_transcript` (fixtures/worker transcripts)
/// and `manual` (REST/MCP memory_add); `repo` and `doc` round out the
/// documented vocabulary. Wire/DB stay strings via [`Self::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    SessionTranscript,
    Repo,
    Doc,
    Manual,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionTranscript => "session_transcript",
            Self::Repo => "repo",
            Self::Doc => "doc",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "session_transcript" => Some(Self::SessionTranscript),
            "repo" => Some(Self::Repo),
            "doc" => Some(Self::Doc),
            "manual" => Some(Self::Manual),
            _ => None,
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
    /// Wire/DB string; legal values are [`SourceKind`].
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
    /// Distilled natural-language statement. Stays the retrieval surface (FTS +
    /// embeddings point here) — [`Self::detail_md`] is evidence, not the claim.
    pub content: String,
    /// Where the claim sits relative to shipped reality (KB-PLAN D2).
    pub lifecycle: Lifecycle,
    /// Optional structure the sentence summarizes: a code block, table, or
    /// config snippet lifted from the source (KB-PLAN D3). `None` for the vast
    /// majority of memories — a claim with no artifact behind it.
    pub detail_md: Option<String>,
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
    /// Wire/DB string; legal values are [`EntityKind`].
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
    /// Wire/DB string; legal values are [`EntityKind`].
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "embedding_block" => Some(Self::EmbeddingBlock),
            "llm_adjudicated" => Some(Self::LlmAdjudicated),
            "human" => Some(Self::Human),
            _ => None,
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
    /// Wire/DB string; legal values are [`EdgeRelation`].
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto_approved" => Some(Self::AutoApproved),
            "needs_review" => Some(Self::NeedsReview),
            "denied" => Some(Self::Denied),
            _ => None,
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

    #[test]
    fn vocabulary_enum_round_trips() {
        for s in ["human", "agent", "pipeline"] {
            assert_eq!(ActorKind::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["service", "repo", "tech", "feature", "concept", "team"] {
            assert_eq!(EntityKind::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["uses", "depends_on", "owns", "deprecates", "relates_to"] {
            assert_eq!(EdgeRelation::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["session_transcript", "repo", "doc", "manual"] {
            assert_eq!(SourceKind::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["embedding_block", "llm_adjudicated", "human"] {
            assert_eq!(LinkMethod::parse(s).map(|v| v.as_str()), Some(s));
        }
        for s in ["auto_approved", "needs_review", "denied"] {
            assert_eq!(PolicyDecision::parse(s).map(|v| v.as_str()), Some(s));
        }
    }

    #[test]
    fn unknown_vocabulary_is_rejected_and_entity_kind_defaults_to_concept() {
        assert_eq!(EntityKind::parse("serrvice"), None, "typos don't parse");
        assert_eq!(EdgeRelation::parse("uzes"), None);
        assert_eq!(SourceKind::parse("chat"), None);
        // The permissive fallback the extraction firewall coerces to.
        assert_eq!(EntityKind::default(), EntityKind::Concept);
        assert_eq!(EntityKind::default().as_str(), "concept");
    }
}
