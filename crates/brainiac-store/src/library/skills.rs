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

/// The version a maintainer would publish next: the newest UNSIGNED version of
/// a skill, or `None` when every version already carries a name. Picking the
/// draft lives here rather than in a handler so the REST route, the console and
/// any later surface cannot drift on *which* version "publish" means.
pub async fn pending_draft_version(
    conn: &mut PgConnection,
    skill_id: Uuid,
) -> Result<Option<SkillVersion>> {
    let row = sqlx::query(
        "SELECT id, skill_id, semver, manifest, content_md, resources,
                published_by, published_at, created_at
         FROM skill_versions
         WHERE skill_id = $1 AND published_by IS NULL
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(skill_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_skill_version))
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

/// The size bound on a whole skill BUNDLE — body + manifest + every resource
/// file — not on a single markdown body.
///
/// It lives here, in the store, for two reasons. The bound is a property of
/// what the database will hold inline, so the layer that writes the row is the
/// layer that should refuse an oversized one; and it was previously duplicated
/// as a per-body cap on each of the two surfaces (REST and MCP), which is the
/// kind of duplication that silently drifts. Both surfaces now consume this
/// one number by way of [`SkillProposeOutcome::TooLarge`].
///
/// 200k characters is ~10x the old single-body cap: a real multi-file skill —
/// a 25k-character procedure plus its references and scripts — fits without
/// being condensed into a digest, while the server still never buffers,
/// stores, or serves an unbounded blob. MCP's 1 MB JSON-RPC frame ceiling
/// independently bounds ingress on that surface.
pub const MAX_SKILL_BUNDLE_CHARS: usize = 200_000;

/// The whole bundle's weight: the body, the front-matter, and every resource
/// file, measured on the serialized form so a caller cannot smuggle bulk
/// through a shape the cap does not know about.
pub fn bundle_chars(
    content_md: &str,
    manifest: &serde_json::Value,
    resources: &serde_json::Value,
) -> usize {
    content_md.chars().count()
        + manifest.to_string().chars().count()
        + resources.to_string().chars().count()
}

/// The next MINOR of a semver, used when an agent proposes an improvement to a
/// skill that is already published. Proposals are minor bumps by construction:
/// the proposer does not get to declare a breaking change or a release number,
/// only to offer the next iteration. An unparseable existing version falls back
/// to a `.1`-suffixed successor rather than failing the proposal.
fn next_minor(current: &str) -> String {
    let parts: Vec<&str> = current.split('.').collect();
    if parts.len() == 3 {
        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
            return format!("{major}.{}.0", minor + 1);
        }
    }
    format!("{current}.1")
}

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
    /// The rest of the bundle: `[{path, content}]`. An empty array is a
    /// body-only skill, which is what most of them are.
    pub resources: serde_json::Value,
}

pub enum SkillProposeOutcome {
    /// A fresh draft skill is waiting for a maintainer to publish it.
    Created {
        skill_id: Uuid,
        version_id: Uuid,
        slug: String,
        semver: String,
    },
    /// The skill exists and is already published, so this proposal became the
    /// NEXT DRAFT VERSION of it rather than being turned away. The published
    /// version keeps serving until a maintainer signs this one — improving a
    /// skill and replacing it are different acts.
    NewVersion {
        skill_id: Uuid,
        version_id: Uuid,
        slug: String,
        semver: String,
        /// The version still being served while this draft waits.
        serving_semver: Option<String>,
    },
    /// A skill with this slug already exists and there is nothing to add — it
    /// already has a draft awaiting a signature (someone is on this), or it was
    /// deprecated on purpose. Collapsed, no new row.
    Duplicate {
        skill_id: Uuid,
        slug: String,
        maturity: SkillMaturity,
    },
    /// The author spent this hour's budget.
    RateLimited { per_hour: i64 },
    /// The bundle is over [`MAX_SKILL_BUNDLE_CHARS`]. Carries the numbers so a
    /// surface can render the refusal without knowing the constant.
    TooLarge { chars: usize, max: usize },
}

/// Propose a skill (F-4). Rate-limited per author; the slug still decides
/// whether this is a new skill, the next version of an existing one, or a
/// no-op.
///
/// The dedup rule is deliberately three-way rather than the original all-or-
/// nothing collapse:
///
/// - no skill with this slug   → a new DRAFT skill + its first version;
/// - published, nothing pending → the NEXT DRAFT VERSION (a skill is supposed
///   to improve; the published version keeps serving until someone signs the
///   new one);
/// - a draft already pending, or deprecated → collapsed, as before. One
///   unsigned draft at a time keeps "publish this skill" unambiguous, and a
///   skill retired on purpose is not revived by a proposal.
///
/// Publishing stays a separate named-human act in every branch.
pub async fn propose_skill(
    conn: &mut PgConnection,
    p: &SkillProposal,
    per_hour: i64,
) -> Result<SkillProposeOutcome> {
    // 0. The bundle bound, before anything is counted or written: an oversized
    //    proposal is refused, not charged against the author's hour.
    let chars = bundle_chars(&p.content_md, &p.manifest, &p.resources);
    if chars > MAX_SKILL_BUNDLE_CHARS {
        return Ok(SkillProposeOutcome::TooLarge {
            chars,
            max: MAX_SKILL_BUNDLE_CHARS,
        });
    }

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

    // 2. An existing skill with this slug: improve it, or say it exists.
    let slug = slugify(&p.name);
    if let Some(row) =
        sqlx::query("SELECT id, maturity FROM skills WHERE org_id = $1 AND slug = $2")
            .bind(p.org_id)
            .bind(&slug)
            .fetch_optional(&mut *conn)
            .await?
    {
        let skill_id: Uuid = row.get("id");
        let maturity =
            SkillMaturity::parse(row.get::<String, _>("maturity").as_str()).unwrap_or_default();

        let pending = pending_draft_version(&mut *conn, skill_id).await?;
        let serving = current_published_version(&mut *conn, skill_id).await?;

        // Only a published skill with nothing already awaiting a signature
        // takes a new version. Everything else collapses, as before.
        if pending.is_some() || serving.is_none() || maturity == SkillMaturity::Deprecated {
            return Ok(SkillProposeOutcome::Duplicate {
                skill_id,
                slug,
                maturity,
            });
        }

        // The successor number is derived from the newest version that exists
        // — not from the serving one — so a semver is never reused and the
        // (skill_id, semver) unique constraint cannot be tripped.
        let newest: Option<String> = sqlx::query_scalar(
            "SELECT semver FROM skill_versions WHERE skill_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(skill_id)
        .fetch_optional(&mut *conn)
        .await?;
        let semver = next_minor(newest.as_deref().unwrap_or(PROPOSED_SKILL_SEMVER));

        let version_id = Uuid::new_v4();
        add_skill_version(
            conn,
            &NewSkillVersion {
                id: version_id,
                skill_id,
                org_id: p.org_id,
                semver: semver.clone(),
                manifest: p.manifest.clone(),
                content_md: p.content_md.clone(),
                resources: p.resources.clone(),
            },
        )
        .await?;
        return Ok(SkillProposeOutcome::NewVersion {
            skill_id,
            version_id,
            slug,
            semver,
            serving_semver: serving.map(|v| v.semver),
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
            resources: p.resources.clone(),
        },
    )
    .await?;
    Ok(SkillProposeOutcome::Created {
        skill_id,
        version_id,
        slug,
        semver: PROPOSED_SKILL_SEMVER.to_string(),
    })
}
