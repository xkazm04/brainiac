//! Memory rows, embeddings, and the two candidate retrievers (vector + FTS).
//! Every SELECT here joins through `memories`, so RLS scopes all results to
//! the transaction's principal — including the pgvector scan.

use anyhow::Result;

use brainiac_core::{Memory, MemoryKind, MemoryStatus, Visibility};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// Full insert shape (fixtures + extraction pipeline share it).
pub struct NewMemory {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub visibility: Visibility,
    pub status: MemoryStatus,
    pub kind: MemoryKind,
    pub content: String,
    pub language: String,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub superseded_by: Option<Uuid>,
    pub confidence: Option<f32>,
    pub provenance_id: Option<Uuid>,
}

/// Plain INSERT — deliberately NO `ON CONFLICT`: under RLS, an ON CONFLICT
/// arbiter makes Postgres additionally apply the SELECT policy to the new
/// row, so a principal writing a memory for a team it does not belong to
/// (the pipeline case) fails with an RLS violation even though the INSERT
/// policy allows it. Idempotency is the caller's job (fresh UUIDs from the
/// pipeline; TRUNCATE-first for fixture seeding).
pub async fn insert(conn: &mut PgConnection, m: &NewMemory) -> Result<()> {
    sqlx::query(
        "INSERT INTO memories
            (id, org_id, team_id, owner_user_id, visibility, status, kind,
             content, language, valid_from, valid_to, superseded_by, confidence, provenance_id)
         VALUES ($1,$2,$3,$4,$5::visibility,$6::memory_status,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(m.id)
    .bind(m.org_id)
    .bind(m.team_id)
    .bind(m.owner_user_id)
    .bind(m.visibility.as_str())
    .bind(m.status.as_str())
    .bind(m.kind.as_str())
    .bind(&m.content)
    .bind(&m.language)
    .bind(m.valid_from)
    .bind(m.valid_to)
    .bind(m.superseded_by)
    .bind(m.confidence)
    .bind(m.provenance_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn ensure_embedding_version(
    conn: &mut PgConnection,
    model_name: &str,
    dim: i32,
) -> Result<i32> {
    if let Some(row) =
        sqlx::query("SELECT id FROM embedding_versions WHERE model_name = $1 AND dim = $2")
            .bind(model_name)
            .bind(dim)
            .fetch_optional(&mut *conn)
            .await?
    {
        return Ok(row.get::<i32, _>("id"));
    }
    let row = sqlx::query(
        "INSERT INTO embedding_versions (model_name, dim, is_active) VALUES ($1, $2, true)
         RETURNING id",
    )
    .bind(model_name)
    .bind(dim)
    .fetch_one(conn)
    .await?;
    Ok(row.get::<i32, _>("id"))
}

pub async fn upsert_embedding(
    conn: &mut PgConnection,
    memory_id: Uuid,
    version_id: i32,
    embedding: &[f32],
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_embeddings (memory_id, embedding_version_id, embedding)
         VALUES ($1, $2, $3::vector)
         ON CONFLICT (memory_id, embedding_version_id) DO UPDATE SET embedding = EXCLUDED.embedding",
    )
    .bind(memory_id)
    .bind(version_id)
    .bind(vector_literal(embedding))
    .execute(conn)
    .await?;
    Ok(())
}

/// Vector candidates: cosine distance over the given embedding version,
/// RLS-scoped via the join to `memories`. Returns (memory_id, similarity).
pub async fn search_vector(
    conn: &mut PgConnection,
    version_id: i32,
    query: &[f32],
    limit: i64,
) -> Result<Vec<(Uuid, f32)>> {
    let rows = sqlx::query(
        "SELECT m.id, 1 - (e.embedding <=> $1::vector) AS score
         FROM memory_embeddings e
         JOIN memories m ON m.id = e.memory_id
         WHERE e.embedding_version_id = $2
           AND m.status <> 'rejected'
         ORDER BY e.embedding <=> $1::vector
         LIMIT $3",
    )
    .bind(vector_literal(query))
    .bind(version_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<f64, _>("score") as f32))
        .collect())
}

/// Full-text candidates via websearch-style parsing + ts_rank.
pub async fn search_fts(
    conn: &mut PgConnection,
    query: &str,
    limit: i64,
) -> Result<Vec<(Uuid, f32)>> {
    let rows = sqlx::query(
        "SELECT m.id, ts_rank(m.content_fts, q) AS score
         FROM memories m, plainto_tsquery('english', $1) q
         WHERE m.content_fts @@ q
           AND m.status <> 'rejected'
         ORDER BY score DESC
         LIMIT $2",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<f32, _>("score")))
        .collect())
}

fn row_to_memory(r: &sqlx::postgres::PgRow) -> Memory {
    Memory {
        id: r.get("id"),
        org_id: r.get("org_id"),
        team_id: r.get("team_id"),
        owner_user_id: r.get("owner_user_id"),
        visibility: Visibility::parse(r.get::<String, _>("visibility").as_str())
            .unwrap_or(Visibility::Private),
        status: MemoryStatus::parse(r.get::<String, _>("status").as_str())
            .unwrap_or(MemoryStatus::Raw),
        kind: MemoryKind::parse(r.get::<String, _>("kind").as_str()).unwrap_or(MemoryKind::Fact),
        content: r.get("content"),
        valid_from: r.get("valid_from"),
        valid_to: r.get("valid_to"),
        superseded_by: r.get("superseded_by"),
        confidence: r.get("confidence"),
        provenance_id: r.get("provenance_id"),
        created_at: r.get("created_at"),
    }
}

const MEMORY_COLUMNS: &str = "id, org_id, team_id, owner_user_id, visibility::text AS visibility,
     status::text AS status, kind, content, valid_from, valid_to, superseded_by,
     confidence, provenance_id, created_at";

/// Fetch memories by id (RLS filters silently — absent ids were not visible).
pub async fn get_by_ids(conn: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<Memory>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories WHERE id = ANY($1)"
    ))
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_memory).collect())
}

/// Strongest visible memories anchored to any of `entity_ids` (graph
/// expansion stage). Bounded; canonical/candidate only.
pub async fn for_entities(
    conn: &mut PgConnection,
    entity_ids: &[Uuid],
    limit: i64,
) -> Result<Vec<Memory>> {
    if entity_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(&format!(
        "SELECT DISTINCT ON (m.id) {cols}
         FROM memories m
         JOIN memory_entities me ON me.memory_id = m.id
         WHERE me.entity_id = ANY($1)
           AND m.status IN ('canonical', 'candidate')
         ORDER BY m.id, m.created_at DESC
         LIMIT $2",
        cols = MEMORY_COLUMNS
            .split(',')
            .map(|c| format!("m.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .bind(entity_ids)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_memory).collect())
}

pub async fn link_entity(conn: &mut PgConnection, memory_id: Uuid, entity_id: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_entities (memory_id, entity_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(memory_id)
    .bind(entity_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// pgvector text literal ("[1,2,3]") — bound as text and cast with ::vector,
/// avoiding a pgvector client-crate dependency.
fn vector_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 10 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&x.to_string());
    }
    s.push(']');
    s
}
