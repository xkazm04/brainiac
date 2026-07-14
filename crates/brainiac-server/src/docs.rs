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

use crate::http::{internal, principal_of, AppState, HttpError};

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

#[derive(Serialize, ToSchema)]
pub(crate) struct DocDetailResponse {
    pub document: DocSummary,
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
    let principal = principal_of(&state, &headers).await?;
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
    let principal = principal_of(&state, &headers).await?;
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
    tx.commit().await.map_err(internal)?;

    Ok(Json(DocDetailResponse {
        document: summary(&doc, pending.is_some()),
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
    let principal = principal_of(&state, &headers).await?;
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
    let principal = principal_of(&state, &headers).await?;
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

async fn is_any_maintainer(
    conn: &mut sqlx::PgConnection,
    principal: &brainiac_core::Principal,
) -> Result<bool, HttpError> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT 1 AS ok FROM team_members WHERE user_id = $1 AND role = 'maintainer' LIMIT 1",
    )
    .bind(principal.user_id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?;
    Ok(row.map(|r| r.get::<i32, _>("ok") == 1).unwrap_or(false))
}
