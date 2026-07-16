//! Agent proposals (LIBRARY-PLAN LB4) — the noisy channel, tamed.
//!
//! An agent that found a better pattern mid-session proposes it here. Three
//! things stand between that and triage spam, in order:
//!
//!   1. RATE LIMIT per author: N proposals per hour (the surface passes the
//!      limit; env-configurable). Counted from the rows themselves — the
//!      first version's author — so there is no separate counter to drift.
//!   2. DEDUP against the whole corpus: a proposal whose slug or exact
//!      statement already exists COLLAPSES onto that standard, whatever its
//!      lifecycle. Ten agents finding the same thing make one candidate; an
//!      agent proposing something the org already rejected is told exactly
//!      that, instead of reopening the argument.
//!   3. THE GATE, unchanged: the outcome is only ever a `proposed` candidate.
//!      Evidence is optional (a memory the agent cites); without it the rule
//!      can only ever be adopted by decree — schema-enforced, as always.

use anyhow::Result;
use brainiac_core::{Enforcement, StandardLifecycle, StandardOrigin, StandardProvenanceKind};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use super::bridge::slugify;
use super::standards::{insert_standard, NewStandard};

/// Default per-author proposals per hour. Five: a session that genuinely
/// surfaces more distinct org-wide rules than that in an hour is not
/// proposing, it is flooding — and the sixth can wait sixty minutes.
pub const DEFAULT_PROPOSE_PER_HOUR: i64 = 5;

pub struct Proposal {
    pub org_id: Uuid,
    /// The proposing identity (the token's user). Rate-limited and recorded
    /// as the first version's author.
    pub author: Uuid,
    /// Short practice name — the dedup key (slugified) and the rule's slug.
    pub name: String,
    pub statement: String,
    pub stack: Option<String>,
    pub category: Option<String>,
    pub rationale: Option<String>,
    pub detail_md: Option<String>,
    /// A memory the agent cites as evidence. Optional — but a proposal
    /// without evidence can only ever be adopted by decree.
    pub evidence_memory_id: Option<Uuid>,
}

pub enum ProposeOutcome {
    /// A fresh candidate is waiting at the gate.
    Created(Uuid),
    /// The idea already exists — collapsed, no new row. The lifecycle tells
    /// the agent what the org already decided (an open candidate, an adopted
    /// rule it should simply follow, or a rejection it should respect).
    Duplicate {
        standard_id: Uuid,
        lifecycle: StandardLifecycle,
    },
    /// The author spent this hour's budget.
    RateLimited { per_hour: i64 },
    /// The cited evidence memory does not exist (or RLS hides it — the same
    /// answer, deliberately).
    EvidenceNotFound,
}

pub async fn propose_standard(
    conn: &mut PgConnection,
    p: &Proposal,
    per_hour: i64,
) -> Result<ProposeOutcome> {
    // 1. The hour budget, counted from the corpus itself.
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM standard_versions
         WHERE org_id = $1 AND rev = 1 AND author = $2
           AND created_at > now() - interval '1 hour'",
    )
    .bind(p.org_id)
    .bind(p.author)
    .fetch_one(&mut *conn)
    .await?;
    if recent >= per_hour {
        return Ok(ProposeOutcome::RateLimited { per_hour });
    }

    // 2. Collapse onto anything that already says this — by name or verbatim
    //    statement, whatever its lifecycle. NOT windowed like the sweep: an
    //    agent is answerable in-session, so telling it "rejected" is strictly
    //    better than quietly minting a second candidate.
    let slug = slugify(&p.name);
    if let Some(row) = sqlx::query(
        "SELECT id, lifecycle FROM standards
         WHERE org_id = $1 AND (slug = $2 OR statement = $3)
         ORDER BY (lifecycle <> 'rejected') DESC, updated_at DESC
         LIMIT 1",
    )
    .bind(p.org_id)
    .bind(&slug)
    .bind(&p.statement)
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(ProposeOutcome::Duplicate {
            standard_id: row.get("id"),
            lifecycle: StandardLifecycle::parse(row.get::<String, _>("lifecycle").as_str())
                .unwrap_or_default(),
        });
    }

    // 3. Cited evidence must exist under the caller's own visibility.
    let mut provenance = Vec::new();
    if let Some(memory_id) = p.evidence_memory_id {
        let seen = sqlx::query("SELECT 1 AS one FROM memories WHERE id = $1")
            .bind(memory_id)
            .fetch_optional(&mut *conn)
            .await?
            .is_some();
        if !seen {
            return Ok(ProposeOutcome::EvidenceNotFound);
        }
        provenance.push((StandardProvenanceKind::Memory, memory_id));
    }

    let id = Uuid::new_v4();
    insert_standard(
        conn,
        &NewStandard {
            id,
            org_id: p.org_id,
            origin: StandardOrigin::Agent,
            stack: p.stack.clone().unwrap_or_else(|| "general".into()),
            category: p.category.clone().unwrap_or_else(|| "practice".into()),
            slug,
            statement: p.statement.clone(),
            rationale: p.rationale.clone(),
            detail_md: p.detail_md.clone(),
            enforcement: Enforcement::Experimental,
            provenance,
            author: Some(p.author),
        },
    )
    .await?;
    Ok(ProposeOutcome::Created(id))
}
