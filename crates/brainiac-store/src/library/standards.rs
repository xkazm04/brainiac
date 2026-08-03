//! Standards: the rule is the atom (LIBRARY-PLAN L1).
//!
//! Insert lands a `proposed` candidate (row + version rev 1 + provenance,
//! atomically). Adoption is the gate: evidence or a named decree, enforced by
//! the schema's attribution trigger, never by caller discipline.

use anyhow::Result;
use brainiac_core::{
    Enforcement, Standard, StandardLifecycle, StandardOrigin, StandardProvenance,
    StandardProvenanceKind,
};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub struct NewStandard {
    pub id: Uuid,
    pub org_id: Uuid,
    /// Who is asking: a human path, the mining sweep, or an agent proposal.
    pub origin: StandardOrigin,
    pub stack: String,
    pub category: String,
    pub slug: String,
    pub statement: String,
    pub rationale: Option<String>,
    pub detail_md: Option<String>,
    pub enforcement: Enforcement,
    /// Provenance rows written atomically with the standard. May be empty for
    /// a `proposed` candidate; must be non-empty (or the rule later decreed)
    /// before adoption — the schema enforces that, not this struct.
    pub provenance: Vec<(StandardProvenanceKind, Uuid)>,
    /// Recorded as the first version's author (a human, or the bridge's ratifier).
    pub author: Option<Uuid>,
}

/// Insert a standard as a `proposed` candidate: the row, version rev 1, and
/// its provenance, in the caller's transaction.
pub async fn insert_standard(conn: &mut PgConnection, s: &NewStandard) -> Result<()> {
    sqlx::query(
        "INSERT INTO standards
            (id, org_id, origin, stack, category, slug, statement, rationale, detail_md, enforcement)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(s.id)
    .bind(s.org_id)
    .bind(s.origin.as_str())
    .bind(&s.stack)
    .bind(&s.category)
    .bind(&s.slug)
    .bind(&s.statement)
    .bind(&s.rationale)
    .bind(&s.detail_md)
    .bind(s.enforcement.as_str())
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO standard_versions
            (standard_id, org_id, rev, statement, rationale, detail_md, enforcement, author)
         VALUES ($1,$2,1,$3,$4,$5,$6,$7)",
    )
    .bind(s.id)
    .bind(s.org_id)
    .bind(&s.statement)
    .bind(&s.rationale)
    .bind(&s.detail_md)
    .bind(s.enforcement.as_str())
    .bind(s.author)
    .execute(&mut *conn)
    .await?;
    for (kind, ref_id) in &s.provenance {
        add_provenance(conn, s.id, s.org_id, *kind, *ref_id).await?;
    }
    Ok(())
}

pub async fn add_provenance(
    conn: &mut PgConnection,
    standard_id: Uuid,
    org_id: Uuid,
    kind: StandardProvenanceKind,
    ref_id: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO standard_provenance (standard_id, org_id, kind, ref_id)
         VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(standard_id)
    .bind(org_id)
    .bind(kind.as_str())
    .bind(ref_id)
    .execute(conn)
    .await?;
    Ok(())
}

fn row_to_standard(r: &sqlx::postgres::PgRow) -> Standard {
    Standard {
        id: r.get("id"),
        org_id: r.get("org_id"),
        origin: StandardOrigin::parse(r.get::<String, _>("origin").as_str()).unwrap_or_default(),
        stack: r.get("stack"),
        category: r.get("category"),
        slug: r.get("slug"),
        statement: r.get("statement"),
        rationale: r.get("rationale"),
        detail_md: r.get("detail_md"),
        enforcement: Enforcement::parse(r.get::<String, _>("enforcement").as_str())
            .unwrap_or_default(),
        lifecycle: StandardLifecycle::parse(r.get::<String, _>("lifecycle").as_str())
            .unwrap_or_default(),
        adopted_by: r.get("adopted_by"),
        adopted_at: r.get("adopted_at"),
        decreed_by: r.get("decreed_by"),
        updated_at: r.get("updated_at"),
    }
}

const STANDARD_COLUMNS: &str = "id, org_id, origin, stack, category, slug, statement, rationale,
     detail_md, enforcement, lifecycle, adopted_by, adopted_at, decreed_by, updated_at";

pub async fn get_standard(conn: &mut PgConnection, id: Uuid) -> Result<Option<Standard>> {
    let row = sqlx::query(&format!(
        "SELECT {STANDARD_COLUMNS} FROM standards WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_standard))
}

pub async fn get_standard_by_slug(conn: &mut PgConnection, slug: &str) -> Result<Option<Standard>> {
    let row = sqlx::query(&format!(
        "SELECT {STANDARD_COLUMNS} FROM standards WHERE slug = $1"
    ))
    .bind(slug)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_standard))
}

/// List standards, optionally narrowed to a stack and/or lifecycle. The
/// serve path (LB1) calls this with `lifecycle = Some(Adopted)` — agents
/// are never handed a proposal as if it were the org's judgment.
pub async fn list_standards(
    conn: &mut PgConnection,
    stack: Option<&str>,
    lifecycle: Option<StandardLifecycle>,
) -> Result<Vec<Standard>> {
    let rows = sqlx::query(&format!(
        "SELECT {STANDARD_COLUMNS} FROM standards
         WHERE ($1::text IS NULL OR stack = $1)
           AND ($2::text IS NULL OR lifecycle = $2)
         ORDER BY stack, category, slug"
    ))
    .bind(stack)
    .bind(lifecycle.map(|l| l.as_str()))
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_standard).collect())
}

pub async fn provenance(
    conn: &mut PgConnection,
    standard_id: Uuid,
) -> Result<Vec<StandardProvenance>> {
    let rows = sqlx::query(
        "SELECT standard_id, kind, ref_id FROM standard_provenance
         WHERE standard_id = $1 ORDER BY kind, ref_id",
    )
    .bind(standard_id)
    .fetch_all(conn)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(StandardProvenance {
                standard_id: r.get("standard_id"),
                kind: StandardProvenanceKind::parse(r.get::<String, _>("kind").as_str())
                    .ok_or_else(|| anyhow::anyhow!("unknown provenance kind"))?,
                ref_id: r.get("ref_id"),
            })
        })
        .collect()
}

/// THE anti-rot call for the normative layer (L8), the Library's mirror of
/// `documents::mark_dirty_for_memory`. A rule was adopted, retired, or
/// re-worded → the standards page for its stack is now telling readers
/// something the org no longer holds. Marking it dirty is what makes the page
/// follow the gate without anyone remembering to edit it.
///
/// Returns how many pages were marked (0 when the stack has no page yet —
/// scaffolding will make one on the next sweep).
pub async fn mark_standards_pages_dirty(conn: &mut PgConnection, stack: &str) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE documents SET dirty_at = COALESCE(dirty_at, now())
         WHERE doc_kind = 'standards_page' AND slug = 'standards-' || $1",
    )
    .bind(stack)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// Mark the standards page for a rule's stack dirty, resolving the stack from
/// the rule itself. Called by every lifecycle transition below.
async fn mark_page_for_standard(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    let stack: Option<String> = sqlx::query_scalar("SELECT stack FROM standards WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(stack) = stack {
        mark_standards_pages_dirty(conn, &stack).await?;
    }
    Ok(())
}

/// Adopt a `proposed` standard — the gate itself. `decree` marks an
/// evidence-free rule the human signs for by name; without it, the schema's
/// attribution trigger refuses a rule that has no provenance. The trigger is
/// deferred by default; we force it IMMEDIATE here so an illegal adoption
/// fails at this statement (a typed store error) instead of at commit.
///
/// Returns `false` if the standard is missing or not `proposed` — adoption is
/// a one-way door and re-adopting is a caller bug worth surfacing.
pub async fn adopt_standard(
    conn: &mut PgConnection,
    id: Uuid,
    adopted_by: Uuid,
    decree: bool,
) -> Result<bool> {
    sqlx::query("SET CONSTRAINTS standards_attribution IMMEDIATE")
        .execute(&mut *conn)
        .await?;
    let res = sqlx::query(
        "UPDATE standards
         SET lifecycle = 'adopted', adopted_by = $2, adopted_at = now(),
             decreed_by = CASE WHEN $3 THEN $2 ELSE decreed_by END,
             updated_at = now()
         WHERE id = $1 AND lifecycle = 'proposed'",
    )
    .bind(id)
    .bind(adopted_by)
    .bind(decree)
    .execute(&mut *conn)
    .await?;
    if res.rows_affected() > 0 {
        // The org's judgment changed → the page that publishes it is stale.
        mark_page_for_standard(conn, id).await?;
    }
    Ok(res.rows_affected() > 0)
}

/// One numbered revision of a rule, for the console's version history.
pub struct StandardVersionRow {
    pub rev: i32,
    pub statement: String,
    pub enforcement: String,
    pub author: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn versions(
    conn: &mut PgConnection,
    standard_id: Uuid,
) -> Result<Vec<StandardVersionRow>> {
    let rows = sqlx::query(
        "SELECT rev, statement, enforcement, author, created_at
         FROM standard_versions WHERE standard_id = $1 ORDER BY rev DESC",
    )
    .bind(standard_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| StandardVersionRow {
            rev: r.get("rev"),
            statement: r.get("statement"),
            enforcement: r.get("enforcement"),
            author: r.get("author"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// What triage may change about a rule. Every field is a patch: `None` means
/// "leave it alone", so a maintainer narrowing a mined candidate's stack never
/// has to re-send the statement it already carries.
#[derive(Default)]
pub struct StandardPatch {
    /// Re-file the rule under a real stack. Mining files everything under
    /// `general`; this is how a maintainer moves it to `rust` before adopting.
    pub stack: Option<String>,
    pub category: Option<String>,
    pub statement: Option<String>,
    pub rationale: Option<String>,
    pub detail_md: Option<String>,
    pub enforcement: Option<Enforcement>,
}

pub enum UpdateOutcome {
    /// The rule changed. `rev` is the NEW revision number when the wording
    /// changed (statement / rationale / examples / enforcement); `None` when
    /// only the filing moved — where a rule is shelved is not a revision of
    /// what the org committed to, and minting a version for it would make the
    /// history lie about how often the rule itself changed.
    Updated { rev: Option<i32> },
    /// No such rule under the caller's scope (another org's rule is "missing",
    /// never "forbidden").
    NotFound,
    /// Deprecated or rejected: terminal. The way back is a new proposal, not
    /// an edit to history.
    Terminal { lifecycle: StandardLifecycle },
}

/// Narrow, re-file, or re-word a rule — the triage edit the gate was missing.
///
/// Two properties this function exists to guarantee, and which its callers
/// must not have to remember:
///
///  1. **A re-word is a REVISION, never an in-place mutation.** Changing what
///     an adopted rule says appends `standard_versions` rev N+1 authored by
///     the maintainer, so "the org changed its mind on 4 August" is readable
///     off the rule forever.
///  2. **The page follows.** An adopted rule that moved or changed text makes
///     its standards page stale, so the page is dirty-marked exactly like any
///     other document whose source moved — including the page it LEFT when
///     the stack changed, which would otherwise keep publishing a rule that
///     no longer lives there.
///
/// A `proposed` candidate is on no page yet, so nothing is marked for it; the
/// adoption that follows does that (see [`adopt_standard`]).
pub async fn update_standard(
    conn: &mut PgConnection,
    id: Uuid,
    patch: &StandardPatch,
    author: Uuid,
) -> Result<UpdateOutcome> {
    let Some(before) = get_standard(&mut *conn, id).await? else {
        return Ok(UpdateOutcome::NotFound);
    };
    if matches!(
        before.lifecycle,
        StandardLifecycle::Deprecated | StandardLifecycle::Rejected
    ) {
        return Ok(UpdateOutcome::Terminal {
            lifecycle: before.lifecycle,
        });
    }

    let stack = patch.stack.clone().unwrap_or_else(|| before.stack.clone());
    let category = patch
        .category
        .clone()
        .unwrap_or_else(|| before.category.clone());
    let statement = patch
        .statement
        .clone()
        .unwrap_or_else(|| before.statement.clone());
    let rationale = patch.rationale.clone().or_else(|| before.rationale.clone());
    let detail_md = patch.detail_md.clone().or_else(|| before.detail_md.clone());
    let enforcement = patch.enforcement.unwrap_or(before.enforcement);

    let reworded = statement != before.statement
        || rationale != before.rationale
        || detail_md != before.detail_md
        || enforcement != before.enforcement;
    let refiled = stack != before.stack || category != before.category;
    if !reworded && !refiled {
        // Nothing to say: an idempotent no-op, not a fresh revision of
        // identical prose.
        return Ok(UpdateOutcome::Updated { rev: None });
    }

    sqlx::query(
        "UPDATE standards
         SET stack = $2, category = $3, statement = $4, rationale = $5,
             detail_md = $6, enforcement = $7, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&stack)
    .bind(&category)
    .bind(&statement)
    .bind(&rationale)
    .bind(&detail_md)
    .bind(enforcement.as_str())
    .execute(&mut *conn)
    .await?;

    let rev = if reworded {
        let next: i32 = sqlx::query_scalar(
            "SELECT COALESCE(max(rev), 0) + 1 FROM standard_versions WHERE standard_id = $1",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO standard_versions
                (standard_id, org_id, rev, statement, rationale, detail_md, enforcement, author)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(id)
        .bind(before.org_id)
        .bind(next)
        .bind(&statement)
        .bind(&rationale)
        .bind(&detail_md)
        .bind(enforcement.as_str())
        .bind(author)
        .execute(&mut *conn)
        .await?;
        Some(next)
    } else {
        None
    };

    if before.lifecycle == StandardLifecycle::Adopted {
        // The page that publishes this rule is now telling readers something
        // the org no longer holds — the same anti-rot call every other
        // lifecycle transition makes.
        mark_standards_pages_dirty(&mut *conn, &before.stack).await?;
        if refiled && stack != before.stack {
            // …and the page it moved TO must pick it up.
            mark_standards_pages_dirty(&mut *conn, &stack).await?;
        }
    }

    Ok(UpdateOutcome::Updated { rev })
}

/// Retire an `adopted` standard, in the open. The deprecator is recorded as
/// the last named human on the rule (`adopted_by`), because "who retired
/// this and when" is the question the next reader asks.
pub async fn deprecate_standard(
    conn: &mut PgConnection,
    id: Uuid,
    deprecated_by: Uuid,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE standards
         SET lifecycle = 'deprecated', adopted_by = $2, updated_at = now()
         WHERE id = $1 AND lifecycle = 'adopted'",
    )
    .bind(id)
    .bind(deprecated_by)
    .execute(&mut *conn)
    .await?;
    if res.rows_affected() > 0 {
        // A retired rule must leave the page — silently continuing to publish
        // it would be the wiki lying on the Library's behalf.
        mark_page_for_standard(conn, id).await?;
    }
    Ok(res.rows_affected() > 0)
}

/// Reject a `proposed` candidate — kept, not deleted: the mining sweep dedups
/// against rejections (rejection is knowledge, LB3), and "who said no" is
/// recorded the same way "who adopted" is.
pub async fn reject_standard(conn: &mut PgConnection, id: Uuid, rejected_by: Uuid) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE standards
         SET lifecycle = 'rejected', adopted_by = $2, updated_at = now()
         WHERE id = $1 AND lifecycle = 'proposed'",
    )
    .bind(id)
    .bind(rejected_by)
    .execute(conn)
    .await?;
    Ok(res.rows_affected() > 0)
}
