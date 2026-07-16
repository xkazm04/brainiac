//! The Library distribution surface (docs/LIBRARY-PLAN.md LB1).
//!
//! Read side first, deliberately: agents fetch adopted rules by stack and
//! published skill bundles by slug, and usage flows back — per team, never per
//! person (the events table cannot even store a person). The write side here
//! is only the maintainer gate: ratify a divergence, adopt, deprecate. There
//! is no propose endpoint yet — `lib:propose` stays unminted until the intake
//! phase (LB4) has a dedup corpus to check against.
//!
//! Everything runs under the caller's RLS transaction, so another org's rules
//! are "not found", never "forbidden" — existence is itself information.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use brainiac_core::{LibraryArtifactKind, LibraryUsageEvent, Skill, Standard, StandardLifecycle};
use brainiac_store::library;

use crate::http::{auth_of, internal, AppState, HttpError};

/// Library token scopes (LIBRARY-PLAN L7). `lib:read` is what an agent's token
/// carries; `lib:propose` lets it submit candidates (rate-limited, deduped —
/// LB4); `lib:publish` (adopt/reject/deprecate/ratify) is the maintainer
/// scope — a token minted to read standards must not be able to decree one.
/// `admin` implies all (see AuthContext::allows).
pub const SCOPE_LIB_READ: &str = "lib:read";
pub const SCOPE_LIB_PROPOSE: &str = "lib:propose";
pub const SCOPE_LIB_PUBLISH: &str = "lib:publish";

/// Per-author proposals per hour (BRAINIAC_LIB_PROPOSE_PER_HOUR).
fn propose_per_hour() -> i64 {
    std::env::var("BRAINIAC_LIB_PROPOSE_PER_HOUR")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(library::DEFAULT_PROPOSE_PER_HOUR)
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/library/standards", get(standards_list))
        .route("/v1/library/standards/propose", post(standard_propose))
        .route("/v1/library/standards/{id}", get(standard_get))
        .route("/v1/library/standards/{id}/adopt", post(standard_adopt))
        .route(
            "/v1/library/standards/{id}/deprecate",
            post(standard_deprecate),
        )
        .route("/v1/library/standards/{id}/reject", post(standard_reject))
        .route(
            "/v1/library/divergences/{id}/ratify",
            post(divergence_ratify),
        )
        .route("/v1/library/skills", get(skills_list))
        .route("/v1/library/skills/{slug}", get(skill_get))
        .route("/v1/library/skills/{slug}/download", get(skill_download))
        .route("/v1/library/usage", post(usage_record))
}

// ── views ────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub(crate) struct StandardView {
    pub id: Uuid,
    /// Who created it: `human` (console/ratify), `sweep` (mining), `agent`
    /// (a mid-session proposal). Triage renders this — trust needs a source.
    pub origin: String,
    pub stack: String,
    pub category: String,
    pub slug: String,
    pub statement: String,
    pub rationale: Option<String>,
    /// Good/bad examples — served verbatim, never re-typed by a model.
    pub detail_md: Option<String>,
    pub enforcement: String,
    pub lifecycle: String,
    pub adopted_at: Option<String>,
    /// Set only for an evidence-free rule a named human signed for.
    pub decreed: bool,
}

fn standard_view(s: &Standard) -> StandardView {
    StandardView {
        id: s.id,
        origin: s.origin.as_str().to_string(),
        stack: s.stack.clone(),
        category: s.category.clone(),
        slug: s.slug.clone(),
        statement: s.statement.clone(),
        rationale: s.rationale.clone(),
        detail_md: s.detail_md.clone(),
        enforcement: s.enforcement.as_str().to_string(),
        lifecycle: s.lifecycle.as_str().to_string(),
        adopted_at: s.adopted_at.map(|t| t.to_rfc3339()),
        decreed: s.decreed_by.is_some(),
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StandardsListResponse {
    pub standards: Vec<StandardView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProvenanceView {
    /// `memory` or `divergence` — the evidence class behind the rule.
    pub kind: String,
    pub ref_id: Uuid,
}

/// Usage totals for one artifact, per team. There is deliberately no finer
/// grain to ask for — the storage cannot name a person.
#[derive(Serialize, ToSchema)]
pub(crate) struct TeamUsageView {
    /// Team name; `null` groups events from org-scoped (teamless) tokens.
    pub team: Option<String>,
    pub uses: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StandardVersionView {
    pub rev: i32,
    pub statement: String,
    pub enforcement: String,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StandardDetailResponse {
    #[serde(flatten)]
    pub standard: StandardView,
    /// Why this rule exists. Empty only for a decreed rule.
    pub provenance: Vec<ProvenanceView>,
    /// The rule's pulse: fetches/checks per team.
    pub usage: Vec<TeamUsageView>,
    /// Version history, newest first.
    pub versions: Vec<StandardVersionView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillView {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub maturity: String,
    /// Whether a published version exists to download. A draft-only skill is
    /// listed (it exists) but serves no bundle.
    pub downloadable: bool,
}

fn skill_view(s: &Skill) -> SkillView {
    SkillView {
        id: s.id,
        slug: s.slug.clone(),
        name: s.name.clone(),
        description: s.description.clone(),
        domain: s.domain.clone(),
        maturity: s.maturity.as_str().to_string(),
        downloadable: s.current_version.is_some(),
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillsListResponse {
    pub skills: Vec<SkillView>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillBundleResponse {
    pub slug: String,
    pub name: String,
    pub semver: String,
    /// The open agent-skill bundle: manifest front-matter, markdown body,
    /// auxiliary resources — exactly as the named human published it.
    pub manifest: serde_json::Value,
    pub content_md: String,
    pub resources: serde_json::Value,
    pub published_at: Option<String>,
}

// ── the read surface (lib:read) ──────────────────────────────────────────

#[derive(Deserialize, utoipa::IntoParams)]
pub(crate) struct StandardsQuery {
    /// Narrow to one tech stack (e.g. `rust`, `typescript`, `general`).
    pub stack: Option<String>,
    /// `adopted` (default — what an agent should follow), `proposed`,
    /// `deprecated`, or `all`.
    pub lifecycle: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/library/standards",
    params(StandardsQuery),
    description = "The org's coding standards, adopted rules by default — the set an agent should follow for a stack. RLS-scoped.",
    responses((status = 200, body = StandardsListResponse))
)]
pub(crate) async fn standards_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<StandardsQuery>,
) -> Result<Json<StandardsListResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;

    let lifecycle = match q.lifecycle.as_deref() {
        None => Some(StandardLifecycle::Adopted),
        Some("all") => None,
        Some(other) => Some(StandardLifecycle::parse(other).ok_or_else(|| {
            HttpError::from((
                StatusCode::BAD_REQUEST,
                "lifecycle must be proposed | adopted | deprecated | all".to_string(),
            ))
        })?),
    };

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let standards = library::list_standards(&mut tx, q.stack.as_deref(), lifecycle)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(StandardsListResponse {
        standards: standards.iter().map(standard_view).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/library/standards/{id}",
    params(("id" = Uuid, Path,)),
    description = "One rule with the provenance behind it — the memories, incidents, and divergences that motivated it, or the mark of the human who decreed it.",
    responses((status = 200, body = StandardDetailResponse), (status = 404))
)]
pub(crate) async fn standard_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<StandardDetailResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let Some(s) = library::get_standard(&mut tx, id).await.map_err(internal)? else {
        return Err((StatusCode::NOT_FOUND, "standard not found".to_string()).into());
    };
    let provenance = library::provenance(&mut tx, id).await.map_err(internal)?;
    let usage = library::usage_named(&mut tx, LibraryArtifactKind::Standard, id)
        .await
        .map_err(internal)?;
    let versions = library::versions(&mut tx, id).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(StandardDetailResponse {
        standard: standard_view(&s),
        provenance: provenance
            .iter()
            .map(|p| ProvenanceView {
                kind: p.kind.as_str().to_string(),
                ref_id: p.ref_id,
            })
            .collect(),
        usage: usage
            .into_iter()
            .map(|(team, uses)| TeamUsageView { team, uses })
            .collect(),
        versions: versions
            .iter()
            .map(|v| StandardVersionView {
                rev: v.rev,
                statement: v.statement.clone(),
                enforcement: v.enforcement.clone(),
                created_at: v.created_at.to_rfc3339(),
            })
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/library/skills",
    description = "The org's skill catalog. Every skill is listed; only versions a named human published can be downloaded.",
    responses((status = 200, body = SkillsListResponse))
)]
pub(crate) async fn skills_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SkillsListResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let skills = library::list_skills(&mut tx).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(SkillsListResponse {
        skills: skills.iter().map(skill_view).collect(),
    }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillVersionView {
    pub semver: String,
    /// A version nobody signed is a draft; it is listed here for maintainers
    /// but never served as content.
    pub published: bool,
    pub published_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillDetailResponse {
    #[serde(flatten)]
    pub skill: SkillView,
    /// Version history, newest first — drafts included, marked as such.
    pub versions: Vec<SkillVersionView>,
    /// The skill's pulse: fetches/applies per team.
    pub usage: Vec<TeamUsageView>,
}

#[utoipa::path(
    get,
    path = "/v1/library/skills/{slug}",
    params(("slug" = String, Path,)),
    responses((status = 200, body = SkillDetailResponse), (status = 404))
)]
pub(crate) async fn skill_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<SkillDetailResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let Some(s) = library::get_skill_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "skill not found".to_string()).into());
    };
    let versions = library::versions_of(&mut tx, s.id)
        .await
        .map_err(internal)?;
    let usage = library::usage_named(&mut tx, LibraryArtifactKind::Skill, s.id)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(SkillDetailResponse {
        skill: skill_view(&s),
        versions: versions
            .iter()
            .map(|v| SkillVersionView {
                semver: v.semver.clone(),
                published: v.published_by.is_some(),
                published_at: v.published_at.map(|t| t.to_rfc3339()),
                created_at: v.created_at.to_rfc3339(),
            })
            .collect(),
        usage: usage
            .into_iter()
            .map(|(team, uses)| TeamUsageView { team, uses })
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/library/skills/{slug}/download",
    params(("slug" = String, Path,)),
    description = "The current PUBLISHED bundle of a skill. A draft nobody signed is not served — the response is 404, exactly as if no version existed. Downloading records a fetch event, counted by team.",
    responses((status = 200, body = SkillBundleResponse), (status = 404))
)]
pub(crate) async fn skill_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<SkillBundleResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let Some(skill) = library::get_skill_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "skill not found".to_string()).into());
    };
    let Some(version) = library::current_published_version(&mut tx, skill.id)
        .await
        .map_err(internal)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            "no published version of this skill".to_string(),
        )
            .into());
    };
    // The fetch is the vital sign (L5): recorded in the serving transaction,
    // attributed to the caller's team — the schema cannot store a person.
    library::record_usage(
        &mut tx,
        principal.org_id,
        LibraryArtifactKind::Skill,
        skill.id,
        Some(&version.semver),
        LibraryUsageEvent::Fetch,
        principal.team_ids.first().copied(),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(SkillBundleResponse {
        slug: skill.slug,
        name: skill.name,
        semver: version.semver,
        manifest: version.manifest,
        content_md: version.content_md,
        resources: version.resources,
        published_at: version.published_at.map(|t| t.to_rfc3339()),
    }))
}

// ── telemetry (lib:read) ─────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct UsageRequest {
    /// `standard` or `skill`.
    pub artifact_kind: String,
    pub artifact_id: Uuid,
    pub version: Option<String>,
    /// `fetch`, `check` (compared work against a standard), or `apply` (ran a skill).
    pub event: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct UsageResponse {
    pub recorded: bool,
}

#[utoipa::path(
    post,
    path = "/v1/library/usage",
    request_body = UsageRequest,
    description = "Report a usage signal. Attributed to the caller's team — never to a person; the events table has no user column to fill.",
    responses((status = 200, body = UsageResponse), (status = 400))
)]
pub(crate) async fn usage_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<UsageRequest>,
) -> Result<Json<UsageResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_READ).await?.principal;
    let bad = |m: &str| HttpError::from((StatusCode::BAD_REQUEST, m.to_string()));
    let kind = LibraryArtifactKind::parse(&req.artifact_kind)
        .ok_or_else(|| bad("artifact_kind must be standard | skill"))?;
    let event = LibraryUsageEvent::parse(&req.event)
        .ok_or_else(|| bad("event must be fetch | check | apply"))?;

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    library::record_usage(
        &mut tx,
        principal.org_id,
        kind,
        req.artifact_id,
        req.version.as_deref(),
        event,
        principal.team_ids.first().copied(),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(UsageResponse { recorded: true }))
}

// ── agent proposals (lib:propose, LB4) ───────────────────────────────────

/// Input caps: a proposal is a candidate RULE, not an essay. Anything longer
/// belongs in a memory the proposal can cite as evidence.
const MAX_PROPOSAL_NAME: usize = 120;
const MAX_PROPOSAL_STATEMENT: usize = 500;
const MAX_PROPOSAL_RATIONALE: usize = 2_000;
const MAX_PROPOSAL_EXAMPLES: usize = 4_000;

#[derive(Deserialize, ToSchema)]
pub(crate) struct ProposeRequest {
    /// Short practice name (the dedup key), e.g. "service retry policy".
    pub name: String,
    /// The rule, in one sentence.
    pub statement: String,
    pub stack: Option<String>,
    pub category: Option<String>,
    pub rationale: Option<String>,
    /// Good/bad examples, verbatim markdown.
    pub examples_md: Option<String>,
    /// A memory backing the proposal. Optional — but without evidence the
    /// rule can only ever be adopted by an explicit signed decree.
    pub evidence_memory_id: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProposeResponse {
    /// `created` (a fresh candidate waits at the gate) or `duplicate`
    /// (collapsed onto an existing standard — see `lifecycle` for what the
    /// org already decided about this idea).
    pub outcome: String,
    pub standard_id: Uuid,
    pub lifecycle: String,
}

#[utoipa::path(
    post,
    path = "/v1/library/standards/propose",
    request_body = ProposeRequest,
    description = "Propose a standard candidate (lib:propose). The outcome is only ever a PROPOSED candidate — the gate stays human. Deduplicated against the whole corpus (a duplicate collapses onto the existing standard, whatever its lifecycle) and rate-limited per author.",
    responses(
        (status = 200, body = ProposeResponse),
        (status = 404, description = "cited evidence memory not found"),
        (status = 429, description = "per-author hourly proposal budget spent"),
    )
)]
pub(crate) async fn standard_propose(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ProposeRequest>,
) -> Result<Json<ProposeResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PROPOSE)
        .await?
        .principal;
    let bad = |m: String| HttpError::from((StatusCode::BAD_REQUEST, m));
    let cap = |v: &str, max: usize, field: &str| {
        if v.trim().is_empty() {
            Err(bad(format!("{field} must not be empty")))
        } else if v.chars().count() > max {
            Err(bad(format!("{field} exceeds {max} chars")))
        } else {
            Ok(())
        }
    };
    cap(&req.name, MAX_PROPOSAL_NAME, "name")?;
    cap(&req.statement, MAX_PROPOSAL_STATEMENT, "statement")?;
    if let Some(r) = &req.rationale {
        cap(r, MAX_PROPOSAL_RATIONALE, "rationale")?;
    }
    if let Some(e) = &req.examples_md {
        cap(e, MAX_PROPOSAL_EXAMPLES, "examples_md")?;
    }

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let outcome = library::propose_standard(
        &mut tx,
        &library::Proposal {
            org_id: principal.org_id,
            author: principal.user_id,
            name: req.name,
            statement: req.statement,
            stack: req.stack,
            category: req.category,
            rationale: req.rationale,
            detail_md: req.examples_md,
            evidence_memory_id: req.evidence_memory_id,
        },
        propose_per_hour(),
    )
    .await
    .map_err(internal)?;

    match outcome {
        library::ProposeOutcome::Created(id) => {
            tx.commit().await.map_err(internal)?;
            Ok(Json(ProposeResponse {
                outcome: "created".into(),
                standard_id: id,
                lifecycle: "proposed".into(),
            }))
        }
        library::ProposeOutcome::Duplicate {
            standard_id,
            lifecycle,
        } => Ok(Json(ProposeResponse {
            outcome: "duplicate".into(),
            standard_id,
            lifecycle: lifecycle.as_str().into(),
        })),
        library::ProposeOutcome::RateLimited { per_hour } => Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("proposal budget spent: {per_hour} per author per hour — the sixth idea can wait sixty minutes"),
        )
            .into()),
        library::ProposeOutcome::EvidenceNotFound => Err((
            StatusCode::NOT_FOUND,
            "cited evidence memory not found".to_string(),
        )
            .into()),
    }
}

// ── the maintainer gate (lib:publish) ────────────────────────────────────

#[derive(Deserialize, ToSchema, Default)]
pub(crate) struct AdoptRequest {
    /// Adopt WITHOUT evidence, signing for it by name. Absent a decree, a rule
    /// with no provenance is refused — by the database, not this handler.
    #[serde(default)]
    pub decree: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AdoptResponse {
    pub adopted: bool,
}

#[utoipa::path(
    post,
    path = "/v1/library/standards/{id}/adopt",
    params(("id" = Uuid, Path,)),
    request_body = AdoptRequest,
    description = "Adopt a proposed standard — the named-human gate. Takes lib:publish. Refused (409) when the rule has neither provenance nor a decree.",
    responses((status = 200, body = AdoptResponse), (status = 404), (status = 409))
)]
pub(crate) async fn standard_adopt(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    body: Option<Json<AdoptRequest>>,
) -> Result<Json<AdoptResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let decree = body.map(|Json(b)| b.decree).unwrap_or(false);

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    match library::adopt_standard(&mut tx, id, principal.user_id, decree).await {
        Ok(true) => {
            tx.commit().await.map_err(internal)?;
            Ok(Json(AdoptResponse { adopted: true }))
        }
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            "standard not found or not proposed".to_string(),
        )
            .into()),
        // The schema's attribution refusal: surface it as a conflict the
        // maintainer can act on, not a 500.
        Err(e)
            if e.to_string()
                .contains("without provenance or a named decree") =>
        {
            Err((
                StatusCode::CONFLICT,
                "this rule has no provenance; adopt with decree=true to sign for it by name"
                    .to_string(),
            )
                .into())
        }
        Err(e) => Err(internal(e)),
    }
}

#[utoipa::path(
    post,
    path = "/v1/library/standards/{id}/deprecate",
    params(("id" = Uuid, Path,)),
    description = "Retire an adopted standard, in the open. Takes lib:publish.",
    responses((status = 200, body = AdoptResponse), (status = 404))
)]
pub(crate) async fn standard_deprecate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AdoptResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let done = library::deprecate_standard(&mut tx, id, principal.user_id)
        .await
        .map_err(internal)?;
    if !done {
        return Err((
            StatusCode::NOT_FOUND,
            "standard not found or not adopted".to_string(),
        )
            .into());
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(AdoptResponse { adopted: false }))
}

#[utoipa::path(
    post,
    path = "/v1/library/standards/{id}/reject",
    params(("id" = Uuid, Path,)),
    description = "Reject a proposed candidate — kept, not deleted: the mining sweep dedups against rejections for the dedup window, so a maintainer who said no is not asked again next week. Takes lib:publish.",
    responses((status = 200, body = AdoptResponse), (status = 404))
)]
pub(crate) async fn standard_reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AdoptResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let done = library::reject_standard(&mut tx, id, principal.user_id)
        .await
        .map_err(internal)?;
    if !done {
        return Err((
            StatusCode::NOT_FOUND,
            "standard not found or not proposed".to_string(),
        )
            .into());
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(AdoptResponse { adopted: false }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RatifyResponse {
    /// The standard candidate carrying this divergence as provenance. Stable:
    /// ratifying the same divergence again returns the same candidate.
    pub standard_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/library/divergences/{id}/ratify",
    params(("id" = Uuid, Path,)),
    description = "The L6 bridge: turn a detected practice divergence into a proposed standard candidate carrying the divergence as provenance. Idempotent. Takes lib:publish.",
    responses((status = 200, body = RatifyResponse), (status = 404))
)]
pub(crate) async fn divergence_ratify(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<RatifyResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let Some(standard_id) = library::ratify_divergence(&mut tx, id, principal.user_id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "divergence not found".to_string()).into());
    };
    tx.commit().await.map_err(internal)?;
    Ok(Json(RatifyResponse { standard_id }))
}
