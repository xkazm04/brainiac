//! Graph tables: raw entities, canonical entities, soft-merge links, edges —
//! plus the canonical-hop neighborhood expansion used by retrieval stage 4.

use anyhow::Result;
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use uuid::Uuid;

use crate::retrieval::EntityAnchor;

/// The canonical entities anchoring each of `memory_ids`, batched (one query
/// for the whole result set — never an N+1). A memory anchors on a canonical
/// entity when one of its linked raw entities soft-merges to that canonical:
/// `memory_entities → entity_links → canonical_entities`. RLS-scoped like every
/// read; a memory with no canonical-linked entities simply gets an empty list.
/// Anchors are deduped per memory and returned name-sorted for stable output.
pub async fn canonical_anchors_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<EntityAnchor>>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT DISTINCT me.memory_id, c.id AS canonical_id, c.name AS canonical_name
         FROM memory_entities me
         JOIN entity_links l ON l.entity_id = me.entity_id
         JOIN canonical_entities c ON c.id = l.canonical_id
         WHERE me.memory_id = ANY($1)
         ORDER BY me.memory_id, c.name",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    let mut out: HashMap<Uuid, Vec<EntityAnchor>> = HashMap::new();
    for r in rows {
        out.entry(r.get::<Uuid, _>("memory_id"))
            .or_default()
            .push(EntityAnchor {
                id: r.get::<Uuid, _>("canonical_id"),
                name: r.get::<String, _>("canonical_name"),
            });
    }
    Ok(out)
}

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

/// Every raw-entity → canonical link in the org, as (entity_id, canonical_id).
/// RLS-scoped through the join to `entities`. The resolution eval profile reads
/// this to reconstruct the PREDICTED clustering after running the resolve stage
/// over the gold raw entities (each canonical is a predicted cluster; entities
/// with no link are singletons).
pub async fn links_in_org(conn: &mut PgConnection, org_id: Uuid) -> Result<Vec<(Uuid, Uuid)>> {
    let rows = sqlx::query(
        "SELECT l.entity_id, l.canonical_id
         FROM entity_links l
         JOIN entities e ON e.id = l.entity_id
         WHERE e.org_id = $1",
    )
    .bind(org_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("entity_id"),
                r.get::<Uuid, _>("canonical_id"),
            )
        })
        .collect())
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

// ── alias-aware resolution (Direction 3) ────────────────────────────────

/// Exact (case-insensitive) lexical resolution: find a canonical whose name OR
/// a previously-captured alias matches any of the raw entity's surface forms
/// (`surface_forms` must already be lowercased). An alias is a curated known
/// name for the thing, so an exact hit is unambiguous — cheaper and more
/// precise than embedding similarity, and independent of hand-seeded fixtures.
/// RLS-scoped to the org via canonical_entities.
/// Find a canonical entity of the SAME `kind` whose name or an accumulated alias
/// exactly matches one of `surface_forms` (all lowercased by the caller). Kind
/// agreement is required: an exact surface-form match is only unambiguous when the
/// two entities are the same *kind* — otherwise a "person" named "Mercury" would
/// lexically merge into a "service" called "Mercury".
pub async fn find_canonical_by_name_or_alias(
    conn: &mut PgConnection,
    org_id: Uuid,
    kind: &str,
    surface_forms: &[String],
) -> Result<Option<(Uuid, String)>> {
    if surface_forms.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT id, kind FROM canonical_entities
         WHERE org_id = $1
           AND lower(kind) = lower($2)
           AND (
               lower(name) = ANY($3::text[])
               OR EXISTS (
                   SELECT 1 FROM unnest(aliases) al WHERE lower(al) = ANY($3::text[])
               )
           )
         LIMIT 1",
    )
    .bind(org_id)
    .bind(kind)
    .bind(surface_forms)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("kind"))))
}

/// Fold new surface forms into a canonical's alias set (set-union, dedup),
/// dropping blanks and any form equal to the canonical name — so a canonical
/// merge accumulates the aliases of every raw form linked into it.
pub async fn accumulate_canonical_aliases(
    conn: &mut PgConnection,
    canonical_id: Uuid,
    new_aliases: &[String],
) -> Result<()> {
    if new_aliases.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE canonical_entities c
         SET aliases = ARRAY(
             SELECT DISTINCT a FROM (
                 SELECT unnest(c.aliases) AS a
                 UNION
                 SELECT unnest($2::text[]) AS a
             ) t
             WHERE btrim(a) <> '' AND lower(a) <> lower(c.name)
         )
         WHERE c.id = $1",
    )
    .bind(canonical_id)
    .bind(new_aliases)
    .execute(conn)
    .await?;
    Ok(())
}

// ── persisted canonical embeddings (Direction 2) ────────────────────────

/// pgvector text literal ("[1,2,3]"), bound as text and cast with ::vector —
/// avoids a pgvector client-crate dependency (mirrors memories.rs).
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

/// Persist (or refresh) a canonical's name embedding for a version. Called on
/// canonical create and on any future rename/merge, and by lazy backfill.
pub async fn upsert_canonical_embedding(
    conn: &mut PgConnection,
    canonical_id: Uuid,
    version_id: i32,
    embedding: &[f32],
) -> Result<()> {
    sqlx::query(
        "INSERT INTO canonical_entity_embeddings (canonical_id, embedding_version_id, embedding)
         VALUES ($1, $2, $3::vector)
         ON CONFLICT (canonical_id, embedding_version_id)
         DO UPDATE SET embedding = EXCLUDED.embedding",
    )
    .bind(canonical_id)
    .bind(version_id)
    .bind(vector_literal(embedding))
    .execute(conn)
    .await?;
    Ok(())
}

/// Org canonicals with NO persisted embedding for `version` — the lazy
/// backfill worklist (pre-existing canonicals, or a fresh embedding version).
/// Steady state returns nothing; each canonical is embedded at most once per
/// version, never re-embedded on subsequent resolves.
pub async fn canonicals_missing_embedding(
    conn: &mut PgConnection,
    org_id: Uuid,
    version_id: i32,
) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query(
        "SELECT c.id, c.name FROM canonical_entities c
         WHERE c.org_id = $1
           AND NOT EXISTS (
               SELECT 1 FROM canonical_entity_embeddings ce
               WHERE ce.canonical_id = c.id AND ce.embedding_version_id = $2
           )",
    )
    .bind(org_id)
    .bind(version_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("id"), r.get("name")))
        .collect())
}

/// Canonicals (across ALL orgs) with NO persisted embedding for `version_id`,
/// capped at `limit` — the reembed-backfill worklist for canonical embeddings
/// and its resume point. Deliberately NOT org-scoped, for the same reason as
/// [`crate::memories::missing_embedding`]: reembed is a cross-org operator sweep
/// on the RLS-bypassing admin connection. The resolve stage depends on these,
/// so a model swap must backfill canonicals too or cross-team resolution goes
/// blind in the new vector space.
pub async fn all_canonicals_missing_embedding(
    conn: &mut PgConnection,
    version_id: i32,
    limit: i64,
) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query(
        "SELECT c.id, c.name FROM canonical_entities c
         WHERE NOT EXISTS (
               SELECT 1 FROM canonical_entity_embeddings ce
               WHERE ce.canonical_id = c.id AND ce.embedding_version_id = $1
           )
         ORDER BY c.created_at, c.id
         LIMIT $2",
    )
    .bind(version_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("name")))
        .collect())
}

/// Nearest canonicals to `query_vec` by cosine similarity over persisted
/// embeddings — one SQL round-trip, no live re-embedding of canonicals.
/// RLS-scoped through the join to canonical_entities. Returns
/// (id, name, kind, similarity) best-first.
pub async fn nearest_canonical(
    conn: &mut PgConnection,
    org_id: Uuid,
    version_id: i32,
    query_vec: &[f32],
    limit: i64,
) -> Result<Vec<(Uuid, String, String, f32)>> {
    let rows = sqlx::query(
        "SELECT c.id, c.name, c.kind, 1 - (ce.embedding <=> $1::vector) AS score
         FROM canonical_entity_embeddings ce
         JOIN canonical_entities c ON c.id = ce.canonical_id
         WHERE ce.embedding_version_id = $2 AND c.org_id = $3
         ORDER BY ce.embedding <=> $1::vector
         LIMIT $4",
    )
    .bind(vector_literal(query_vec))
    .bind(version_id)
    .bind(org_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("id"),
                r.get::<String, _>("name"),
                r.get::<String, _>("kind"),
                r.get::<f64, _>("score") as f32,
            )
        })
        .collect())
}
