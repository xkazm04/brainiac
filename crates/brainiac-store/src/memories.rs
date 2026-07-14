//! Memory rows, embeddings, and the two candidate retrievers (vector + FTS).
//! Every SELECT here joins through `memories`, so RLS scopes all results to
//! the transaction's principal — including the pgvector scan.

use anyhow::Result;

use brainiac_core::{Lifecycle, Memory, MemoryKind, MemoryStatus, Visibility};
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
    /// KB-PLAN D2. Callers with no signal use [`Lifecycle::default`] (shipped).
    pub lifecycle: Lifecycle,
    /// KB-PLAN D3. `None` unless the source carried structure worth preserving.
    pub detail_md: Option<String>,
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
             content, lifecycle, detail_md, language, valid_from, valid_to,
             superseded_by, confidence, provenance_id)
         VALUES ($1,$2,$3,$4,$5::visibility,$6::memory_status,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(m.id)
    .bind(m.org_id)
    .bind(m.team_id)
    .bind(m.owner_user_id)
    .bind(m.visibility.as_str())
    .bind(m.status.as_str())
    .bind(m.kind.as_str())
    .bind(&m.content)
    .bind(m.lifecycle.as_str())
    .bind(&m.detail_md)
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
    // Dim-agnostic ANN (0012): whenever a version is ensured/activated, make
    // sure its dimension has a matching partial HNSW index, so a bake-off model
    // at 768/1536/etc. is served by an index instead of silently seq-scanning
    // (0006 only hard-coded 256 + 1024). Idempotent (CREATE INDEX IF NOT
    // EXISTS + an advisory lock inside the SECURITY DEFINER function, which
    // also supplies the DDL privilege the demoted brainiac_app role lacks).
    // Runs for existing versions too — cheap, and it self-heals a version whose
    // index was never built (e.g. rows first written before this shipped).
    ensure_hnsw_index_for_dim(&mut *conn, dim).await?;

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

/// Ensure the partial HNSW index for `dim` exists (0012's SECURITY DEFINER
/// function). Separated so callers that already know the dimension can trigger
/// index creation without touching the versions table.
pub async fn ensure_hnsw_index_for_dim(conn: &mut PgConnection, dim: i32) -> Result<()> {
    sqlx::query("SELECT ensure_hnsw_index($1)")
        .bind(dim)
        .execute(conn)
        .await?;
    Ok(())
}

/// Memories that have NO embedding for `version_id`, oldest first, capped at
/// `limit` — the reembed-backfill worklist AND its resume point (re-running
/// after an interruption just re-reads whatever is still missing). Deliberately
/// NOT org-scoped: reembed is a cross-org OPERATOR sweep run on the migration/
/// admin (table-owner) connection, which bypasses RLS. Under the demoted
/// `brainiac_app` role this same query self-limits to the transaction's org —
/// safe either way, but the operator path needs the whole corpus. Soft-deleted
/// rows are skipped; a re-embedding of a deleted memory would never be served.
pub async fn missing_embedding(
    conn: &mut PgConnection,
    version_id: i32,
    limit: i64,
) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query(
        "SELECT m.id, m.content
         FROM memories m
         WHERE m.deleted_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM memory_embeddings e
               WHERE e.memory_id = m.id AND e.embedding_version_id = $1
           )
         ORDER BY m.created_at, m.id
         LIMIT $2",
    )
    .bind(version_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("content")))
        .collect())
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
///
/// The query is cast to `vector(dim)` and constrained to rows of that
/// dimension so the partial per-dimension HNSW index (0006) can serve the
/// `<=>` ordering. `dim` is the query vector's own length, interpolated as a
/// literal because a `vector` typmod cannot be a bind parameter; it is derived
/// from trusted server state (never user input). The `vector_dims` predicate
/// is a no-op on the result set — every row of one embedding_version already
/// shares that version's dimension — it exists only to unlock the index; where
/// no index exists for `dim` the planner falls back to a correct seq scan.
pub async fn search_vector(
    conn: &mut PgConnection,
    version_id: i32,
    query: &[f32],
    limit: i64,
    filters: &crate::retrieval::RetrievalFilters,
) -> Result<Vec<(Uuid, f32)>> {
    let dim = query.len();
    // HNSW scan hygiene. The scan returns at most `hnsw.ef_search` rows
    // (default 40 < limit 50), and the version/RLS/status conditions are
    // POST-filters — rows they discard leave the frontier and pgvector won't
    // fetch replacements unless iterative scanning is on. Neither matters on
    // a fixture-sized corpus, but at scale both silently starve the candidate
    // pool. No-op (with a warning) outside a transaction or on a seq scan.
    sqlx::query("SET LOCAL hnsw.ef_search = 200")
        .execute(&mut *conn)
        .await?;
    sqlx::query("SET LOCAL hnsw.iterative_scan = strict_order")
        .execute(&mut *conn)
        .await?;
    let rows = sqlx::query(&format!(
        "SELECT m.id, 1 - (e.embedding::vector({dim}) <=> $1::vector({dim})) AS score
         FROM memory_embeddings e
         JOIN memories m ON m.id = e.memory_id
         WHERE e.embedding_version_id = $2
           AND vector_dims(e.embedding) = {dim}
           AND m.status <> 'rejected'
           AND ($4::text[] IS NULL OR m.kind = ANY($4))
           AND ($5::text[] IS NULL OR m.status::text = ANY($5))
           AND ($6::uuid IS NULL OR m.team_id = $6)
           AND ($7::real IS NULL OR m.confidence >= $7)
         ORDER BY e.embedding::vector({dim}) <=> $1::vector({dim})
         LIMIT $3"
    ))
    .bind(vector_literal(query))
    .bind(version_id)
    .bind(limit)
    .bind(filter_kinds(filters))
    .bind(filters.allowed_statuses())
    .bind(filters.team_id)
    .bind(filters.min_confidence)
    .fetch_all(conn)
    .await?;
    let mut hits: Vec<(Uuid, f32)> = rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<f64, _>("score") as f32))
        .collect();
    sort_candidates(&mut hits);
    Ok(hits)
}

/// Deterministic candidate ordering: score descending, id ascending on ties.
/// Postgres leaves tie order to the access path (heap order on a seq scan,
/// graph order on an HNSW scan), and the deterministic test embedder produces
/// many exact ties — without this, eval rankings shift whenever the planner
/// changes its mind. Fixture IDs are deterministic, so this keeps eval runs
/// reproducible across plans.
fn sort_candidates(hits: &mut [(Uuid, f32)]) {
    hits.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
}

/// Full-text candidates via websearch-style parsing + ts_rank.
///
/// The stored tsvector is built per row under the memory's own language config
/// (0007: 'english' for English, 'simple' for Czech/unknown). The query
/// language is unknown at call time, so we parse it BOTH ways with
/// `websearch_to_tsquery` (quotes, OR, `-term` all honored) and match with an
/// OR: the 'english' query hits English-indexed rows, the 'simple' query hits
/// Czech/unknown-indexed rows. Score is the stronger of the two ranks so a row
/// isn't penalized for the config it did not match under.
pub async fn search_fts(
    conn: &mut PgConnection,
    query: &str,
    limit: i64,
    filters: &crate::retrieval::RetrievalFilters,
) -> Result<Vec<(Uuid, f32)>> {
    let rows = sqlx::query(
        "SELECT m.id,
                greatest(ts_rank(m.content_fts, qe), ts_rank(m.content_fts, qs)) AS score
         FROM memories m,
              websearch_to_tsquery('english', $1) qe,
              websearch_to_tsquery('simple', $1) qs
         WHERE (m.content_fts @@ qe OR m.content_fts @@ qs)
           AND m.status <> 'rejected'
           AND ($3::text[] IS NULL OR m.kind = ANY($3))
           AND ($4::text[] IS NULL OR m.status::text = ANY($4))
           AND ($5::uuid IS NULL OR m.team_id = $5)
           AND ($6::real IS NULL OR m.confidence >= $6)
         ORDER BY score DESC
         LIMIT $2",
    )
    .bind(query)
    .bind(limit)
    .bind(filter_kinds(filters))
    .bind(filters.allowed_statuses())
    .bind(filters.team_id)
    .bind(filters.min_confidence)
    .fetch_all(conn)
    .await?;
    let mut hits: Vec<(Uuid, f32)> = rows
        .into_iter()
        .map(|r| (r.get::<Uuid, _>("id"), r.get::<f32, _>("score")))
        .collect();
    sort_candidates(&mut hits);
    Ok(hits)
}

/// `None` when no kind filter applies (SQL treats NULL as "all kinds"). Typed
/// kinds are rendered to their canonical DB strings for the `= ANY($n)` bind.
fn filter_kinds(filters: &crate::retrieval::RetrievalFilters) -> Option<Vec<String>> {
    if filters.kinds.is_empty() {
        None
    } else {
        Some(
            filters
                .kinds
                .iter()
                .map(|k| k.as_str().to_string())
                .collect(),
        )
    }
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
        lifecycle: Lifecycle::parse(r.get::<String, _>("lifecycle").as_str()).unwrap_or_default(),
        detail_md: r.get("detail_md"),
        valid_from: r.get("valid_from"),
        valid_to: r.get("valid_to"),
        superseded_by: r.get("superseded_by"),
        confidence: r.get("confidence"),
        provenance_id: r.get("provenance_id"),
        created_at: r.get("created_at"),
    }
}

const MEMORY_COLUMNS: &str = "id, org_id, team_id, owner_user_id, visibility::text AS visibility,
     status::text AS status, kind, content, lifecycle, detail_md, valid_from, valid_to,
     superseded_by, confidence, provenance_id, created_at";

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

// ── freshness lifecycle (TTL + re-verification) ─────────────────────────

/// Live memories whose validity window closes within `within_days` (or has
/// already closed), oldest boundary first — the re-verification queue.
/// Only candidate/canonical rows: raw/rejected aren't worth re-confirming
/// and deprecated rows already ended deliberately.
pub async fn expiring(
    conn: &mut PgConnection,
    within_days: i64,
    limit: i64,
) -> Result<Vec<Memory>> {
    let rows = sqlx::query(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories
         WHERE valid_to IS NOT NULL
           AND valid_to <= now() + make_interval(days => $1::int)
           AND status IN ('candidate', 'canonical')
           AND superseded_by IS NULL
         ORDER BY valid_to ASC
         LIMIT $2"
    ))
    .bind(within_days)
    .bind(limit.clamp(1, 200))
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_memory).collect())
}

/// Re-verify a memory: extend its validity window `days` from now (not from
/// the old boundary, so a long-expired row doesn't come back pre-stale).
/// Returns the new boundary, or None when the id doesn't resolve under the
/// caller's RLS (or the row is superseded — supersessions are final).
pub async fn extend_validity(
    conn: &mut PgConnection,
    id: Uuid,
    days: i64,
) -> Result<Option<DateTime<Utc>>> {
    let row = sqlx::query(
        "UPDATE memories
         SET valid_to = now() + make_interval(days => $2::int), updated_at = now()
         WHERE id = $1 AND superseded_by IS NULL
         RETURNING valid_to",
    )
    .bind(id)
    .bind(days)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.get("valid_to")))
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
