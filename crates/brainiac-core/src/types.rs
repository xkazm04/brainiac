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
    /// The project a managed key is scoped to (migration 0034), threaded into
    /// the `app.project_id` RLS GUC by `Store::scoped_tx`. `None` for env
    /// tokens and org-wide keys — a NULL/empty GUC that the project-isolation
    /// policy (migration 0040) reads as "no project scope". Purely advisory
    /// until a project opts into `isolated`; only then does it gate reads.
    pub project_id: Option<Uuid>,
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
    /// The application/domain this claim is about (PROJECT-PLAN PR0);
    /// `None` = org-shared. Orthogonal to `team_id`: team answers WHO,
    /// project answers WHAT ABOUT.
    pub project_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Documents (ARCHITECTURE.md §8 — the knowledge base layer)
//
// A document holds NO knowledge. It is a compiled view over canonical memories,
// regenerated when they change. Every type here exists to keep that true: a
// section is either bound to a memory query (composed) or owned prose (pinned);
// a revision records the exact memory ids it was built from, so a claim with no
// backing memory is detectable — and therefore gateable — as a hallucination.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// Auto-scaffolded around one canonical entity (KB2).
    EntityPage,
    #[default]
    TopicPage,
    Runbook,
    Onboarding,
    /// A time-windowed projection — "what changed this week" — recomposed on
    /// cadence (migration 0027). Same compose pipeline, same review gate, same
    /// reader; the WINDOW is the only thing that makes it a digest.
    Digest,
    /// The org's adopted rules for one tech stack, rendered as a page
    /// (LIBRARY-PLAN L8, migration 0031). Rides the whole document layer —
    /// dirty-marking, revisions, review, the health breaker, Confluence — with
    /// ONE difference: it is projected DETERMINISTICALLY, never composed. A
    /// rule's statement is a sentence a named human ratified; handing it to a
    /// model to re-word would fork the org's own commitment.
    StandardsPage,
}

impl DocKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EntityPage => "entity_page",
            Self::TopicPage => "topic_page",
            Self::Runbook => "runbook",
            Self::Onboarding => "onboarding",
            Self::Digest => "digest",
            Self::StandardsPage => "standards_page",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "entity_page" => Some(Self::EntityPage),
            "topic_page" => Some(Self::TopicPage),
            "runbook" => Some(Self::Runbook),
            "onboarding" => Some(Self::Onboarding),
            "digest" => Some(Self::Digest),
            "standards_page" => Some(Self::StandardsPage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocStatus {
    #[default]
    Draft,
    Published,
    Archived,
}

impl DocStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "published" => Some(Self::Published),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Composed = machine-owned (regenerates); Pinned = human-owned (never touched).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionMode {
    Composed,
    Pinned,
}

impl SectionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Composed => "composed",
            Self::Pinned => "pinned",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "composed" => Some(Self::Composed),
            "pinned" => Some(Self::Pinned),
            _ => None,
        }
    }
}

/// What a composed section pulls in. This is a *query*, not a list of memories:
/// the page's content is whatever currently satisfies it, which is precisely why
/// the page cannot go stale while the corpus moves on.
/// NOTE: `Default` is implemented by hand, not derived — see the impl below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionBinding {
    /// Canonical entity ids the section is anchored to.
    #[serde(default)]
    pub entities: Vec<Uuid>,
    /// Memory kinds to admit; empty = all.
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    /// Lifecycle facets to admit; empty = all. The reason a page can carry a
    /// "shipped" section and a separate "on its way" section without the reader
    /// ever having to guess which is which (KB-PLAN D2).
    #[serde(default)]
    pub lifecycle: Vec<Lifecycle>,
    /// Free-text retrieval query; empty = pure entity/kind binding.
    #[serde(default)]
    pub query: String,
    /// Time window in days: when set, the section additionally SOURCES the
    /// org's recently changed canonical memories (newest first) — the binding
    /// shape a digest is made of. `None` for every ordinary page; optional so
    /// each binding stored before migration 0027 deserializes unchanged.
    #[serde(default)]
    pub window_days: Option<i64>,
    /// The tech stack whose ADOPTED rules this section projects (LIBRARY-PLAN
    /// L8). Set only on a `standards_page`; when present the section is
    /// rendered deterministically from the Library and no model is called —
    /// `entities` / `kinds` / `query` are not consulted. `None` for every
    /// ordinary page, so bindings stored before migration 0031 deserialize
    /// unchanged.
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
}

fn default_max_items() -> usize {
    12
}

/// Hand-written so the struct has ONE default, not two that disagree.
///
/// `#[derive(Default)]` gave `max_items = 0` (integer Default) while
/// `#[serde(default = "default_max_items")]` gave 12 — so a binding built the
/// idiomatic way (`SectionBinding { query, ..Default::default() }`, the pattern
/// used for other structs all over the pipeline) silently got a cap of 0. compose
/// then derives `LIMIT max_items * 3` = 0, fan-out `k = max_items * 2` = 0, and
/// `kept.truncate(0)`, rendering an EMPTY page section and reporting success. It
/// was latent only because every current caller happens to set `max_items`
/// explicitly, and `SectionBinding` is `pub` in brainiac-core, so any new caller
/// (or external consumer) would have walked into it.
impl Default for SectionBinding {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            kinds: Vec::new(),
            lifecycle: Vec::new(),
            query: String::new(),
            window_days: None,
            stack: None,
            max_items: default_max_items(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub slug: String,
    pub title: String,
    pub visibility: Visibility,
    pub doc_kind: DocKind,
    pub status: DocStatus,
    pub current_revision: Option<Uuid>,
    /// The application/domain this page is about (PROJECT-PLAN PR4); `None` =
    /// an org-wide page. A stamped page composes from its project's memories
    /// plus org-shared ones — never another project's.
    pub project_id: Option<Uuid>,
    /// Set when a memory this page depends on changed; cleared on recompose.
    pub dirty_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSection {
    pub id: Uuid,
    pub document_id: Uuid,
    pub position: i32,
    pub heading: String,
    pub mode: SectionMode,
    /// `Some` iff composed.
    pub binding: Option<SectionBinding>,
    /// `Some` iff pinned. Byte-preserved across regeneration — the eval gates it.
    pub pinned_content: Option<String>,
}

/// What the policy engine decided about a freshly composed revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionPolicy {
    /// Every claim traced, no previously published claim dropped → ship it.
    AutoPublished,
    /// A human must look: claims disappeared, or the page is structurally new.
    NeedsReview,
    Rejected,
}

impl RevisionPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AutoPublished => "auto_published",
            Self::NeedsReview => "needs_review",
            Self::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto_published" => Some(Self::AutoPublished),
            "needs_review" => Some(Self::NeedsReview),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRevision {
    pub id: Uuid,
    pub document_id: Uuid,
    pub content_md: String,
    /// The provenance closure: exactly the memories this markdown was built
    /// from. A claim in `content_md` not backed by one of these is a
    /// hallucination by definition.
    pub composed_from: Vec<Uuid>,
    pub trigger: String,
    pub policy_decision: RevisionPolicy,
    pub reviewed_by: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Sampled runtime citation-faithfulness verdicts (0036). `None` = not
    /// judged — the judge is best-effort, and absence of a verdict is not a
    /// verdict. When present: which paragraphs were checked and which cite a
    /// real memory while misstating it, for the reviewer to look at first.
    pub faithfulness: Option<serde_json::Value>,
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
            project_id: None,
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
    fn section_binding_has_one_default_not_two() {
        // The derived Default gave max_items = 0 while serde's gave 12, so a
        // `..Default::default()` binding silently composed an EMPTY section
        // (LIMIT 0, k = 0, truncate(0)) and reported success.
        assert_eq!(
            SectionBinding::default().max_items,
            default_max_items(),
            "the struct default must not diverge from the serde default"
        );
        // Both construction paths must agree.
        let from_json: SectionBinding =
            serde_json::from_str(r#"{"query":"retry"}"#).expect("binding parses");
        assert_eq!(from_json.max_items, SectionBinding::default().max_items);
        // ...and an explicit value still wins.
        let explicit: SectionBinding =
            serde_json::from_str(r#"{"query":"retry","max_items":3}"#).expect("binding parses");
        assert_eq!(explicit.max_items, 3);
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
