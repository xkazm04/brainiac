//! Graph tables: raw entities, canonical entities, soft-merge links, edges —
//! plus the canonical-hop neighborhood expansion used by retrieval stage 4.

use anyhow::Result;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn insert_entity(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    team_id: Option<Uuid>,
    name: &str,
    kind: &str,
    aliases: &[String],
    provenance_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO entities (id, org_id, team_id, name, kind, aliases, provenance_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(team_id)
    .bind(name)
    .bind(kind)
    .bind(aliases)
    .bind(provenance_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_canonical(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    name: &str,
    kind: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO canonical_entities (id, org_id, name, kind)
         VALUES ($1,$2,$3,$4) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(name)
    .bind(kind)
    .execute(conn)
    .await?;
    Ok(())
}

/// Soft merge (reversible): raw entity → canonical target.
pub async fn link(
    conn: &mut PgConnection,
    entity_id: Uuid,
    canonical_id: Uuid,
    confidence: f32,
    method: &str,
    confirmed_by: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO entity_links (entity_id, canonical_id, confidence, method, confirmed_by)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (entity_id, canonical_id)
         DO UPDATE SET confidence = EXCLUDED.confidence, method = EXCLUDED.method",
    )
    .bind(entity_id)
    .bind(canonical_id)
    .bind(confidence)
    .bind(method)
    .bind(confirmed_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_edge(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    src: Uuid,
    dst: Uuid,
    relation: &str,
    memory_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO edges (id, org_id, src_entity, dst_entity, relation, memory_id)
         VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(src)
    .bind(dst)
    .bind(relation)
    .bind(memory_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Entities anchoring the given memories (retrieval stage 4 input).
pub async fn anchors_of_memories(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<Vec<Uuid>> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows =
        sqlx::query("SELECT DISTINCT entity_id FROM memory_entities WHERE memory_id = ANY($1)")
            .bind(memory_ids)
            .fetch_all(conn)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| r.get::<Uuid, _>("entity_id"))
        .collect())
}

/// Neighborhood of the anchor entities, up to `hops` (1 or 2):
/// one hop = (a) same-canonical siblings via entity_links — the cross-team
/// bridge — and (b) direct edge neighbors. The second hop repeats over the
/// first hop frontier. Returns entity ids EXCLUDING the anchors.
pub async fn neighbors(
    conn: &mut PgConnection,
    anchors: &[Uuid],
    hops: u8,
    limit: i64,
) -> Result<Vec<Uuid>> {
    if anchors.is_empty() || hops == 0 {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "WITH RECURSIVE frontier(entity_id, depth) AS (
             SELECT unnest($1::uuid[]) AS entity_id, 0 AS depth
           UNION
             SELECT n.next_id, f.depth + 1
             FROM frontier f
             JOIN LATERAL (
                 SELECT l2.entity_id AS next_id
                 FROM entity_links l1
                 JOIN entity_links l2 ON l2.canonical_id = l1.canonical_id
                 WHERE l1.entity_id = f.entity_id
               UNION
                 SELECT e.dst_entity FROM edges e WHERE e.src_entity = f.entity_id
               UNION
                 SELECT e.src_entity FROM edges e WHERE e.dst_entity = f.entity_id
             ) n ON true
             WHERE f.depth < $2
         )
         SELECT DISTINCT entity_id FROM frontier
         WHERE depth > 0 AND entity_id <> ALL($1::uuid[])
         LIMIT $3",
    )
    .bind(anchors)
    .bind(i32::from(hops))
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| r.get::<Uuid, _>("entity_id"))
        .collect())
}

/// Case-insensitive lookup of a raw entity by name within a team scope
/// (extraction get-or-create path).
pub async fn find_by_name(
    conn: &mut PgConnection,
    org_id: Uuid,
    team_id: Option<Uuid>,
    name: &str,
) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        "SELECT id FROM entities
         WHERE org_id = $1 AND team_id IS NOT DISTINCT FROM $2 AND lower(name) = lower($3)
         LIMIT 1",
    )
    .bind(org_id)
    .bind(team_id)
    .bind(name)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

/// All canonical entities of the org (resolve blocking candidates; small in
/// v0 — revisit with ANN over canonical embeddings at scale).
pub async fn list_canonicals(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<Vec<(Uuid, String, String)>> {
    let rows = sqlx::query("SELECT id, name, kind FROM canonical_entities WHERE org_id = $1")
        .bind(org_id)
        .fetch_all(conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("id"), r.get("name"), r.get("kind")))
        .collect())
}
