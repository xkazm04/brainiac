//! Document layer repository (ARCHITECTURE.md §8, migration 0017).
//!
//! Pages are projections. Nothing in this module stores knowledge — it stores
//! *bindings* (what a section should pull), *revisions* (what it pulled, and
//! from which memories), and *dependencies* (which pages a memory feeds).
//!
//! The dependency index is the load-bearing piece: [`mark_dirty_for_memory`] is
//! what makes a resolved contradiction propagate to every page that cited the
//! losing claim, without anyone remembering to edit a page. That single call is
//! the difference between this and every wiki that has ever rotted.

use anyhow::Result;
use brainiac_core::{
    DocKind, DocStatus, Document, DocumentRevision, DocumentSection, RevisionPolicy,
    SectionBinding, SectionMode, Visibility,
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub struct NewDocument {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub slug: String,
    pub title: String,
    pub visibility: Visibility,
    pub doc_kind: DocKind,
}

pub async fn insert_document(conn: &mut PgConnection, d: &NewDocument) -> Result<()> {
    sqlx::query(
        "INSERT INTO documents (id, org_id, team_id, slug, title, visibility, doc_kind, status)
         VALUES ($1,$2,$3,$4,$5,$6::visibility,$7,'draft')",
    )
    .bind(d.id)
    .bind(d.org_id)
    .bind(d.team_id)
    .bind(&d.slug)
    .bind(&d.title)
    .bind(d.visibility.as_str())
    .bind(d.doc_kind.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

pub struct NewSection {
    pub id: Uuid,
    pub document_id: Uuid,
    pub org_id: Uuid,
    pub position: i32,
    pub heading: String,
    pub mode: SectionMode,
    pub binding: Option<SectionBinding>,
    pub pinned_content: Option<String>,
}

pub async fn insert_section(conn: &mut PgConnection, s: &NewSection) -> Result<()> {
    let binding = s.binding.as_ref().map(serde_json::to_value).transpose()?;
    sqlx::query(
        "INSERT INTO document_sections
            (id, document_id, org_id, position, heading, mode, binding, pinned_content)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(s.id)
    .bind(s.document_id)
    .bind(s.org_id)
    .bind(s.position)
    .bind(&s.heading)
    .bind(s.mode.as_str())
    .bind(binding)
    .bind(&s.pinned_content)
    .execute(conn)
    .await?;
    Ok(())
}

fn row_to_document(r: &sqlx::postgres::PgRow) -> Document {
    Document {
        id: r.get("id"),
        org_id: r.get("org_id"),
        team_id: r.get("team_id"),
        slug: r.get("slug"),
        title: r.get("title"),
        visibility: Visibility::parse(r.get::<String, _>("visibility").as_str())
            .unwrap_or(Visibility::Team),
        doc_kind: DocKind::parse(r.get::<String, _>("doc_kind").as_str()).unwrap_or_default(),
        status: DocStatus::parse(r.get::<String, _>("status").as_str()).unwrap_or_default(),
        current_revision: r.get("current_revision"),
        dirty_at: r.get("dirty_at"),
        updated_at: r.get("updated_at"),
    }
}

const DOC_COLUMNS: &str = "id, org_id, team_id, slug, title, visibility::text AS visibility,
     doc_kind, status, current_revision, dirty_at, updated_at";

pub async fn get_document(conn: &mut PgConnection, id: Uuid) -> Result<Option<Document>> {
    let row = sqlx::query(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_document))
}

pub async fn get_document_by_slug(conn: &mut PgConnection, slug: &str) -> Result<Option<Document>> {
    let row = sqlx::query(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE slug = $1"
    ))
    .bind(slug)
    .fetch_optional(conn)
    .await?;
    Ok(row.as_ref().map(row_to_document))
}

pub async fn list_documents(conn: &mut PgConnection) -> Result<Vec<Document>> {
    let rows = sqlx::query(&format!(
        "SELECT {DOC_COLUMNS} FROM documents ORDER BY updated_at DESC"
    ))
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_document).collect())
}

/// Lexical search over pages by title, slug, or current-revision body (F-5:
/// gives REST the doc search the MCP surface already had). Deliberately simple
/// LIKE, like the MCP `doc_search` and for the same reason: pages are few and
/// titled for what they cover. The match is done in a subquery so the outer
/// `DOC_COLUMNS` select stays join-free (no ambiguous `id`); RLS scopes both.
pub async fn search_documents(
    conn: &mut PgConnection,
    query: &str,
    limit: i64,
) -> Result<Vec<Document>> {
    let rows = sqlx::query(&format!(
        "SELECT {DOC_COLUMNS} FROM documents
         WHERE id IN (
             SELECT d.id FROM documents d
             LEFT JOIN document_revisions r ON r.id = d.current_revision
             WHERE d.title ILIKE '%' || $1 || '%'
                OR d.slug ILIKE '%' || $1 || '%'
                OR r.content_md ILIKE '%' || $1 || '%'
         )
         ORDER BY updated_at DESC
         LIMIT $2"
    ))
    .bind(query)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_document).collect())
}

/// Pages whose underlying memories moved. The compose worker's work list.
/// Pages awaiting recompose. Skips any page inside its failure backoff window
/// (0021): a deterministically-failing compose would otherwise be re-picked on
/// every tick, burning an LLM call each time and crowding healthy pages out of the
/// tick's limit. `compose_next_at IS NULL` is the healthy case.
pub async fn dirty_documents(conn: &mut PgConnection, limit: i64) -> Result<Vec<Document>> {
    let rows = sqlx::query(&format!(
        "SELECT {DOC_COLUMNS} FROM documents
         WHERE dirty_at IS NOT NULL AND status <> 'archived'
           AND (compose_next_at IS NULL OR compose_next_at <= now())
         ORDER BY dirty_at LIMIT $1"
    ))
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_document).collect())
}

pub async fn sections(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<DocumentSection>> {
    let rows = sqlx::query(
        "SELECT id, document_id, position, heading, mode, binding, pinned_content
         FROM document_sections WHERE document_id = $1 ORDER BY position",
    )
    .bind(document_id)
    .fetch_all(conn)
    .await?;
    rows.iter()
        .map(|r| {
            let binding: Option<serde_json::Value> = r.get("binding");
            Ok(DocumentSection {
                id: r.get("id"),
                document_id: r.get("document_id"),
                position: r.get("position"),
                heading: r.get("heading"),
                mode: SectionMode::parse(r.get::<String, _>("mode").as_str())
                    .unwrap_or(SectionMode::Pinned),
                binding: binding.map(serde_json::from_value).transpose()?,
                pinned_content: r.get("pinned_content"),
            })
        })
        .collect()
}

/// Update a PINNED section's prose (KB4). Only pinned sections can be written
/// this way — a composed section's text is a projection, and overwriting it here
/// would fork the truth. The API enforces that; this function is the honest
/// primitive underneath it.
///
/// Optimistic concurrency: the write lands only if the stored prose still equals
/// `expected_current` (the content the caller read before editing). Under READ
/// COMMITTED this catches a concurrent editor who committed in between — the WHERE
/// then sees the new value and matches 0 rows. Returns `false` on such a conflict
/// so the caller can 409 instead of silently clobbering the other save.
pub async fn update_pinned(
    conn: &mut PgConnection,
    section_id: Uuid,
    content: &str,
    expected_current: Option<&str>,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE document_sections SET pinned_content = $2
         WHERE id = $1 AND mode = 'pinned' AND pinned_content IS NOT DISTINCT FROM $3",
    )
    .bind(section_id)
    .bind(content)
    .bind(expected_current)
    .execute(conn)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// THE anti-rot call. A canonical memory was inserted, superseded, deprecated,
/// or lost a contradiction → every page that cited it is now suspect. Marking
/// them dirty is cheap; the compose worker does the expensive part later.
/// Returns how many pages were affected (0 is the common, healthy case).
pub async fn mark_dirty_for_memory(conn: &mut PgConnection, memory_id: Uuid) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE documents SET dirty_at = COALESCE(dirty_at, now()), updated_at = now()
         WHERE id IN (SELECT document_id FROM document_dependencies WHERE memory_id = $1)",
    )
    .bind(memory_id)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// Record a failed compose and schedule the retry (0021).
///
/// The page STAYS dirty — a failed compose must retry, never silently leave a
/// stale page looking fresh — but it retries on an exponential schedule instead of
/// on the very next tick. Returns the new attempt count so the caller can log the
/// severity; a page with a climbing count is queryable and no longer invisibly
/// stuck.
///
/// Backoff: `base * 2^(attempts-1)`, capped, so one poison page costs a handful of
/// LLM calls a day rather than one per tick forever.
pub async fn record_compose_failure(
    conn: &mut PgConnection,
    document_id: Uuid,
    base_secs: i64,
    max_secs: i64,
) -> Result<i32> {
    let row = sqlx::query(
        "UPDATE documents
         SET compose_attempts = compose_attempts + 1,
             compose_next_at = now() + make_interval(
                 secs => LEAST($2::bigint * (2 ^ LEAST(compose_attempts, 20))::bigint, $3::bigint)::double precision
             ),
             updated_at = now()
         WHERE id = $1
         RETURNING compose_attempts",
    )
    .bind(document_id)
    .bind(base_secs)
    .bind(max_secs)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row
        .map(|r| r.get::<i32, _>("compose_attempts"))
        .unwrap_or(0))
}

/// Record that a page was served to a reader (migration 0025) — the raw event
/// behind the liquidity signals. `via` is the channel (`http` | `mcp`), and
/// `was_dirty` captures whether the page was serving a superseded belief at
/// the moment of the read — the fact that ranks rot by harm.
///
/// Callers run this in its OWN transaction after the read has been served: an
/// analytics insert must never fail a read, and inside the serving transaction
/// a failed insert would poison the commit.
pub async fn record_read(
    conn: &mut PgConnection,
    org_id: Uuid,
    document_id: Uuid,
    via: &str,
    was_dirty: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO document_reads (org_id, document_id, via, was_dirty)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(document_id)
    .bind(via)
    .bind(was_dirty)
    .execute(conn)
    .await?;
    Ok(())
}

/// Mark a page dirty directly (a new binding, a manual recompose request).
pub async fn mark_dirty(conn: &mut PgConnection, document_id: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE documents SET dirty_at = COALESCE(dirty_at, now()), updated_at = now()
         WHERE id = $1",
    )
    .bind(document_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub struct NewRevision {
    pub id: Uuid,
    pub document_id: Uuid,
    pub org_id: Uuid,
    pub content_md: String,
    pub composed_from: Vec<Uuid>,
    pub trigger: String,
    pub policy_decision: RevisionPolicy,
    /// The document's `updated_at` as the compose worker observed it when it
    /// claimed this page from the dirty list. Used as a compare-and-swap token:
    /// `dirty_at` is cleared ONLY if `updated_at` is unchanged since the claim, so
    /// a dependency-memory change that landed mid-compose (which bumps
    /// `updated_at` via `mark_dirty*`) leaves the page dirty for the next tick
    /// instead of being silently marked clean. `None` clears unconditionally (the
    /// pre-CAS behavior) for callers with no compose-window race — e.g. tests and
    /// direct one-shot writes.
    pub claimed_updated_at: Option<DateTime<Utc>>,
}

/// Write a revision and, when it auto-publishes, make it current.
///
/// The dependency index is rebuilt from `composed_from` in the SAME transaction:
/// a page's dirty-marking must never lag the content it was built from, or a
/// memory could change in the window and never mark the page that now cites it.
pub async fn insert_revision(conn: &mut PgConnection, r: &NewRevision) -> Result<()> {
    let published = r.policy_decision == RevisionPolicy::AutoPublished;
    sqlx::query(
        "INSERT INTO document_revisions
            (id, document_id, org_id, content_md, composed_from, trigger, policy_decision, published_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7, CASE WHEN $8 THEN now() ELSE NULL END)",
    )
    .bind(r.id)
    .bind(r.document_id)
    .bind(r.org_id)
    .bind(&r.content_md)
    .bind(serde_json::to_value(&r.composed_from)?)
    .bind(&r.trigger)
    .bind(r.policy_decision.as_str())
    .bind(published)
    .execute(&mut *conn)
    .await?;

    // Rebuild this page's dependency edges from the revision's provenance
    // closure. Delete-then-insert: a memory that no longer feeds the page must
    // stop marking it dirty, or the page would recompose forever over a memory
    // it dropped.
    sqlx::query("DELETE FROM document_dependencies WHERE document_id = $1")
        .bind(r.document_id)
        .execute(&mut *conn)
        .await?;
    for mid in &r.composed_from {
        sqlx::query(
            "INSERT INTO document_dependencies (document_id, memory_id, org_id)
             VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(r.document_id)
        .bind(mid)
        .bind(r.org_id)
        .execute(&mut *conn)
        .await?;
    }

    // The page is clean again — UNLESS a dependency memory changed during the
    // (multi-second, LLM-bound) compose window. `mark_dirty*` bumps `updated_at`
    // on every call, so if it no longer matches the value the worker claimed, a
    // change landed mid-compose and this freshly written revision is already
    // stale: keep the page dirty so the next tick recomposes it. Otherwise a
    // needs_review revision has been *produced*, so clearing avoids burning tokens
    // reproducing the same pending revision. current_revision/status still update
    // unconditionally — the revision was written and (if auto) published.
    sqlx::query(
        "UPDATE documents
         SET dirty_at = CASE
                 WHEN $4::timestamptz IS NULL THEN NULL
                 WHEN updated_at IS NOT DISTINCT FROM $4 THEN NULL
                 ELSE dirty_at
             END,
             updated_at = now(),
             -- A revision was produced, so any failure backoff (0021) is cleared:
             -- the page composed successfully and must not stay throttled.
             compose_attempts = 0,
             compose_next_at = NULL,
             current_revision = CASE WHEN $2 THEN $3 ELSE current_revision END,
             status = CASE WHEN $2 THEN 'published' ELSE status END
         WHERE id = $1",
    )
    .bind(r.document_id)
    .bind(published)
    .bind(r.id)
    .bind(r.claimed_updated_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn row_to_revision(r: &sqlx::postgres::PgRow) -> Result<DocumentRevision> {
    let composed: serde_json::Value = r.get("composed_from");
    Ok(DocumentRevision {
        id: r.get("id"),
        document_id: r.get("document_id"),
        content_md: r.get("content_md"),
        composed_from: serde_json::from_value(composed)?,
        trigger: r.get("trigger"),
        policy_decision: RevisionPolicy::parse(r.get::<String, _>("policy_decision").as_str())
            .unwrap_or(RevisionPolicy::NeedsReview),
        reviewed_by: r.get("reviewed_by"),
        published_at: r.get("published_at"),
        created_at: r.get("created_at"),
    })
}

const REV_COLUMNS: &str = "id, document_id, content_md, composed_from, trigger,
     policy_decision, reviewed_by, published_at, created_at";

pub async fn get_revision(conn: &mut PgConnection, id: Uuid) -> Result<Option<DocumentRevision>> {
    let row = sqlx::query(&format!(
        "SELECT {REV_COLUMNS} FROM document_revisions WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    row.as_ref().map(row_to_revision).transpose()
}

/// The currently published markdown for a page, if it has one.
pub async fn current_revision(
    conn: &mut PgConnection,
    document_id: Uuid,
) -> Result<Option<DocumentRevision>> {
    let row = sqlx::query(&format!(
        "SELECT {REV_COLUMNS} FROM document_revisions r
         WHERE r.id = (SELECT current_revision FROM documents WHERE id = $1)"
    ))
    .bind(document_id)
    .fetch_optional(conn)
    .await?;
    row.as_ref().map(row_to_revision).transpose()
}

pub async fn revisions(
    conn: &mut PgConnection,
    document_id: Uuid,
    limit: i64,
) -> Result<Vec<DocumentRevision>> {
    let rows = sqlx::query(&format!(
        "SELECT {REV_COLUMNS} FROM document_revisions
         WHERE document_id = $1 ORDER BY created_at DESC LIMIT $2"
    ))
    .bind(document_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    rows.iter().map(row_to_revision).collect()
}

/// Revisions a human must sign before they go live (KB2's review queue).
pub async fn pending_revisions(conn: &mut PgConnection) -> Result<Vec<DocumentRevision>> {
    let rows = sqlx::query(&format!(
        "SELECT {REV_COLUMNS} FROM document_revisions
         WHERE policy_decision = 'needs_review' AND reviewed_by IS NULL
         ORDER BY created_at"
    ))
    .fetch_all(conn)
    .await?;
    rows.iter().map(row_to_revision).collect()
}

/// A maintainer signs a pending revision: it becomes the page's current view.
/// Same gate as promotions — an agent proposed, a named human published.
pub async fn approve_revision(
    conn: &mut PgConnection,
    revision_id: Uuid,
    reviewer: Uuid,
    at: DateTime<Utc>,
) -> Result<bool> {
    // Read the target revision (existence + not-yet-reviewed gate) alongside the
    // page's CURRENT revision timestamp, and lock the document so a concurrent
    // auto-publish can't slip a newer revision in between this check and the
    // promotion below.
    let Some(row) = sqlx::query(
        "SELECT r.document_id, r.created_at AS rev_created, cur.created_at AS cur_created
         FROM document_revisions r
         JOIN documents d ON d.id = r.document_id
         LEFT JOIN document_revisions cur ON cur.id = d.current_revision
         WHERE r.id = $1 AND r.reviewed_by IS NULL
         FOR UPDATE OF d",
    )
    .bind(revision_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false); // gone, or already reviewed
    };
    let doc_id: Uuid = row.get("document_id");
    let rev_created: DateTime<Utc> = row.get("rev_created");
    let cur_created: Option<DateTime<Utc>> = row.get("cur_created");
    // Reject a backwards move: while this revision sat in review, a memory change
    // may have auto-published a NEWER revision as current. Approving the older one
    // would republish content built from since-superseded memories — a confident
    // republish of known-stale belief. Leave it pending so the UI can prompt a
    // recompose. (dirty_at is intentionally NOT cleared here: if the page is dirty
    // it must still recompose — clearing it would drop those pending changes.)
    if let Some(cur) = cur_created {
        if rev_created < cur {
            return Ok(false);
        }
    }
    sqlx::query(
        "UPDATE document_revisions
         SET reviewed_by = $2, published_at = $3, policy_decision = 'auto_published'
         WHERE id = $1 AND reviewed_by IS NULL",
    )
    .bind(revision_id)
    .bind(reviewer)
    .bind(at)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE documents SET current_revision = $2, status = 'published', updated_at = now()
         WHERE id = $1",
    )
    .bind(doc_id)
    .bind(revision_id)
    .execute(&mut *conn)
    .await?;
    Ok(true)
}
