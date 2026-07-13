//! Extract stage: source text → BYOM call → validated raw memories +
//! entities + relations, all provenance-stamped (stage 2 of §3).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{ActorKind, MemoryKind, MemoryStatus, Visibility};
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
) -> Result<ExtractStats> {
    let resp = provider
        .complete(&ChatRequest {
            system: EXTRACT_SYSTEM_PROMPT_V1.to_string(),
            user: raw_text.to_string(),
            json_mode: true,
            max_tokens: 4096,
        })
        .await
        .context("extract LLM call")?;

    let json_str = extract_json_object(&resp.text)
        .ok_or_else(|| anyhow::anyhow!("extractor returned no JSON object"))?;
    let output: ExtractionOutput =
        serde_json::from_str(json_str).context("parsing extractor output")?;

    let provenance_id = Uuid::new_v4();
    brainiac_store::governance::insert_provenance(
        conn,
        provenance_id,
        org_id,
        ActorKind::Pipeline,
        "extract-worker",
        Some(&resp.model_ref),
        Some(source_id),
        None,
    )
    .await?;

    let mut stats = ExtractStats::default();
    for m in output.memories {
        // Validation firewall: invalid kinds/empty content are dropped and
        // counted, never written.
        let Some(kind) = MemoryKind::parse(&m.kind) else {
            stats.dropped_invalid += 1;
            continue;
        };
        if m.content.trim().is_empty() {
            stats.dropped_invalid += 1;
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
                content: m.content.trim().to_string(),
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
            let id = match brainiac_store::entities::find_by_name(conn, org_id, team_id, &e.name)
                .await?
            {
                Some(id) => id,
                None => {
                    let id = Uuid::new_v4();
                    brainiac_store::entities::insert_entity(
                        conn,
                        id,
                        org_id,
                        team_id,
                        &e.name,
                        &e.kind,
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
            brainiac_store::entities::insert_edge(
                conn,
                Uuid::new_v4(),
                org_id,
                *src,
                *dst,
                &r.rel,
                Some(memory_id),
            )
            .await?;
        }

        // Embed stage (local model, no queue round-trip needed in v0).
        brainiac_store::memories::upsert_embedding(
            conn,
            memory_id,
            embedding_version,
            &embedder.embed(m.content.trim()).await?,
        )
        .await?;

        stats.memory_ids.push(memory_id);
        stats.memories_written += 1;
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
