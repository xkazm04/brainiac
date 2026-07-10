//! Governance writes: sources, provenance, promotions, contradictions —
//! plus the status transition the promote worker applies.

use anyhow::Result;
use brainiac_core::{ActorKind, MemoryStatus, PolicyDecision};
use sqlx::PgConnection;
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
