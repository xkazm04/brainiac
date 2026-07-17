//! Practice-divergence detection — the standardization sweep.
//!
//! `contradict` catches two facts that cannot both be true. This catches the
//! subtler, more valuable thing for a platform team: several teams solving the
//! SAME problem in DIFFERENT, each-locally-reasonable ways — a divergence that is
//! invisible to any one team and only exists in the aggregate. It gathers
//! cross-team clusters (memories anchored on a shared CANONICAL entity, so
//! "payments API" and "the payments backend" land in one cluster), then asks the
//! provider to adjudicate each into a named practice, each team's approach, and a
//! single recommended standard.
//!
//! Conservative by design: over-flagging "divergences" that are really just two
//! teams describing different aspects would waste exactly the platform-lead time
//! this is meant to save — so the prompt names the not-a-divergence outcome and
//! the sweep only stores `divergence: true` verdicts.

use anyhow::{Context, Result};
use brainiac_gateway::{ChatProvider, ChatRequest};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

pub const DIVERGENCE_SYSTEM_PROMPT_V1: &str = "\
Several teams in ONE organization have each recorded how they handle something about the SAME
system or concept. Decide whether they are solving the SAME practice in DIFFERENT, inconsistent
ways (a divergence worth standardizing) — or whether their statements are consistent, complementary,
or simply about different aspects.

Respond with ONLY a JSON object:
{\"divergence\":true|false,
 \"practice\":\"short name of the practice, e.g. 'service retry policy'\",
 \"summary\":\"one sentence: what actually differs between the teams\",
 \"approaches\":[{\"team\":\"team name\",\"approach\":\"one line: this team's way\"}],
 \"recommended_standard\":\"one sentence: the single standard the org should adopt\",
 \"impact\":\"high|medium|low\"}

Only return divergence:true when teams genuinely do the SAME thing in INCOMPATIBLE ways (conflicting
values, competing procedures). If they agree, or each covers a different aspect, return
divergence:false with empty fields. Be conservative — a false divergence wastes a platform team's time.";

/// The project-axis twin (PROJECT-PLAN PR3): the same adjudication with the
/// grouping unit swapped from teams to applications/domains. Kept as its own
/// prompt rather than a parameterized one because the not-a-divergence
/// framing differs: two APPLICATIONS legitimately differ more often than two
/// teams do (different constraints), so the conservatism bar is stated higher.
pub const DIVERGENCE_PROJECT_PROMPT_V1: &str = "\
Several applications (projects) in ONE organization each have recorded practice for the SAME
system or concept. Decide whether they solve the SAME practice in DIFFERENT, inconsistent ways
(a divergence worth standardizing across applications) — or whether the difference is a
legitimate consequence of each application's own constraints.

Respond with ONLY a JSON object:
{\"divergence\":true|false,
 \"practice\":\"short name of the practice, e.g. 'service retry policy'\",
 \"summary\":\"one sentence: what actually differs between the applications\",
 \"approaches\":[{\"project\":\"application name\",\"approach\":\"one line: this application's way\"}],
 \"recommended_standard\":\"one sentence: the single standard the org should adopt\",
 \"impact\":\"high|medium|low\"}

Only return divergence:true for genuinely INCOMPATIBLE ways of doing the same thing. Applications
differing because their domains demand it is NOT a divergence. Be conservative.";

const MAX_POSITIONS_PER_CLUSTER: usize = 8;
const MAX_DIVERGENCE_TOKENS: u32 = 1200;

#[derive(Debug, Deserialize)]
struct Verdict {
    #[serde(default)]
    divergence: bool,
    #[serde(default)]
    practice: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    approaches: Vec<Approach>,
    #[serde(default)]
    recommended_standard: String,
    #[serde(default)]
    impact: String,
}

/// One group's way. `team` is set on team-axis rows, `project` on
/// project-axis rows; the other side is omitted from the JSON entirely
/// (skip_serializing_if) so old readers of `positions` are undisturbed.
#[derive(Debug, Deserialize, serde::Serialize)]
struct Approach {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    team: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    #[serde(default)]
    approach: String,
}

struct Position {
    team: String,
    /// Project display name; `None` = org-shared (PR0 stamping).
    project: Option<String>,
    kind: String,
    content: String,
}

#[derive(Debug, Default)]
pub struct DivergenceStats {
    /// Cross-team clusters that were candidates for adjudication.
    pub clusters: usize,
    /// Clusters the adjudicator confirmed as genuine divergences (stored).
    pub divergences: usize,
}

/// Scan one org for practice divergences, replacing any prior scan's rows. Uses a
/// raw (RLS-bypassing) pool — this is an operator/scheduled sweep, like reembed,
/// not a per-request path. Returns what it found.
pub async fn scan_org(
    pool: &sqlx::PgPool,
    provider: &dyn ChatProvider,
    org_id: Uuid,
) -> Result<DivergenceStats> {
    // Gather every cross-team-anchorable memory, grouped by canonical entity.
    // The project name rides along (PR3) so the same clusters can be judged
    // on the project axis too.
    let rows = sqlx::query(
        "SELECT ce.id AS canonical_id, ce.name AS canonical_name,
                t.name AS team, pj.name AS project, m.kind::text AS kind, m.content
         FROM canonical_entities ce
         JOIN entity_links el ON el.canonical_id = ce.id
         JOIN entities e ON e.id = el.entity_id
         JOIN memory_entities me ON me.entity_id = e.id
         JOIN memories m ON m.id = me.memory_id
         JOIN teams t ON t.id = m.team_id
         LEFT JOIN projects pj ON pj.id = m.project_id
         WHERE ce.org_id = $1 AND m.status <> 'rejected'
           AND m.kind IN ('decision','howto','pattern','fact')
         ORDER BY ce.id",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await
    .context("gathering divergence clusters")?;

    // Group by canonical entity → its positions.
    let mut clusters: std::collections::HashMap<(Uuid, String), Vec<Position>> =
        std::collections::HashMap::new();
    for r in &rows {
        let key = (r.get::<Uuid, _>("canonical_id"), r.get("canonical_name"));
        clusters.entry(key).or_default().push(Position {
            team: r.get("team"),
            project: r.get("project"),
            kind: r.get("kind"),
            content: r.get("content"),
        });
    }

    let mut stats = DivergenceStats::default();
    let mut confirmed: Vec<(Uuid, String, Verdict, String, &'static str)> = Vec::new();

    // One adjudication per (cluster, axis). A cluster spanning both ≥2 teams
    // and ≥2 projects is judged TWICE, deliberately: the groupings partition
    // the same statements differently, and "payments vs platform disagree" is
    // a different finding — with a different audience — from "payments-api vs
    // checkout-web diverge" (PROJECT-PLAN PR3, open decision 1: cross-project
    // is an ADDED class, never a replacement).
    for ((canonical_id, canonical_name), positions) in &clusters {
        let teams: std::collections::HashSet<&str> =
            positions.iter().map(|p| p.team.as_str()).collect();
        // Only project-STAMPED rows count toward the project axis: org-shared
        // memories belong to every application and cannot diverge between two.
        let projects: std::collections::HashSet<&str> = positions
            .iter()
            .filter_map(|p| p.project.as_deref())
            .collect();

        let mut axes: Vec<&'static str> = Vec::new();
        if teams.len() >= 2 {
            axes.push("team");
        }
        if projects.len() >= 2 {
            axes.push("project");
        }

        for axis in axes {
            stats.clusters += 1;
            let (system, listed): (&str, String) = match axis {
                "project" => (
                    DIVERGENCE_PROJECT_PROMPT_V1,
                    positions
                        .iter()
                        .filter(|p| p.project.is_some())
                        .take(MAX_POSITIONS_PER_CLUSTER)
                        .map(|p| {
                            format!(
                                "- [{}] ({}) {}",
                                p.project.as_deref().unwrap_or_default(),
                                p.kind,
                                p.content
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => (
                    DIVERGENCE_SYSTEM_PROMPT_V1,
                    positions
                        .iter()
                        .take(MAX_POSITIONS_PER_CLUSTER)
                        .map(|p| format!("- [{}] ({}) {}", p.team, p.kind, p.content))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            };
            let noun = if axis == "project" { "application" } else { "team" };
            let user = format!(
                "System / concept: {canonical_name}\n\nWhat each {noun} has recorded about it:\n{listed}"
            );
            let resp = provider
                .complete(&ChatRequest {
                    system: system.to_string(),
                    user,
                    json_mode: true,
                    max_tokens: MAX_DIVERGENCE_TOKENS,
                    temperature: 0.0,
                })
                .await
                .context("divergence adjudication call")?;

            // Reuse the extractor's tolerant JSON recovery for real-provider output.
            let Some(json) = crate::extract::extract_json_object(&resp.text) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Verdict>(json) else {
                continue;
            };
            if v.divergence && !v.practice.trim().is_empty() {
                stats.divergences += 1;
                confirmed.push((*canonical_id, canonical_name.clone(), v, resp.model_ref, axis));
            }
        }
    }

    // Replace the org's prior scan atomically, then insert the fresh verdicts.
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM practice_divergences WHERE org_id = $1")
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
    for (canonical_id, _name, v, model_ref, axis) in &confirmed {
        let impact = match v.impact.trim().to_lowercase().as_str() {
            "high" => "high",
            "low" => "low",
            _ => "medium",
        };
        sqlx::query(
            "INSERT INTO practice_divergences
               (org_id, canonical_id, practice, summary, recommended_standard, impact, positions, model_ref, axis)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(org_id)
        .bind(canonical_id)
        .bind(v.practice.trim())
        .bind(v.summary.trim())
        .bind(v.recommended_standard.trim())
        .bind(impact)
        .bind(serde_json::to_value(&v.approaches).unwrap_or(serde_json::json!([])))
        .bind(model_ref)
        .bind(axis)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(stats)
}

/// Scan every org in the database — the operator sweep.
pub async fn scan_all(pool: &sqlx::PgPool, provider: &dyn ChatProvider) -> Result<DivergenceStats> {
    let orgs: Vec<Uuid> = sqlx::query("SELECT id FROM orgs")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| r.get("id"))
        .collect();
    let mut total = DivergenceStats::default();
    for org in orgs {
        let s = scan_org(pool, provider, org).await?;
        total.clusters += s.clusters;
        total.divergences += s.divergences;
    }
    Ok(total)
}
