//! Serde schemas for the `fixtures/v1/` YAML files. Field names mirror the
//! files verbatim; defaults follow the conventions documented in each file
//! header (visibility=team, status=canonical, language=en).

use chrono::{DateTime, Utc};
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

// ── org.yaml ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct OrgFile {
    pub org: String,
    pub teams: Vec<TeamFx>,
    pub users: Vec<UserFx>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamFx {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserFx {
    pub id: String,
    pub email: String,
    pub teams: Vec<String>,
    pub role: String,
}

// ── entities/entities.yaml ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct EntitiesFile {
    pub entities: Vec<EntityFx>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntityFx {
    pub id: String,
    pub team: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

// ── entities/merges.yaml ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MergesFile {
    pub merge_sets: Vec<MergeSetFx>,
    pub negative_pairs: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeSetFx {
    pub canonical: String,
    pub kind: String,
    pub difficulty: String,
    pub members: Vec<String>,
}

// ── memories/gold.yaml ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MemoriesFile {
    pub memories: Vec<MemoryFx>,
}

#[derive(Debug, Clone, Deserialize)]
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
    pub content: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct RelationFx {
    pub src: String,
    pub rel: String,
    pub dst: String,
}

// ── transcripts/*.yaml ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TurnFx {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct ContradictionsFile {
    pub cases: Vec<ContradictionFx>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TemporalFile {
    pub cases: Vec<TemporalCaseFx>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemporalCaseFx {
    pub id: String,
    pub question: String,
    pub as_of: DateTime<Utc>,
    pub expected_memory: String,
}

// ── retrieval/qa.yaml + leak.yaml ───────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct QaFile {
    pub queries: Vec<QaQueryFx>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct AskingAsFx {
    pub team: String,
    pub user: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GradedFx {
    pub memory: String,
    pub grade: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeakFile {
    pub queries: Vec<LeakQueryFx>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeakQueryFx {
    pub id: String,
    pub query: String,
    pub asking_as: AskingAsFx,
    pub forbidden: Vec<String>,
}
