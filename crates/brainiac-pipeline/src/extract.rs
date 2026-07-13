//! Extract stage: source text → BYOM call → validated raw memories +
//! entities + relations, all provenance-stamped (stage 2 of §3).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{ActorKind, EdgeRelation, EntityKind, MemoryKind, MemoryStatus, Visibility};
use brainiac_gateway::{ChatProvider, ChatRequest};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Versioned prompt — changes here must not drop eval scores past the gate
/// (EVAL.md §3.2), which is why it lives in code, not config.
pub const EXTRACT_SYSTEM_PROMPT_V1: &str = "\
You distill organizational knowledge from a work transcript into discrete memory units.

Extract ONLY durable, reusable knowledge: facts, decisions, patterns, pitfalls, howtos.
NEVER extract small talk, speculation, or ideas that were explicitly rejected in the conversation.

Respond with ONLY a JSON object:
{\"memories\":[{
  \"kind\":\"fact|decision|pattern|pitfall|howto\",
  \"content\":\"one self-contained natural-language statement\",
  \"visibility\":\"team|org\",
  \"confidence\":0.0,
  \"entities\":[{\"name\":\"...\",\"kind\":\"service|repo|tech|feature|concept|team\",\"aliases\":[\"...\"]}],
  \"relations\":[{\"src\":\"entity name\",\"rel\":\"uses|depends_on|owns|deprecates|relates_to\",\"dst\":\"entity name\"}]
}]}

Rules: entity names verbatim as the team says them (do NOT normalize across teams);
aliases are OTHER surface forms the transcript uses for that SAME entity (acronyms,
short names, spelled-out forms) — omit or leave [] when the entity is named only one
way, never invent synonyms; relations only between listed entities; confidence
reflects how explicitly the transcript supports the statement.";

#[derive(Debug, Deserialize)]
struct ExtractionOutput {
    #[serde(default)]
    memories: Vec<ExtractedMemory>,
}

#[derive(Debug, Deserialize)]
struct ExtractedMemory {
    kind: String,
    content: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    entities: Vec<ExtractedEntity>,
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

#[derive(Debug, Deserialize)]
struct ExtractedEntity {
    name: String,
    #[serde(default = "default_entity_kind")]
    kind: String,
    #[serde(default)]
    aliases: Vec<String>,
}

impl ExtractedEntity {
    /// Surface forms to store as aliases: trimmed, non-empty, de-duplicated,
    /// and never the entity's own name (that is the canonical anchor).
    fn clean_aliases(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in &self.aliases {
            let a = a.trim();
            if a.is_empty() || a.eq_ignore_ascii_case(self.name.trim()) {
                continue;
            }
            if !out.iter().any(|e: &String| e.eq_ignore_ascii_case(a)) {
                out.push(a.to_string());
            }
        }
        out
    }
}

fn default_entity_kind() -> String {
    "concept".into()
}

#[derive(Debug, Deserialize)]
struct ExtractedRelation {
    src: String,
    rel: String,
    dst: String,
}

#[derive(Debug, Default)]
pub struct ExtractStats {
    pub memories_written: usize,
    pub entities_created: Vec<Uuid>,
    pub memory_ids: Vec<Uuid>,
    pub dropped_invalid: usize,
    /// Memories skipped because an identical (source, content) row already
    /// existed — the idempotency guard against a redelivered retry or an
    /// overlap region re-extracted by an adjacent chunk.
    pub deduped: usize,
    /// Chunks the source was split into = number of primary extract calls.
    pub chunks: usize,
    /// First responses that failed to parse and triggered a repair re-prompt.
    pub parse_failures: usize,
    /// Repair re-prompts that recovered a parseable response. Total LLM calls
    /// for the source = `chunks + repairs`.
    pub repairs: usize,
    /// Model ref the provider actually reported (from the first chunk's call),
    /// recorded on the pipeline_runs row. `None` only for an empty source.
    pub model_ref: Option<String>,
}

/// Ceiling on the extractor's completion length (tokens). One place so the
/// primary call and the repair re-prompt stay in lockstep, and so the chunk
/// budget below can reason about the input/output split of a context window.
const MAX_EXTRACT_TOKENS: u32 = 4096;

// ── chunking (ARCHITECTURE.md §5 rows 1-2) ──────────────────────────────
//
// The whole raw source used to go to the model in a single call capped at
// MAX_EXTRACT_TOKENS — long agent sessions silently truncated exactly the
// transcripts most likely to contain decisions (the tail). Oversized sources
// are split into overlapping char windows so no region is dropped; the
// (source, content) dedup above collapses the memories an overlap re-extracts.

/// Coarse chars-per-token for English transcripts (~4). Only used to size
/// chunks; exactness doesn't matter because the window carries headroom.
const CHARS_PER_TOKEN: usize = 4;

/// Target source tokens per chunk. With MAX_EXTRACT_TOKENS of completion plus
/// the system prompt, ~3000 source tokens keeps a call comfortably inside a
/// modest (8k) context window.
const CHUNK_TARGET_TOKENS: usize = 3000;

/// A source at or under this many chars is sent whole — no behavior change for
/// the common short session. ~12000 chars.
const MAX_CHUNK_CHARS: usize = CHUNK_TARGET_TOKENS * CHARS_PER_TOKEN;

/// Overlap between consecutive windows so a decision straddling a boundary is
/// seen intact by at least one chunk; dedup collapses the repeat.
const CHUNK_OVERLAP_CHARS: usize = 800;

/// Split `text` into overlapping char windows. UTF-8 safe (windows are cut on
/// char boundaries, never mid-codepoint). Short sources return a single chunk.
fn chunk_source(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let step = MAX_CHUNK_CHARS - CHUNK_OVERLAP_CHARS;
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + MAX_CHUNK_CHARS).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// Extract the outermost JSON object from provider output that may carry
/// prose or fences around it (string/escape aware).
pub fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// TTL for a kind in days: `BRAINIAC_TTL_DAYS_<KIND>` overrides the
/// core default; `0` disables expiry for that kind (valid_to stays NULL).
fn ttl_days(kind: MemoryKind) -> Option<i64> {
    let days = std::env::var(format!(
        "BRAINIAC_TTL_DAYS_{}",
        kind.as_str().to_uppercase()
    ))
    .ok()
    .and_then(|v| v.trim().parse::<i64>().ok())
    .unwrap_or(i64::from(kind.default_ttl_days()));
    (days > 0).then_some(days)
}

/// Parse an extractor response into the typed output, or return the failure
/// reason (used verbatim in the repair re-prompt).
fn parse_extraction(text: &str) -> std::result::Result<ExtractionOutput, String> {
    let json_str =
        extract_json_object(text).ok_or_else(|| "no JSON object in response".to_string())?;
    serde_json::from_str(json_str).map_err(|e| e.to_string())
}

/// One extraction call with a single bounded JSON-repair re-prompt. A
/// malformed first response is re-asked once — echoing the parse error and
/// the bad output and demanding corrected JSON only. A second failure returns
/// Err, which the worker maps onto the queue's attempt-aware fail path
/// (queue::fail → dead-letter after MAX_ATTEMPTS, never infinite retry).
struct LlmExtract {
    output: ExtractionOutput,
    model_ref: String,
    /// The first parse failed and a repair recovered it.
    repaired: bool,
}

async fn extract_once(provider: &dyn ChatProvider, user: &str) -> Result<LlmExtract> {
    let resp = provider
        .complete(&ChatRequest {
            system: EXTRACT_SYSTEM_PROMPT_V1.to_string(),
            user: user.to_string(),
            json_mode: true,
            max_tokens: MAX_EXTRACT_TOKENS,
        })
        .await
        .context("extract LLM call")?;

    match parse_extraction(&resp.text) {
        Ok(output) => Ok(LlmExtract {
            output,
            model_ref: resp.model_ref,
            repaired: false,
        }),
        Err(err) => {
            // Bounded to exactly one repair attempt.
            let repair_user = format!(
                "Your previous response could not be parsed as JSON ({err}).\n\
                 Here is the exact response that failed:\n\n{}\n\n\
                 Return ONLY the corrected JSON object matching the required \
                 schema — no prose, no code fences, nothing else.",
                resp.text
            );
            let repaired = provider
                .complete(&ChatRequest {
                    system: EXTRACT_SYSTEM_PROMPT_V1.to_string(),
                    user: repair_user,
                    json_mode: true,
                    max_tokens: MAX_EXTRACT_TOKENS,
                })
                .await
                .context("extract repair LLM call")?;
            let output = parse_extraction(&repaired.text).map_err(|e| {
                anyhow::anyhow!("extractor output unparseable after one repair: {e}")
            })?;
            Ok(LlmExtract {
                output,
                model_ref: repaired.model_ref,
                repaired: true,
            })
        }
    }
}

/// Idempotency guard: has a memory with this exact content already been
/// written for this source? Scoped to (org, source) via the provenance link
/// (memories carry no source_id column; provenance.source_id is the join).
/// Exact-content equality is the cheapest correct dedup here — no schema
/// change, no hash column — and it collapses both a redelivered job's retry
/// and an overlap region re-extracted by an adjacent chunk. Relies on the
/// worker read scope (org+team) to see the just-written rows under RLS.
async fn memory_exists_for_source(
    conn: &mut PgConnection,
    org_id: Uuid,
    source_id: Uuid,
    content: &str,
) -> Result<bool> {
    let existing = sqlx::query(
        "SELECT 1 FROM memories m
         JOIN provenance p ON p.id = m.provenance_id
         WHERE m.org_id = $1 AND p.source_id = $2 AND m.content = $3
         LIMIT 1",
    )
    .bind(org_id)
    .bind(source_id)
    .bind(content)
    .fetch_optional(conn)
    .await?;
    Ok(existing.is_some())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_extract(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    team_id: Option<Uuid>,
    source_id: Uuid,
    raw_text: &str,
    // Direction 2: the run this extraction belongs to. Stamped onto the single
    // provenance row so every memory (and entity) written here links back to
    // the pipeline_runs record via provenance.pipeline_run_id.
    pipeline_run_id: Option<Uuid>,
) -> Result<ExtractStats> {
    // Chunk oversized sources so a long session's tail isn't truncated. One
    // provenance row per source still (stamped from the first chunk's model
    // ref), every chunk's memories linked to it; dedup collapses overlaps.
    let chunks = chunk_source(raw_text);
    let mut stats = ExtractStats {
        chunks: chunks.len(),
        ..Default::default()
    };
    let mut provenance_id: Option<Uuid> = None;

    for chunk in &chunks {
        // One BYOM call per chunk, each with a bounded JSON-repair re-prompt.
        let call = extract_once(provider, chunk).await?;
        if call.repaired {
            stats.parse_failures += 1;
            stats.repairs += 1;
        }

        // Lazily create the single provenance row on the first chunk, using
        // the model ref the provider actually reported, and stamp the run id so
        // memories link back to their pipeline_runs record.
        if provenance_id.is_none() {
            stats.model_ref = Some(call.model_ref.clone());
            let pid = Uuid::new_v4();
            brainiac_store::governance::insert_provenance(
                conn,
                pid,
                org_id,
                ActorKind::Pipeline,
                "extract-worker",
                Some(&call.model_ref),
                Some(source_id),
                pipeline_run_id,
            )
            .await?;
            provenance_id = Some(pid);
        }
        let provenance_id = provenance_id.expect("provenance set above");

        for m in call.output.memories {
            // Validation firewall: invalid kinds/empty content are dropped and
            // counted, never written.
            let Some(kind) = MemoryKind::parse(&m.kind) else {
                stats.dropped_invalid += 1;
                continue;
            };
            let content = m.content.trim().to_string();
            if content.is_empty() {
                stats.dropped_invalid += 1;
                continue;
            }
            // Idempotency: skip a memory this source already produced (a
            // redelivered retry, or an overlap region seen by an adjacent
            // chunk).
            if memory_exists_for_source(conn, org_id, source_id, &content).await? {
                stats.deduped += 1;
                continue;
            }
            let visibility = m
                .visibility
                .as_deref()
                .and_then(Visibility::parse)
                .unwrap_or(Visibility::Team);
            let confidence = m.confidence.map(|c| c.clamp(0.0, 1.0));

            let memory_id = Uuid::new_v4();
            // Freshness lifecycle: stamp the validity window at extraction so
            // knowledge expires from retrieval (temporal::valid_at) instead of
            // staying presumed-true forever; /v1/memories/expiring surfaces rows
            // approaching the boundary for re-verification.
            let now = chrono::Utc::now();
            brainiac_store::memories::insert(
                conn,
                &brainiac_store::memories::NewMemory {
                    id: memory_id,
                    org_id,
                    team_id,
                    owner_user_id: None,
                    visibility,
                    status: MemoryStatus::Raw,
                    kind,
                    content: content.clone(),
                    language: "en".into(),
                    valid_from: Some(now),
                    valid_to: ttl_days(kind).map(|d| now + chrono::Duration::days(d)),
                    superseded_by: None,
                    confidence,
                    provenance_id: Some(provenance_id),
                },
            )
            .await?;

            // Entities: get-or-create within the source's team scope.
            let mut name_to_id: std::collections::HashMap<String, Uuid> =
                std::collections::HashMap::new();
            for e in &m.entities {
                let id =
                    match brainiac_store::entities::find_by_name(conn, org_id, team_id, &e.name)
                        .await?
                    {
                        Some(id) => id,
                        None => {
                            let id = Uuid::new_v4();
                            // Entity-kind firewall: coerce an unknown/typo'd
                            // kind to the `concept` default rather than storing
                            // the raw typo (dropping a whole memory over one
                            // mislabeled entity would be too harsh — the kind is
                            // advisory metadata, and a canonical value keeps the
                            // graph queryable).
                            let entity_kind = EntityKind::parse(&e.kind).unwrap_or_default();
                            brainiac_store::entities::insert_entity(
                                conn,
                                id,
                                org_id,
                                team_id,
                                &e.name,
                                entity_kind.as_str(),
                                &e.clean_aliases(),
                                Some(provenance_id),
                            )
                            .await?;
                            stats.entities_created.push(id);
                            id
                        }
                    };
                name_to_id.insert(e.name.to_lowercase(), id);
                brainiac_store::memories::link_entity(conn, memory_id, id).await?;
            }
            for r in &m.relations {
                let (Some(src), Some(dst)) = (
                    name_to_id.get(&r.src.to_lowercase()),
                    name_to_id.get(&r.dst.to_lowercase()),
                ) else {
                    stats.dropped_invalid += 1;
                    continue; // relation names must reference listed entities
                };
                // Relation firewall: unlike entity kind, an unknown relation is
                // DROPPED (counted), not coerced — a bogus edge invents graph
                // structure (wrong depends_on/owns), which is worse than an
                // untyped node. Only the five canonical relations reach the DB.
                let Some(relation) = EdgeRelation::parse(&r.rel) else {
                    stats.dropped_invalid += 1;
                    continue;
                };
                brainiac_store::entities::insert_edge(
                    conn,
                    Uuid::new_v4(),
                    org_id,
                    *src,
                    *dst,
                    relation.as_str(),
                    Some(memory_id),
                )
                .await?;
            }

            // Embed stage (local model, no queue round-trip needed in v0).
            brainiac_store::memories::upsert_embedding(
                conn,
                memory_id,
                embedding_version,
                &embedder.embed(&content).await?,
            )
            .await?;

            stats.memory_ids.push(memory_id);
            stats.memories_written += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_extraction_tolerates_fences() {
        let raw = "Sure! ```json\n{\"memories\":[]}\n```";
        assert_eq!(extract_json_object(raw), Some("{\"memories\":[]}"));
    }

    #[test]
    fn braces_inside_strings_do_not_confuse() {
        let raw = "{\"memories\":[{\"kind\":\"fact\",\"content\":\"a { b } c\"}]}";
        assert_eq!(extract_json_object(raw), Some(raw));
    }

    #[test]
    fn parse_extraction_reports_failure_reason() {
        assert!(parse_extraction("not json at all").is_err());
        assert!(parse_extraction(r#"{"memories":[]}"#).is_ok());
    }

    #[test]
    fn short_source_is_a_single_chunk() {
        let chunks = chunk_source("a short transcript");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "a short transcript");
    }

    #[test]
    fn long_source_splits_into_overlapping_chunks() {
        // Oversized source splits into contiguous windows that cover the whole
        // text (no dropped region), stepping by MAX_CHUNK_CHARS - overlap.
        let text: String = std::iter::repeat_n('x', MAX_CHUNK_CHARS + 1000).collect();
        let chunks = chunk_source(&text);
        assert!(
            chunks.len() >= 2,
            "oversized source splits: {}",
            chunks.len()
        );
        assert_eq!(chunks[0].chars().count(), MAX_CHUNK_CHARS);
        // Last window ends exactly at the true end of the source.
        let step = MAX_CHUNK_CHARS - CHUNK_OVERLAP_CHARS;
        assert_eq!(
            (chunks.len() - 1) * step + chunks.last().expect("chunk").chars().count(),
            text.chars().count()
        );
    }

    #[test]
    fn chunking_is_utf8_boundary_safe() {
        // Multibyte chars around the cut must not panic or corrupt: windows
        // are cut on char boundaries, so every chunk is valid UTF-8.
        let text: String = "é😀🚀".chars().cycle().take(MAX_CHUNK_CHARS + 50).collect();
        let chunks = chunk_source(&text);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].chars().count(), MAX_CHUNK_CHARS);
    }

    #[test]
    fn aliases_parse_and_clean() {
        // Parser captures the aliases array; cleaning trims, drops blanks and
        // any form equal to the name, and de-duplicates case-insensitively.
        let e: ExtractedEntity = serde_json::from_str(
            r#"{"name":"psp-gateway","kind":"service",
                "aliases":["PSP"," psp-gateway ","psp","PSP",""]}"#,
        )
        .expect("parse");
        assert_eq!(e.clean_aliases(), vec!["PSP".to_string()]);
    }

    #[test]
    fn entity_without_aliases_defaults_empty() {
        let e: ExtractedEntity =
            serde_json::from_str(r#"{"name":"kafka","kind":"tech"}"#).expect("parse");
        assert!(e.clean_aliases().is_empty());
    }
}
