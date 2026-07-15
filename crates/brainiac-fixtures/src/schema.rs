//! Serde schemas for the `fixtures/v1/` YAML files. Field names mirror the
//! files verbatim; defaults follow the conventions documented in each file
//! header (visibility=team, status=canonical, language=en).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Deserialize;

fn default_visibility() -> String {
    "team".into()
}
fn default_status() -> String {
    "canonical".into()
}
fn default_language() -> String {
    "en".into()
}
fn default_lifecycle() -> String {
    "shipped".into()
}

// ── org.yaml ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OrgFile {
    pub org: String,
    pub teams: Vec<TeamFx>,
    pub users: Vec<UserFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TeamFx {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct UserFx {
    pub id: String,
    pub email: String,
    pub teams: Vec<String>,
    pub role: String,
}

// ── entities/entities.yaml ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EntitiesFile {
    pub entities: Vec<EntityFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EntityFx {
    pub id: String,
    pub team: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

// ── entities/merges.yaml ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MergesFile {
    pub merge_sets: Vec<MergeSetFx>,
    pub negative_pairs: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MergeSetFx {
    pub canonical: String,
    pub kind: String,
    pub difficulty: String,
    pub members: Vec<String>,
}

// ── memories/gold.yaml ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoriesFile {
    pub memories: Vec<MemoryFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryFx {
    pub id: String,
    pub team: String,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Required when visibility == private.
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub kind: String,
    /// A short label for the archive's row (migration 0023). Optional: the
    /// hand-authored v1 corpus predates it and falls back to `content`.
    #[serde(default)]
    pub title: Option<String>,
    pub content: String,
    /// KB-PLAN D2 — omit for shipped reality (the overwhelming default).
    #[serde(default = "default_lifecycle")]
    pub lifecycle: String,
    /// KB-PLAN D3 — the artifact the statement summarizes, when the gold
    /// memory has one (composition gold uses these to check that pages render
    /// structure instead of paraphrasing it away).
    #[serde(default)]
    pub detail_md: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub relations: Vec<RelationFx>,
    #[serde(default)]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub valid_to: Option<DateTime<Utc>>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Transcript this memory was distilled from.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RelationFx {
    pub src: String,
    pub rel: String,
    pub dst: String,
}

// ── drift/docs.yaml (Level 2 — docs-drift gold) ─────────────────────────

/// The synthetic stale-docs corpus: human-authored documents whose claims are
/// labeled against the gold memory corpus. This is the instrument-calibration
/// gold for cross-documentation intelligence (KB-PLAN follow-up #2) — the
/// drift detector must find the claims restating superseded beliefs WITHOUT
/// attacking the fresh ones that share their vocabulary.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DriftFile {
    pub docs: Vec<DriftDocFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DriftDocFx {
    pub id: String,
    pub title: String,
    /// The document markdown, as a human left it.
    pub body: String,
    /// Every substantive claim in `body`, labeled. Fixture discipline: label
    /// them ALL — an unlabeled claim the detector flags cannot be scored.
    pub gold: Vec<DriftGoldFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DriftGoldFx {
    /// Substring locating the claim in `body` — must match exactly one of the
    /// detector's split claims (the profile refuses the fixture otherwise).
    pub claim: String,
    /// `drifted` (restates a superseded belief) | `aligned` (matches current
    /// canon) | `unmatched` (the corpus knows nothing about it — a harvest
    /// candidate, NOT drift).
    pub label: String,
    /// For `drifted`: the fixture id of the CURRENT memory the detector should
    /// propose as the correction (the terminal of the supersession chain).
    #[serde(default)]
    pub propose: Option<String>,
}

// ── documents/pages.yaml (EVAL.md §2.6 — composition gold) ──────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocumentsFile {
    pub documents: Vec<DocumentFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocumentFx {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default = "default_doc_kind")]
    pub doc_kind: String,
    #[serde(default = "default_doc_visibility")]
    pub visibility: String,
    /// Owning team (a team page's audience; recorded for org pages too).
    pub team: String,
    pub sections: Vec<DocSectionFx>,
    #[serde(default)]
    pub must_cite: bool,
    /// Memories the page's audience is NOT entitled to. Surfacing one is a
    /// build failure — see the zero-tolerance gate in EVAL §2.6.
    #[serde(default)]
    pub forbidden_memories: Vec<String>,
    #[serde(default)]
    pub staleness_case: Option<StalenessCaseFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocSectionFx {
    pub heading: String,
    /// composed | pinned
    pub mode: String,
    #[serde(default)]
    pub bindings: Option<BindingFx>,
    #[serde(default)]
    pub pinned_content: Option<String>,
    /// Claim gists the composed section must contain (semantically matched).
    #[serde(default)]
    pub must_cover: Vec<String>,
    /// The section's knowledge is not-yet-shipped, so the prose must SAY so
    /// (KB-PLAN D2) — a page that renders intent as current architecture is
    /// lying in the most common way a wiki lies.
    #[serde(default)]
    pub must_mark_unshipped: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BindingFx {
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub lifecycle: Vec<String>,
    #[serde(default)]
    pub query: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StalenessCaseFx {
    pub supersede: SupersedeFx,
    #[serde(default)]
    pub expect_dirty: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SupersedeFx {
    pub old: String,
    pub new: String,
}

fn default_doc_kind() -> String {
    "topic_page".into()
}
fn default_doc_visibility() -> String {
    "org".into()
}

// ── transcripts/*.yaml ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TranscriptFx {
    pub id: String,
    pub team: String,
    pub kind: String,
    #[serde(default = "default_language")]
    pub language: String,
    pub turns: Vec<TurnFx>,
    pub gold_memories: Vec<TranscriptGoldFx>,
    #[serde(default)]
    pub distractors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TurnFx {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TranscriptGoldFx {
    pub id: String,
    pub kind: String,
    pub content_gist: String,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub relations: Vec<RelationFx>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub must_extract: bool,
}

// ── contradictions/cases.yaml ───────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContradictionsFile {
    pub cases: Vec<ContradictionFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContradictionFx {
    pub id: String,
    pub memory_a: String,
    pub memory_b: String,
    /// resolved_supersede | resolved_coexist | dismissed
    pub expected: String,
    #[serde(default)]
    pub supersede_direction: Option<String>,
}

// ── temporal/asof.yaml ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TemporalFile {
    pub cases: Vec<TemporalCaseFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TemporalCaseFx {
    pub id: String,
    pub question: String,
    pub as_of: DateTime<Utc>,
    pub expected_memory: String,
}

// ── retrieval/qa.yaml + leak.yaml ───────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QaFile {
    pub queries: Vec<QaQueryFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QaQueryFx {
    pub id: String,
    pub stratum: String,
    pub query: String,
    pub asking_as: AskingAsFx,
    #[serde(default)]
    pub relevant: Vec<GradedFx>,
    #[serde(default)]
    pub as_of: Option<DateTime<Utc>>,
    /// Superseded memories that must not appear in the top 3.
    #[serde(default)]
    pub forbidden_top3: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AskingAsFx {
    pub team: String,
    pub user: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GradedFx {
    pub memory: String,
    pub grade: u8,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LeakFile {
    pub queries: Vec<LeakQueryFx>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LeakQueryFx {
    pub id: String,
    pub query: String,
    pub asking_as: AskingAsFx,
    pub forbidden: Vec<String>,
}
