//! The knowledge-base read surface (ARCHITECTURE.md §8.4; KB-PLAN KB2).
//!
//! Everything here runs under the caller's RLS transaction, so a reader can only
//! ever see a page their principal is entitled to — the same enforcement path as
//! `memory_search`, not a parallel one that could drift.
//!
//! The one design decision worth stating: `GET /v1/docs/{slug}` resolves the
//! revision's ENTIRE provenance closure in the same response. The reader's whole
//! reason to exist is that every sentence on the page is traceable to a governed
//! memory a named human signed; if checking a citation cost a round trip, nobody
//! would ever check one, and the guarantee would quietly become decorative.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{auth_of, internal, AppState, HttpError};

/// KB token scopes (KB-PLAN D6). The layer is optional and separately scoped, so
/// an agent's token can read the knowledge base without ever being able to
/// publish a page — and a token issued for the memory layer alone cannot read
/// pages at all. `admin` implies both (see AuthContext::allows).
pub const SCOPE_KB_READ: &str = "kb:read";
pub const SCOPE_KB_PUBLISH: &str = "kb:publish";

/// Record a page read (migration 0025) in its own transaction, warn-only on
/// failure. Runs AFTER the serving transaction commits: analytics must never
/// cost a reader their page, and a failed insert inside the serving tx would
/// poison its commit. Shared by the HTTP reader here and MCP `doc_get`.
pub(crate) async fn record_read(
    store: &brainiac_store::Store,
    principal: &brainiac_core::Principal,
    doc: &brainiac_core::Document,
    via: &str,
) {
    let outcome = async {
        let mut tx = store.scoped_tx(principal).await?;
        brainiac_store::documents::record_read(
            &mut tx,
            doc.org_id,
            doc.id,
            via,
            doc.dirty_at.is_some(),
        )
        .await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(e) = outcome {
        tracing::warn!(slug = %doc.slug, via, error = %e, "page read served but not recorded");
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocSummary {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub doc_kind: String,
    pub visibility: String,
    pub status: String,
    /// An underlying memory changed and the page has not recomposed yet. The
    /// honest signal that what you are reading may already be behind the corpus.
    pub dirty: bool,
    /// A revision is waiting on a human. Work, not decoration.
    pub pending_review: bool,
    pub updated_at: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocsListResponse {
    pub documents: Vec<DocSummary>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocRevisionView {
    pub id: Uuid,
    pub content_md: String,
    pub composed_from: Vec<Uuid>,
    pub policy_decision: String,
    pub published_at: Option<String>,
    pub created_at: String,
}

/// A memory a page's revision cites, resolved for the reader's provenance popover.
#[derive(Serialize, ToSchema)]
pub(crate) struct Citation {
    pub memory_id: Uuid,
    pub content: String,
    pub kind: String,
    /// shipped | in_flight | proposed — so the reader can mark a claim that
    /// describes intent rather than production (KB-PLAN D2).
    pub lifecycle: String,
    pub status: String,
    pub team: Option<String>,
    /// The artifact behind the claim, when there is one (KB-PLAN D3).
    pub detail_md: Option<String>,
}

/// A section as the reader needs to know it. Without this the console cannot
/// offer an editor at all: an edit needs the section's id, and there is nothing
/// in the rendered markdown to invent one from. It also carries `mode`, because
/// the editor must tell a human WHICH KIND of edit they are about to make —
/// their prose, or a proposal — before they type, not after.
#[derive(Serialize, ToSchema)]
pub(crate) struct DocSectionView {
    pub id: Uuid,
    pub heading: String,
    /// `composed` (a projection — edits become proposed knowledge) or `pinned`
    /// (the human's own prose — edits save).
    pub mode: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocDetailResponse {
    pub document: DocSummary,
    pub sections: Vec<DocSectionView>,
    /// The published view. `None` for a page whose first revision is still
    /// awaiting a human — nothing publishes itself into existence.
    pub revision: Option<DocRevisionView>,
    /// A revision awaiting review, if any.
    pub pending: Option<DocRevisionView>,
    pub citations: Vec<Citation>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocRevisionsResponse {
    pub revisions: Vec<DocRevisionView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DocApproveResponse {
    pub revision_id: Uuid,
    pub document_id: Uuid,
    pub published: bool,
}

fn view(r: &brainiac_core::DocumentRevision) -> DocRevisionView {
    DocRevisionView {
        id: r.id,
        content_md: r.content_md.clone(),
        composed_from: r.composed_from.clone(),
        policy_decision: r.policy_decision.as_str().to_string(),
        published_at: r.published_at.map(|t| t.to_rfc3339()),
        created_at: r.created_at.to_rfc3339(),
    }
}

fn summary(d: &brainiac_core::Document, pending_review: bool) -> DocSummary {
    DocSummary {
        id: d.id,
        slug: d.slug.clone(),
        title: d.title.clone(),
        doc_kind: d.doc_kind.as_str().to_string(),
        visibility: d.visibility.as_str().to_string(),
        status: d.status.as_str().to_string(),
        dirty: d.dirty_at.is_some(),
        pending_review,
        updated_at: d.updated_at.to_rfc3339(),
    }
}

#[utoipa::path(get, path = "/v1/docs", responses((status = 200, body = DocsListResponse)))]
pub(crate) async fn docs_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DocsListResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_KB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let docs = brainiac_store::documents::list_documents(&mut tx)
        .await
        .map_err(internal)?;
    let pending = brainiac_store::documents::pending_revisions(&mut tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    let documents = docs
        .iter()
        .map(|d| summary(d, pending.iter().any(|r| r.document_id == d.id)))
        .collect();
    Ok(Json(DocsListResponse { documents }))
}

#[utoipa::path(
    get,
    path = "/v1/docs/{slug}",
    params(("slug" = String, Path,)),
    responses((status = 200, body = DocDetailResponse), (status = 404))
)]
pub(crate) async fn doc_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<DocDetailResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_KB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    // RLS makes an unreadable page indistinguishable from a missing one, which
    // is the correct answer to give: existence is itself information.
    let doc = brainiac_store::documents::get_document_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "document not found".to_string()).into()
        })?;

    let current = brainiac_store::documents::current_revision(&mut tx, doc.id)
        .await
        .map_err(internal)?;
    let pending = brainiac_store::documents::pending_revisions(&mut tx)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|r| r.document_id == doc.id);

    // Resolve the provenance closure of whatever the reader will actually see.
    let shown = current.as_ref().or(pending.as_ref());
    let ids: Vec<Uuid> = shown.map(|r| r.composed_from.clone()).unwrap_or_default();
    let mut citations = Vec::new();
    if !ids.is_empty() {
        let rows = sqlx::query(
            "SELECT m.id, m.content, m.kind, m.lifecycle, m.status::text AS status,
                    m.detail_md, t.name AS team
             FROM memories m LEFT JOIN teams t ON t.id = m.team_id
             WHERE m.id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?;
        use sqlx::Row;
        // NB: RLS filters this join. If a memory in the closure is not readable
        // by THIS principal it simply does not come back — the page renders with
        // an unresolvable citation rather than leaking the memory. That is the
        // right failure: a citation the reader cannot open is a smaller harm
        // than one they should never have seen.
        citations = rows
            .iter()
            .map(|r| Citation {
                memory_id: r.get("id"),
                content: r.get("content"),
                kind: r.get("kind"),
                lifecycle: r.get("lifecycle"),
                status: r.get("status"),
                team: r.get("team"),
                detail_md: r.get("detail_md"),
            })
            .collect();
    }
    let sections = brainiac_store::documents::sections(&mut tx, doc.id)
        .await
        .map_err(internal)?
        .iter()
        .map(|s| DocSectionView {
            id: s.id,
            heading: s.heading.clone(),
            mode: s.mode.as_str().to_string(),
        })
        .collect();
    let served_content = shown.is_some();
    tx.commit().await.map_err(internal)?;

    // Read analytics (0025), only when revision content was actually served.
    // Its own transaction, after the read committed: a failed analytics insert
    // must cost a warning, never the read itself.
    if served_content {
        record_read(&state.store, &principal, &doc, "http").await;
    }

    Ok(Json(DocDetailResponse {
        document: summary(&doc, pending.is_some()),
        sections,
        revision: current.as_ref().map(view),
        pending: pending.as_ref().map(view),
        citations,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/docs/{slug}/revisions",
    params(("slug" = String, Path,)),
    responses((status = 200, body = DocRevisionsResponse), (status = 404))
)]
pub(crate) async fn doc_revisions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<DocRevisionsResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_KB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let doc = brainiac_store::documents::get_document_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "document not found".to_string()).into()
        })?;
    let revs = brainiac_store::documents::revisions(&mut tx, doc.id, 50)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(DocRevisionsResponse {
        revisions: revs.iter().map(view).collect(),
    }))
}

/// Publish a pending revision. The same gate as promotions: an agent composed
/// it; a NAMED HUMAN publishes it. Maintainer of the owning team only — and for
/// an org-wide page with no owning team, any maintainer in the org, since no
/// single team owns the org's shared view.
#[utoipa::path(
    post,
    path = "/v1/docs/revisions/{id}/approve",
    params(("id" = Uuid, Path,)),
    responses((status = 200, body = DocApproveResponse), (status = 403), (status = 404))
)]
pub(crate) async fn doc_approve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<DocApproveResponse>, HttpError> {
    // Publishing a revision is the act that puts words in the org's mouth (and,
    // once a target is configured, into its wiki). It takes `kb:publish` — a
    // token minted to READ the knowledge base must not be able to sign one.
    let principal = auth_of(&state, &headers, SCOPE_KB_PUBLISH).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let rev = brainiac_store::documents::get_revision(&mut tx, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "revision not found".to_string()).into()
        })?;
    let doc = brainiac_store::documents::get_document(&mut tx, rev.document_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "document not found".to_string()).into()
        })?;

    let allowed = match doc.team_id {
        Some(team) => crate::console::is_maintainer(&mut tx, &principal, team).await?,
        None => is_any_maintainer(&mut tx, &principal).await?,
    };
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "only a maintainer of the owning team may publish a page revision".to_string(),
        )
            .into());
    }

    let published = brainiac_store::documents::approve_revision(
        &mut tx,
        id,
        principal.user_id,
        chrono::Utc::now(),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(DocApproveResponse {
        revision_id: id,
        document_id: doc.id,
        published,
    }))
}

// ── KB4: a human edits a page, and the truth does not fork ──────────────

#[derive(serde::Deserialize, ToSchema)]
pub(crate) struct EditSectionBody {
    pub section_id: Uuid,
    /// The section as the human now wants it to read.
    pub content: String,
    /// Optional: why. Carried into the extraction source, because "why" is
    /// exactly the knowledge a diff cannot recover.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct EditSectionResponse {
    /// `saved` (pinned prose — the human owns it) or `captured` (composed
    /// section — the edit became proposed knowledge).
    pub outcome: String,
    /// For a composed edit: the ingest source the edit became.
    pub source_id: Option<Uuid>,
    pub job_id: Option<i64>,
    /// What to tell the editor. The wording matters more than it looks — see
    /// the handler.
    pub message: String,
}

/// Edit a section of a page.
///
/// This endpoint is where the document layer's central asymmetry becomes a
/// product experience rather than an architecture diagram (KB-PLAN D1):
///
/// - A **pinned** section is human-owned prose. It saves. Done.
/// - A **composed** section is a *projection of memories*. Saving the edited
///   text into the page would fork the truth: the page would say one thing, the
///   memory layer another, and the next recompose would silently revert the
///   human — the single most infuriating behaviour a wiki can have. So the edit
///   is sent through the EXTRACTION pipeline instead. It becomes candidate
///   memories, passes the same review gate as everything else, and the section
///   regenerates once they land.
///
/// The editor is told exactly that: "your change was captured as proposed
/// knowledge". Not "saved" — because it wasn't, and a tool that says "saved"
/// when it means "queued for someone else's approval" has lied to the person
/// most likely to notice.
#[utoipa::path(
    post,
    path = "/v1/docs/{slug}/edit",
    params(("slug" = String, Path,)),
    request_body = EditSectionBody,
    responses((status = 200, body = EditSectionResponse), (status = 404))
)]
pub(crate) async fn doc_edit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(body): Json<EditSectionBody>,
) -> Result<Json<EditSectionResponse>, HttpError> {
    // Editing a page mutates org-visible state: a pinned edit auto-publishes into
    // the live markdown on the next compose (no review), and a composed edit is
    // injected into the extraction pipeline framed as a maintainer's belief. That
    // is a write, and must not be reachable with a read-only token — the same bar
    // doc_approve sets ("a token minted to READ the KB must not be able to sign
    // one"). RLS visibility (can-see) is not authorization (can-mutate).
    let principal = auth_of(&state, &headers, SCOPE_KB_PUBLISH).await?.principal;
    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "an empty edit is a deletion; delete the section instead".to_string(),
        )
            .into());
    }

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let doc = brainiac_store::documents::get_document_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "document not found".to_string()).into()
        })?;
    // Same maintainer gate as doc_approve: only a maintainer of the owning team
    // (or any org maintainer for an org-wide page) may edit a section. Without
    // this, a read-scoped agent token could rewrite published pinned prose and
    // inject "a maintainer edited …"-framed text into extraction.
    let allowed = match doc.team_id {
        Some(team) => crate::console::is_maintainer(&mut tx, &principal, team).await?,
        None => is_any_maintainer(&mut tx, &principal).await?,
    };
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "only a maintainer of the owning team may edit a page section".to_string(),
        )
            .into());
    }
    let sections = brainiac_store::documents::sections(&mut tx, doc.id)
        .await
        .map_err(internal)?;
    let section = sections
        .iter()
        .find(|s| s.id == body.section_id)
        .ok_or_else(|| -> HttpError {
            (StatusCode::NOT_FOUND, "section not found".to_string()).into()
        })?;

    if section.mode == brainiac_core::SectionMode::Pinned {
        // Optimistic concurrency: only save if the stored prose still matches what
        // we read, so a second editor of the same section is told (409) instead of
        // silently overwriting the first's save.
        let saved = brainiac_store::documents::update_pinned(
            &mut tx,
            section.id,
            &content,
            section.pinned_content.as_deref(),
        )
        .await
        .map_err(internal)?;
        if !saved {
            return Err((
                StatusCode::CONFLICT,
                "this section changed since you loaded it — reload and reapply your edit"
                    .to_string(),
            )
                .into());
        }
        // The page must recompose so the published markdown carries the new
        // prose — a pinned edit that never reaches a revision is invisible.
        brainiac_store::documents::mark_dirty(&mut tx, doc.id)
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        return Ok(Json(EditSectionResponse {
            outcome: "saved".into(),
            source_id: None,
            job_id: None,
            message: "Saved. This section is yours — regeneration never touches it.".into(),
        }));
    }

    // Composed: the edit is knowledge, not prose. Send it where knowledge goes.
    //
    // The source text is framed so the extractor sees a claim about the world
    // rather than a diff: it is being asked "what does this person now believe?",
    // which is the same question it answers for a transcript.
    let source_id = Uuid::new_v4();
    let raw = format!(
        "A maintainer edited the \"{}\" section of the knowledge-base page \"{}\".\n\
         They now state:\n\n{}\n\n{}",
        section.heading,
        doc.title,
        content,
        body.note
            .as_deref()
            .map(|n| format!("Their reason: {n}"))
            .unwrap_or_default()
    );
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        principal.org_id,
        doc.team_id,
        "doc",
        &raw,
        Some(principal.user_id),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    let job_id =
        brainiac_pipeline::worker::enqueue_source(&state.store, principal.org_id, source_id)
            .await
            .map_err(internal)?;

    Ok(Json(EditSectionResponse {
        outcome: "captured".into(),
        source_id: Some(source_id),
        job_id: Some(job_id),
        message: "Captured as proposed knowledge. This section is compiled from the org's \
                  memories, so your edit goes through the same review gate as everything else — \
                  once it is approved, the section will say so on its own."
            .into(),
    }))
}

use crate::console::is_any_maintainer;
