//! Skills: versioned bundles in the open agent-skill format (LIBRARY-PLAN L4).
//!
//! The one rule that matters here: DRAFTS ARE NEVER SERVED. Publishing is a
//! named-human act, and the serve path joins through `current_version` +
//! `published_by IS NOT NULL` — the same refusal the document layer makes for
//! unsigned pages.

use anyhow::Result;
use brainiac_core::{Skill, SkillMaturity, SkillVersion};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::bridge::slugify;

pub struct NewSkill {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    /// The agent identity that proposed this skill as a draft, or `None` when a
    /// maintainer created it directly at the console. Recorded for attribution
    /// and for the per-author proposal rate limit ([`propose_skill`]).
    pub proposed_by: Option<Uuid>,
}

pub async fn insert_skill(conn: &mut PgConnection, s: &NewSkill) -> Result<()> {
    sqlx::query(
        "INSERT INTO skills (id, org_id, slug, name, description, domain, proposed_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(s.id)
    .bind(s.org_id)
    .bind(&s.slug)
    .bind(&s.name)
    .bind(&s.description)
    .bind(&s.domain)
    .bind(s.proposed_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub struct NewSkillVersion {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub org_id: Uuid,
    pub semver: String,
    pub manifest: serde_json::Value,
    pub content_md: String,
    pub resources: serde_json::Value,
}

/// Add a DRAFT version of a bundle. Drafts are never served; publishing is a
/// separate named-human act ([`publish_skill_version`]).
pub async fn add_skill_version(conn: &mut PgConnection, v: &NewSkillVersion) -> Result<()> {
    sqlx::query(
        "INSERT INTO skill_versions
            (id, skill_id, org_id, semver, manifest, content_md, resources)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(v.id)
    .bind(v.skill_id)
    .bind(v.org_id)
    .bind(&v.semver)
    .bind(&v.manifest)
    .bind(&v.content_md)
    .bind(&v.resources)
    .execute(conn)
    .await?;
    Ok(())
}

/// Publish a draft version: stamps the named human on the version, points the
/// skill's `current_version` at it, and lifts the skill out of `draft`. One
/// statement per table, same transaction — a skill can never point at a
/// version that isn't published.
pub async fn publish_skill_version(
    conn: &mut PgConnection,
    version_id: Uuid,
    published_by: Uuid,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE skill_versions SET published_by = $2, published_at = now()
         WHERE id = $1 AND published_by IS NULL",
    )
    .bind(version_id)
    .bind(published_by)
    .execute(&mut *conn)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE skills SET current_version = $1, maturity = 'published', updated_at = now()
         WHERE id = (SELECT skill_id FROM skill_versions WHERE id = $1)",
    )
    .bind(version_id)
    .execute(conn)
    .await?;
    Ok(true)
}

fn row_to_skill(r: &sqlx::postgres::PgRow) -> Skill {
    Skill {
        id: r.get("id"),
        org_id: r.get("org_id"),
        slug: r.get("slug"),
        name: r.get("name"),
        description: r.get("description"),
        domain: r.get("domain"),
        maturity: SkillMaturity::parse(r.get::<String, _>("maturity").as_str()).unwrap_or_default(),
        current_version: r.get("current_version"),
        updated_at: r.get("updated_at"),
    }
}

const SKILL_COLUMNS: &str =
    "id, org_id, slug, name, description, domain, maturity, current_version, updated_at";

pub async fn get_skill_by_slug(conn: &mut PgConnection, slug: &str) -> Result<Option<Skill>> {
    let row = sqlx::query(&format!(
        "SELECT {SKILL_COLUMNS} FROM skills WHERE slug = $1"
    ))
    .bind(slug)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_skill))
}

pub async fn list_skills(conn: &mut PgConnection) -> Result<Vec<Skill>> {
    let rows = sqlx::query(&format!("SELECT {SKILL_COLUMNS} FROM skills ORDER BY slug"))
        .fetch_all(conn)
        .await?;
    Ok(rows.iter().map(row_to_skill).collect())
}

fn row_to_skill_version(r: &sqlx::postgres::PgRow) -> SkillVersion {
    SkillVersion {
        id: r.get("id"),
        skill_id: r.get("skill_id"),
        semver: r.get("semver"),
        manifest: r.get("manifest"),
        content_md: r.get("content_md"),
        resources: r.get("resources"),
        published_by: r.get("published_by"),
        published_at: r.get("published_at"),
        created_at: r.get("created_at"),
    }
}

/// Every version of a skill, newest first — the console's history view. The
/// SERVE path stays [`current_published_version`]; this is for maintainers who
/// need to see drafts awaiting a signature.
pub async fn versions_of(conn: &mut PgConnection, skill_id: Uuid) -> Result<Vec<SkillVersion>> {
    let rows = sqlx::query(
        "SELECT id, skill_id, semver, manifest, content_md, resources,
                published_by, published_at, created_at
         FROM skill_versions WHERE skill_id = $1 ORDER BY created_at DESC",
    )
    .bind(skill_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_skill_version).collect())
}

/// The serve path: a skill's current PUBLISHED version. A draft nobody signed
/// returns nothing — same refusal the document layer makes for unsigned pages.
pub async fn current_published_version(
    conn: &mut PgConnection,
    skill_id: Uuid,
) -> Result<Option<SkillVersion>> {
    let row = sqlx::query(
        "SELECT v.id, v.skill_id, v.semver, v.manifest, v.content_md, v.resources,
                v.published_by, v.published_at, v.created_at
         FROM skill_versions v
         JOIN skills s ON s.current_version = v.id
         WHERE s.id = $1 AND v.published_by IS NOT NULL",
    )
    .bind(skill_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_skill_version))
}

// ── agent proposals (F-4) ────────────────────────────────────────────────
//
// An agent that codified a runbook mid-session proposes it here. It lands as a
// DRAFT skill + a DRAFT (unpublished) version — never served, never policy —
// exactly like a `standard_propose` candidate waiting at the gate. The same two
// guards as the standard channel stand in front of it: a per-author hourly rate
// limit (counted from `skills.proposed_by`, no separate counter to drift) and a
// slug dedup that collapses a re-proposal onto the existing skill.

/// The initial semver a proposed draft carries. A maintainer bumps it when they
/// publish; until then the number is a placeholder, not a promise.
const PROPOSED_SKILL_SEMVER: &str = "0.1.0";

pub struct SkillProposal {
    pub org_id: Uuid,
    /// The proposing identity (the token's user). Rate-limited and recorded on
    /// the skill as `proposed_by`.
    pub author: Uuid,
    /// Human name; the slug (the dedup key) is derived from it.
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    /// The skill body — the runbook/checklist markdown the agent authored.
    pub content_md: String,
    /// Bundle front-matter (the surface builds it from name/description/domain).
    pub manifest: serde_json::Value,
}

pub enum SkillProposeOutcome {
    /// A fresh draft skill is waiting for a maintainer to publish it.
    Created {
        skill_id: Uuid,
        version_id: Uuid,
        slug: String,
    },
    /// A skill with this slug already exists — collapsed, no new row. The
    /// maturity tells the agent whether it is already published (use it),
    /// still a draft (someone is already on this), or deprecated.
    Duplicate {
        skill_id: Uuid,
        slug: String,
        maturity: SkillMaturity,
    },
    /// The author spent this hour's budget.
    RateLimited { per_hour: i64 },
}

/// Propose a skill as a draft (F-4). Rate-limited per author, deduplicated on
/// slug; otherwise inserts a draft skill and its first unpublished version in
/// one transaction. Publishing stays a separate named-human act.
pub async fn propose_skill(
    conn: &mut PgConnection,
    p: &SkillProposal,
    per_hour: i64,
) -> Result<SkillProposeOutcome> {
    // 1. The hour budget, counted from the skills the author has proposed.
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM skills
         WHERE org_id = $1 AND proposed_by = $2
           AND created_at > now() - interval '1 hour'",
    )
    .bind(p.org_id)
    .bind(p.author)
    .fetch_one(&mut *conn)
    .await?;
    if recent >= per_hour {
        return Ok(SkillProposeOutcome::RateLimited { per_hour });
    }

    // 2. Collapse onto an existing skill with the same slug, whatever its
    //    maturity — a second agent proposing the same runbook is told it exists
    //    rather than minting a duplicate the maintainer must then reconcile.
    let slug = slugify(&p.name);
    if let Some(row) =
        sqlx::query("SELECT id, maturity FROM skills WHERE org_id = $1 AND slug = $2")
            .bind(p.org_id)
            .bind(&slug)
            .fetch_optional(&mut *conn)
            .await?
    {
        return Ok(SkillProposeOutcome::Duplicate {
            skill_id: row.get("id"),
            slug,
            maturity: SkillMaturity::parse(row.get::<String, _>("maturity").as_str())
                .unwrap_or_default(),
        });
    }

    // 3. Insert the draft skill + its first (unpublished) version together.
    let skill_id = Uuid::new_v4();
    insert_skill(
        conn,
        &NewSkill {
            id: skill_id,
            org_id: p.org_id,
            slug: slug.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            domain: p.domain.clone(),
            proposed_by: Some(p.author),
        },
    )
    .await?;
    let version_id = Uuid::new_v4();
    add_skill_version(
        conn,
        &NewSkillVersion {
            id: version_id,
            skill_id,
            org_id: p.org_id,
            semver: PROPOSED_SKILL_SEMVER.to_string(),
            manifest: p.manifest.clone(),
            content_md: p.content_md.clone(),
            resources: serde_json::json!([]),
        },
    )
    .await?;
    Ok(SkillProposeOutcome::Created {
        skill_id,
        version_id,
        slug,
    })
}
