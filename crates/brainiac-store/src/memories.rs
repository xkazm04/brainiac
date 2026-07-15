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
    /// A short label for the claim — what the archive anchors a row on.
    /// `None` is normal: the extractor does not write one yet, and readers fall
    /// back to `content` (migration 0023).
    pub title: Option<String>,
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
             content, title, lifecycle, detail_md, language, valid_from, valid_to,
             superseded_by, confidence, provenance_id)
         VALUES ($1,$2,$3,$4,$5::visibility,$6::memory_status,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
    )
    .bind(m.id)
    .bind(m.org_id)
    .bind(m.team_id)
    .bind(m.owner_user_id)
    .bind(m.visibility.as_str())
    .bind(m.status.as_str())
    .bind(m.kind.as_str())
    .bind(&m.content)
    .bind(&m.title)
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

/// Expire RAW memories older than `ttl_days` to `rejected` — the raw-TTL sweep
/// (UAT P0.3). Cross-org by design: runs on the RLS-bypassing admin pool from
/// the sweep scheduler, exactly like the other operator sweeps.
///
/// Why `rejected` and not deletion: rejected drops the row from every retrieval
/// path (the one standing exclusion) while preserving it for audit — and each
/// expiry leaves a `promotions` row naming this sweep, so "who decided this
/// belief goes away" has the same answer shape as every other status change:
/// nobody-in-particular is never the answer, a named actor is, and here the
/// actor is the org's own configured janitor.
///
/// Why raw memories matter at all: default retrieval excludes only `rejected`,
/// so an unreviewed raw belief is SERVED — with implied authority — for as long
/// as it exists. Auto-capture creates them faster than humans review them; past
/// the TTL the honest reading of "nobody has looked at this in a month" is not
/// "pending", it is "declined by neglect", and the corpus should say so.
///
/// Returns (memories expired, orgs touched).
pub async fn expire_stale_raw(pool: &sqlx::PgPool, ttl_days: i64) -> Result<(u64, u64)> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query(
        "WITH doomed AS (
             SELECT id, org_id FROM memories
             WHERE status = 'raw'
               AND deleted_at IS NULL
               AND created_at < now() - make_interval(days => $1::int)
             FOR UPDATE SKIP LOCKED
         ),
         expired AS (
             UPDATE memories m
             SET status = 'rejected'::memory_status, updated_at = now()
             FROM doomed d WHERE m.id = d.id
             RETURNING m.id, m.org_id
         ),
         audited AS (
             INSERT INTO promotions
                 (id, org_id, memory_id, from_status, to_status,
                  policy_decision, policy_rule, reviewed_at)
             SELECT gen_random_uuid(), e.org_id, e.id,
                    'raw'::memory_status, 'rejected'::memory_status,
                    'auto_rejected', 'raw_ttl_sweep', now()
             FROM expired e
             RETURNING 1
         )
         SELECT count(*) AS expired, count(DISTINCT org_id) AS orgs FROM expired",
    )
    .bind(ttl_days)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        rows.get::<i64, _>("expired") as u64,
        rows.get::<i64, _>("orgs") as u64,
    ))
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
    // `is_active` means "backfill complete, safe to serve" (migration 0001:
    // "backfill, flip is_active"). A brand-new version is born complete ONLY when
    // it is the first version in the corpus — a fresh system has nothing to
    // backfill and ingest fills it going forward. A later swap-target version is
    // born INCOMPLETE: reembed must fully drain both backfill loops before it
    // calls `activate_embedding_version`. This is what stops an interrupted
    // reembed from leaving a half-populated version silently servable.
    let row = sqlx::query(
        "INSERT INTO embedding_versions (model_name, dim, is_active)
         VALUES ($1, $2, NOT EXISTS (SELECT 1 FROM embedding_versions))
         RETURNING id",
    )
    .bind(model_name)
    .bind(dim)
    .fetch_one(conn)
    .await?;
    Ok(row.get::<i32, _>("id"))
}

/// Mark an embedding version fully backfilled and safe to serve. Called by
/// `reembed` only after both backfill loops have drained. Idempotent.
pub async fn activate_embedding_version(conn: &mut PgConnection, version_id: i32) -> Result<()> {
    sqlx::query("UPDATE embedding_versions SET is_active = true WHERE id = $1")
        .bind(version_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Resolve the version a **serve** path (REST/MCP) should use: ensure it exists,
/// then require it be `is_active` (backfill-complete). A configured-but-not-yet-
/// backfilled swap-target version fails loudly here instead of silently serving a
/// partially-embedded corpus — the operator must run `reembed` to completion (which
/// activates it) or revert the embedder config. Writers keep using
/// `ensure_embedding_version` (they populate the version and don't require active).
pub async fn serving_embedding_version(
    conn: &mut PgConnection,
    model_name: &str,
    dim: i32,
) -> Result<i32> {
    let id = ensure_embedding_version(&mut *conn, model_name, dim).await?;
    let active: bool = sqlx::query_scalar("SELECT is_active FROM embedding_versions WHERE id = $1")
        .bind(id)
        .fetch_one(conn)
        .await?;
    if !active {
        anyhow::bail!(
            "embedding version {id} ({model_name}, dim {dim}) is not fully backfilled — \
             run `reembed` to completion before serving this model, or revert the embedder config"
        );
    }
    Ok(id)
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

/// Canonical memories that CHANGED within the window (newest first) — the
/// source a digest binding draws from (migration 0027). `updated_at`, not
/// `created_at`: a promotion or supersession is exactly the kind of change a
/// digest exists to surface, and both touch `updated_at`. RLS filters as
/// always, so a digest can never show its reader a change they may not see.
pub async fn recent_canonical(
    conn: &mut PgConnection,
    window_days: i64,
    limit: i64,
) -> Result<Vec<Memory>> {
    let rows = sqlx::query(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories
         WHERE status = 'canonical'
           AND superseded_by IS NULL
           AND deleted_at IS NULL
           AND updated_at > now() - make_interval(days => $1::int)
         ORDER BY updated_at DESC
         LIMIT $2"
    ))
    .bind(window_days)
    .bind(limit)
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
    // Dedupe and rank are NESTED deliberately. `DISTINCT ON (m.id)` forces the
    // ORDER BY to lead with `m.id`, which also orders the FINAL result set — so
    // `… ORDER BY m.id, m.created_at DESC LIMIT $2` returned the N smallest UUIDs
    // (and the trailing `created_at DESC` was dead: it only breaks ties within one
    // m.id group, and there is exactly one row per id). Retrieval scores every
    // graph extra with the identical `graph_relevance(anchor_strength)`, so this
    // SELECTION is the whole result — an arbitrary, UUID-keyed pick for the
    // headline "cross-team knowledge surfaces here" feature. With time-ordered
    // (v7) UUIDs it deterministically returned the OLDEST, the opposite of intent.
    //
    // The inner query only dedupes the entity join fan-out; the outer one ranks
    // and applies the cap, so the limit keeps the most recent.
    let rows = sqlx::query(&format!(
        "SELECT * FROM (
             SELECT DISTINCT ON (m.id) {cols}
             FROM memories m
             JOIN memory_entities me ON me.memory_id = m.id
             WHERE me.entity_id = ANY($1)
               AND m.status IN ('canonical', 'candidate')
             ORDER BY m.id
         ) s
         ORDER BY s.created_at DESC
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

/// Why an [`extend_validity`] call did not move the boundary. The old signature
/// returned a bare `Option` and collapsed two materially different failures —
/// "you cannot see this memory" and "this memory is superseded" — into one
/// `None`, so callers could not tell a 404 from a 409 and reported both as
/// success. Each case is now nameable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendOutcome {
    /// The window moved; carries the new boundary.
    Extended(DateTime<Utc>),
    /// The row is visible but carries `superseded_by` — supersessions are final,
    /// so its window is frozen. A conflict, not a missing row.
    Superseded,
    /// No memory with this id resolves under the caller's RLS view. Callers must
    /// answer this exactly as they answer a nonexistent id (no existence oracle).
    NotFound,
}

/// Re-verify a memory: extend its validity window `days` from now (not from
/// the old boundary, so a long-expired row doesn't come back pre-stale).
///
/// The visibility SELECT is load-bearing, not a convenience read. `memories_read`
/// (0001 + 0002) enforces the three-tier private/team/org model, but
/// `memories_update` USING is only `org_id = current_setting('app.org_id')`.
/// Updating first and inspecting after would therefore let a caller move
/// `valid_to` on another user's private memory — a write to a row they cannot
/// read. Gating the UPDATE behind a read-policy SELECT in the same transaction
/// closes that for this path (see
/// docs/harness/refactor-bughunt-2026-07-14/store-memories-retrieval-queue.md §2,
/// which tracks the underlying policy asymmetry for the other writers).
pub async fn extend_validity(
    conn: &mut PgConnection,
    id: Uuid,
    days: i64,
) -> Result<ExtendOutcome> {
    let Some(row) = sqlx::query("SELECT superseded_by FROM memories WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(ExtendOutcome::NotFound);
    };
    if row.get::<Option<Uuid>, _>("superseded_by").is_some() {
        return Ok(ExtendOutcome::Superseded);
    }
    let updated = sqlx::query(
        "UPDATE memories
         SET valid_to = now() + make_interval(days => $2::int), updated_at = now()
         WHERE id = $1 AND superseded_by IS NULL
         RETURNING valid_to",
    )
    .bind(id)
    .bind(days)
    .fetch_optional(&mut *conn)
    .await?;
    // The only predicate the UPDATE adds over the SELECT is `superseded_by IS
    // NULL`, so a 0-row update means it was superseded between the two
    // statements — the same conflict, found a moment later.
    Ok(match updated {
        Some(r) => ExtendOutcome::Extended(r.get("valid_to")),
        None => ExtendOutcome::Superseded,
    })
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
