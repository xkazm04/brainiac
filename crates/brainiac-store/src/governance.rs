//! Governance writes: sources, provenance, promotions, contradictions —
//! plus the status transition the promote worker applies.

use anyhow::Result;
use brainiac_core::{ActorKind, MemoryStatus, PolicyDecision};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use uuid::Uuid;

pub async fn insert_source(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    team_id: Option<Uuid>,
    kind: &str,
    raw_text: &str,
    created_by: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources (id, org_id, team_id, kind, raw_text, created_by)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(id)
    .bind(org_id)
    .bind(team_id)
    .bind(kind)
    .bind(raw_text)
    .bind(created_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_source_text(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<(Option<Uuid>, String)>> {
    use sqlx::Row;
    let row = sqlx::query("SELECT team_id, raw_text FROM sources WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| {
        (
            r.get("team_id"),
            r.get::<Option<String>, _>("raw_text").unwrap_or_default(),
        )
    }))
}

#[allow(clippy::too_many_arguments)] // mirrors the provenance row shape
pub async fn insert_provenance(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    actor_kind: ActorKind,
    actor_id: &str,
    model_ref: Option<&str>,
    source_id: Option<Uuid>,
    pipeline_run_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO provenance (id, org_id, actor_kind, actor_id, model_ref, source_id, pipeline_run_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(id)
    .bind(org_id)
    .bind(actor_kind.as_str())
    .bind(actor_id)
    .bind(model_ref)
    .bind(source_id)
    .bind(pipeline_run_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_promotion(
    conn: &mut PgConnection,
    org_id: Uuid,
    memory_id: Uuid,
    from: MemoryStatus,
    to: MemoryStatus,
    decision: PolicyDecision,
    rule: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status, policy_decision, policy_rule)
         VALUES ($1,$2,$3,$4::memory_status,$5::memory_status,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(memory_id)
    .bind(from.as_str())
    .bind(to.as_str())
    .bind(decision.as_str())
    .bind(rule)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_memory_status(
    conn: &mut PgConnection,
    memory_id: Uuid,
    status: MemoryStatus,
) -> Result<()> {
    sqlx::query("UPDATE memories SET status = $2::memory_status, updated_at = now() WHERE id = $1")
        .bind(memory_id)
        .bind(status.as_str())
        .execute(conn)
        .await?;
    Ok(())
}

/// Apply a governance-decided supersession: the losing memory is deprecated,
/// pointed at the winner, and its validity window closed at `now()` so the
/// temporal dedupe (retrieval stage 5) serves ONLY the winner from now on —
/// the piece ARCHITECTURE.md §5 row 5 promised but no code path delivered.
///
/// The deprecation is recorded in `promotions`, the same status-transition
/// audit log every other status change flows through, stamped with who/what
/// applied it (`reviewer_id`): a human maintainer (`applied_by = Some`, decided
/// `approved`) or a policy actor (`None`, `auto_approved`). `rule` names the
/// trigger, e.g. `contradiction_supersede`.
///
/// Idempotent and RLS-safe: a memory already superseded, or not updatable
/// under the caller's scope, is left untouched and returns `false`.
pub async fn apply_supersession(
    conn: &mut PgConnection,
    org_id: Uuid,
    loser: Uuid,
    winner: Uuid,
    applied_by: Option<Uuid>,
    rule: &str,
) -> Result<bool> {
    // Snapshot the pre-transition status (also the existence + RLS gate) before
    // the update overwrites it; a live supersession is final, so skip rows that
    // already carry a forward pointer.
    let Some(from_status) = sqlx::query_scalar::<_, String>(
        "SELECT status::text FROM memories WHERE id = $1 AND superseded_by IS NULL",
    )
    .bind(loser)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };

    sqlx::query(
        "UPDATE memories
         SET valid_to = now(), superseded_by = $2,
             status = 'deprecated'::memory_status, updated_at = now()
         WHERE id = $1 AND superseded_by IS NULL",
    )
    .bind(loser)
    .bind(winner)
    .execute(&mut *conn)
    .await?;

    let decision = if applied_by.is_some() {
        "approved"
    } else {
        "auto_approved"
    };
    sqlx::query(
        "INSERT INTO promotions
            (id, org_id, memory_id, from_status, to_status,
             policy_decision, policy_rule, reviewer_id, reviewed_at)
         VALUES ($1,$2,$3,$4::memory_status,'deprecated'::memory_status,$5,$6,$7, now())",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(loser)
    .bind(from_status)
    .bind(decision)
    .bind(rule)
    .bind(applied_by)
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// One side of an OPEN contradiction as seen from a result memory: the
/// contradiction row and the memory it conflicts with.
#[derive(Debug, Clone)]
pub struct ContradictionFlag {
    pub contradiction_id: Uuid,
    /// The other memory in the pair (the one the result memory contradicts).
    pub counterpart_id: Uuid,
}

/// Open contradictions touching any memory in `memory_ids`, keyed by the
/// in-set memory to the counterpart it conflicts with. ONE batched query for
/// the whole result set — never an N+1 (mirrors [`feedback::trust_for`]).
///
/// RLS-safe both ways: contradictions is org-scoped, but the counterpart may
/// live in a team the caller cannot read. Joining BOTH sides to `memories`
/// makes the (team-scoped) memories SELECT policy filter out any pair whose
/// counterpart is invisible — so we never surface, nor even reveal the id of,
/// a memory the caller isn't allowed to see (no existence oracle).
pub async fn open_contradictions_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ContradictionFlag>>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT ct.id AS contradiction_id, ct.memory_a, ct.memory_b
         FROM contradictions ct
         JOIN memories ma ON ma.id = ct.memory_a
         JOIN memories mb ON mb.id = ct.memory_b
         WHERE ct.status = 'open'
           AND (ct.memory_a = ANY($1) OR ct.memory_b = ANY($1))",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    let wanted: std::collections::HashSet<Uuid> = memory_ids.iter().copied().collect();
    let mut out: HashMap<Uuid, Vec<ContradictionFlag>> = HashMap::new();
    for r in rows {
        let cid: Uuid = r.get("contradiction_id");
        let a: Uuid = r.get("memory_a");
        let b: Uuid = r.get("memory_b");
        // Flag whichever side(s) are actually in the result set, pointing at the
        // other side (guaranteed visible by the double join above).
        if wanted.contains(&a) {
            out.entry(a).or_default().push(ContradictionFlag {
                contradiction_id: cid,
                counterpart_id: b,
            });
        }
        if wanted.contains(&b) {
            out.entry(b).or_default().push(ContradictionFlag {
                contradiction_id: cid,
                counterpart_id: a,
            });
        }
    }
    Ok(out)
}

pub async fn insert_contradiction(
    conn: &mut PgConnection,
    org_id: Uuid,
    memory_a: Uuid,
    memory_b: Uuid,
    detected_by: &str,
    suggested_resolution: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
         VALUES ($1,$2,$3,$4,$5,'open',$6)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(memory_a)
    .bind(memory_b)
    .bind(detected_by)
    .bind(suggested_resolution)
    .execute(conn)
    .await?;
    Ok(())
}
