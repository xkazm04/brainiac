//! The L6 bridge: the drift detector (migration 0016) already names practices
//! and recommends standards; [`ratify_divergence`] turns one ratified
//! recommendation into a `proposed` standard carrying the divergence as
//! provenance, idempotently. This is where the shipped ancestor meets the
//! Library.

use anyhow::Result;
use brainiac_core::{Enforcement, StandardOrigin, StandardProvenanceKind};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::standards::{insert_standard, NewStandard};

/// Derive a URL-safe slug from a practice name ("Service retry policy" →
/// "service-retry-policy"). Pure and deterministic so the idempotency story
/// stays simple.
pub fn slugify(practice: &str) -> String {
    let mut out = String::with_capacity(practice.len());
    let mut dash = true; // suppress a leading dash
    for c in practice.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "practice".into()
    } else {
        out
    }
}

/// Turn a detected practice divergence into a `proposed` standard candidate
/// carrying the divergence as provenance — the human door (a maintainer
/// ratifies from the drift board). The mining sweep uses
/// [`propose_from_divergence`] directly with no author.
pub async fn ratify_divergence(
    conn: &mut PgConnection,
    divergence_id: Uuid,
    ratified_by: Uuid,
) -> Result<Option<Uuid>> {
    propose_from_divergence(conn, divergence_id, Some(ratified_by)).await
}

/// Turn a detected practice divergence into a `proposed` standard candidate
/// carrying the divergence as provenance.
///
/// Idempotent by provenance, not by slug: bridging the same divergence twice
/// returns the existing candidate's id (exactly one candidate per divergence —
/// the LB0 gate). A slug collision with an UNRELATED standard gets a short
/// suffix from the divergence id rather than an error, because two practices
/// are allowed to share a name with different evidence.
///
/// `author` is the ratifying human, or `None` when the mining sweep proposes —
/// either way the result is only ever a CANDIDATE; the gate stays human.
///
/// Returns `None` when the divergence does not exist (or RLS hides it —
/// deliberately the same answer).
pub async fn propose_from_divergence(
    conn: &mut PgConnection,
    divergence_id: Uuid,
    author: Option<Uuid>,
) -> Result<Option<Uuid>> {
    // Already bridged? Return the existing candidate — never a second one.
    // A REJECTED claim does not count: the rejection is history, not a draft
    // to reopen, so bridging again mints a fresh, dated candidate. (The
    // mining sweep additionally honors the dedup window BEFORE calling this;
    // a human ratifying deliberately is allowed to override a rejection.)
    if let Some(row) = sqlx::query(
        "SELECT sp.standard_id FROM standard_provenance sp
         JOIN standards s ON s.id = sp.standard_id
         WHERE sp.kind = 'divergence' AND sp.ref_id = $1
           AND s.lifecycle <> 'rejected'
         LIMIT 1",
    )
    .bind(divergence_id)
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(Some(row.get("standard_id")));
    }

    let Some(d) = sqlx::query(
        "SELECT org_id, practice, summary, recommended_standard
         FROM practice_divergences WHERE id = $1",
    )
    .bind(divergence_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    let org_id: Uuid = d.get("org_id");
    let practice: String = d.get("practice");
    let summary: Option<String> = d.get("summary");
    let recommended: Option<String> = d.get("recommended_standard");

    let base_slug = slugify(&practice);
    let taken = sqlx::query("SELECT 1 AS one FROM standards WHERE org_id = $1 AND slug = $2")
        .bind(org_id)
        .bind(&base_slug)
        .fetch_optional(&mut *conn)
        .await?
        .is_some();
    let slug = if taken {
        format!("{base_slug}-{}", &divergence_id.simple().to_string()[..8])
    } else {
        base_slug
    };

    let standard = NewStandard {
        id: Uuid::new_v4(),
        org_id,
        // A ratifying human is a human path; an authorless bridge is the sweep.
        origin: if author.is_some() {
            StandardOrigin::Human
        } else {
            StandardOrigin::Sweep
        },
        // The detector clusters practices, not languages; a human narrows the
        // stack during triage if the rule is stack-specific.
        stack: "general".into(),
        category: "practice".into(),
        slug,
        // The adjudicator's recommendation is the candidate statement; a
        // divergence filed without one still becomes a candidate — naming the
        // practice is the statement until a human writes a better one.
        statement: recommended.unwrap_or_else(|| format!("Standardize: {practice}")),
        rationale: summary,
        detail_md: None,
        enforcement: Enforcement::Recommended,
        provenance: vec![(StandardProvenanceKind::Divergence, divergence_id)],
        author,
    };
    insert_standard(conn, &standard).await?;
    Ok(Some(standard.id))
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_is_url_safe_and_stable() {
        assert_eq!(slugify("Service retry policy"), "service-retry-policy");
        assert_eq!(slugify("  DB / migrations!! "), "db-migrations");
        assert_eq!(slugify("čekání na Postgres"), "ek-n-na-postgres");
        assert_eq!(slugify("***"), "practice");
        assert_eq!(slugify(""), "practice");
    }
}
