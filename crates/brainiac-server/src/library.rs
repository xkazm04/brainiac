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

use brainiac_core::{
    Enforcement, LibraryArtifactKind, LibraryUsageEvent, Skill, Standard, StandardLifecycle,
};
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
        .route(
            "/v1/library/standards/{id}",
            get(standard_get).patch(standard_update),
        )
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
        .route("/v1/library/skills/propose", post(skill_propose))
        .route("/v1/library/skills/{slug}", get(skill_get))
        .route("/v1/library/skills/{slug}/download", get(skill_download))
        .route("/v1/library/skills/{slug}/publish", post(skill_publish))
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
    /// THE PULSE: every recorded use of this rule, all time, across teams.
    /// On a `proposed` candidate this is adoption pressure — the org is
    /// already reaching for a rule nobody has ratified yet, which makes it the
    /// highest-leverage thing on the board.
    pub uses: i64,
    /// Uses inside the dormancy window — the same window Knowledge Health uses.
    pub recent_uses: i64,
    pub last_used_at: Option<String>,
    /// An adopted rule old enough to have been used and untouched for the
    /// whole window: a deprecation candidate that surfaced itself. Never set
    /// on a proposal — a rule nobody has adopted cannot be being ignored.
    pub dormant: bool,
}

/// The dormancy verdict for one rule. The rule itself is `core`'s
/// (`rule_is_dormant`); all this does is feed it the two numbers, and refuse
/// to ask the question of a rule that has not passed the gate.
fn is_dormant(s: &Standard, uses_in_window: i64) -> bool {
    if s.lifecycle != StandardLifecycle::Adopted {
        return false;
    }
    match s.adopted_at {
        Some(at) => brainiac_core::health::rule_is_dormant(
            (chrono::Utc::now() - at).num_seconds(),
            uses_in_window,
        ),
        None => false,
    }
}

fn standard_view(s: &Standard, pulse: Option<&library::StandardPulse>) -> StandardView {
    let uses = pulse.map(|p| p.uses).unwrap_or(0);
    let recent_uses = pulse.map(|p| p.uses_in_window).unwrap_or(0);
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
        uses,
        recent_uses,
        last_used_at: pulse.and_then(|p| p.last_used_at).map(|t| t.to_rfc3339()),
        dormant: is_dormant(s, recent_uses),
    }
}

/// Fetch the pulse for one rule (`Some(id)`) or the whole shelf (`None`) and
/// index it. Rules with no events never appear in the result; the view reads
/// their absence as a pulse of zero, which is exactly what it is.
async fn pulse_index(
    conn: &mut sqlx::PgConnection,
    only: Option<Uuid>,
) -> anyhow::Result<std::collections::HashMap<Uuid, library::StandardPulse>> {
    Ok(
        library::standards_pulse(conn, brainiac_core::health::LIBRARY_DORMANT_DAYS, only)
            .await?
            .into_iter()
            .map(|p| (p.standard_id, p))
            .collect(),
    )
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
    // The board ranks by pressure, so the pulse travels WITH the shelf: one
    // grouped read of what the serve path already recorded, not N detail
    // round-trips and not a second telemetry stream.
    let pulse = pulse_index(&mut tx, None).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(StandardsListResponse {
        standards: standards
            .iter()
            .map(|s| standard_view(s, pulse.get(&s.id)))
            .collect(),
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
    let pulse = pulse_index(&mut tx, Some(id)).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(StandardDetailResponse {
        standard: standard_view(&s, pulse.get(&id)),
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

/// The result of signing a draft. `newly_published` distinguishes the act from
/// its replay: a second call is deliberately NOT an error (a console double
/// click, a retried request), it just reports the version that is already
/// serving and mints nothing new.
#[derive(Serialize, ToSchema)]
pub(crate) struct SkillPublishResponse {
    pub slug: String,
    /// The version now serving — the draft this call signed, or the one
    /// already published when the call was a replay.
    pub semver: String,
    pub newly_published: bool,
    pub published_at: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/library/skills/{slug}/publish",
    params(("slug" = String, Path,)),
    description = "Publish a skill's pending draft version — the named-human gate that makes a proposed bundle servable. Takes lib:publish; agents propose, humans publish, and there is deliberately no MCP tool for this. Idempotent: publishing a skill whose draft is already signed reports the serving version and creates nothing. 404 when the skill has no version awaiting a signature.",
    responses((status = 200, body = SkillPublishResponse), (status = 404))
)]
pub(crate) async fn skill_publish(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<SkillPublishResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;

    let Some(skill) = library::get_skill_by_slug(&mut tx, &slug)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "skill not found".to_string()).into());
    };

    // A draft waiting for a name is the normal case; without one, the only
    // non-error outcome is the idempotent replay — a version that is already
    // serving. Anything else is a 404, same shape as the standards gate.
    if let Some(draft) = library::pending_draft_version(&mut tx, skill.id)
        .await
        .map_err(internal)?
    {
        let published = library::publish_skill_version(&mut tx, draft.id, principal.user_id)
            .await
            .map_err(internal)?;
        if !published {
            return Err(internal(anyhow::anyhow!(
                "draft version {} was signed concurrently",
                draft.id
            )));
        }
        // Re-read so the timestamp reported is the one the database stamped.
        let current = library::current_published_version(&mut tx, skill.id)
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        return Ok(Json(SkillPublishResponse {
            slug: skill.slug,
            semver: draft.semver,
            newly_published: true,
            published_at: current.and_then(|v| v.published_at).map(|t| t.to_rfc3339()),
        }));
    }

    let Some(current) = library::current_published_version(&mut tx, skill.id)
        .await
        .map_err(internal)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            "no draft version of this skill to publish".to_string(),
        )
            .into());
    };
    tx.commit().await.map_err(internal)?;
    Ok(Json(SkillPublishResponse {
        slug: skill.slug,
        semver: current.semver,
        newly_published: false,
        published_at: current.published_at.map(|t| t.to_rfc3339()),
    }))
}

// ── telemetry (lib:read) ─────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct UsageRequest {
    /// `standard` or `skill`.
    pub artifact_kind: String,
    /// The artifact's UUID. Optional if `artifact_slug` is given — an agent
    /// holding a slug (what `standards_for` / `skill_fetch` hand back) should
    /// not have to look up the id first, so this mirrors the MCP tool, which
    /// takes a slug. Exactly one of `artifact_id` / `artifact_slug` is required.
    #[serde(default)]
    pub artifact_id: Option<Uuid>,
    /// The artifact's slug, as an alternative to `artifact_id`.
    #[serde(default)]
    pub artifact_slug: Option<String>,
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

    // Resolve the artifact by id or slug — exactly one. A slug that resolves to
    // nothing (or is hidden by RLS) is a clean not-recorded, never a 500.
    let artifact_id = match (req.artifact_id, req.artifact_slug.as_deref()) {
        (Some(id), None) => id,
        (None, Some(slug)) => {
            let resolved = match kind {
                LibraryArtifactKind::Standard => library::get_standard_by_slug(&mut tx, slug)
                    .await
                    .map_err(internal)?
                    .map(|s| s.id),
                LibraryArtifactKind::Skill => library::get_skill_by_slug(&mut tx, slug)
                    .await
                    .map_err(internal)?
                    .map(|s| s.id),
            };
            match resolved {
                Some(id) => id,
                None => return Ok(Json(UsageResponse { recorded: false })),
            }
        }
        (Some(_), Some(_)) => return Err(bad("give artifact_id OR artifact_slug, not both")),
        (None, None) => return Err(bad("one of artifact_id / artifact_slug is required")),
    };

    library::record_usage(
        &mut tx,
        principal.org_id,
        kind,
        artifact_id,
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

/// One auxiliary file in a skill bundle — a reference, a script, a template.
/// The body is `instructions_md`; these are everything else the skill ships
/// with, and they are served verbatim by `download` and MCP `skill_fetch`.
#[derive(Deserialize, Serialize, ToSchema)]
pub(crate) struct SkillResourceInput {
    /// Bundle-relative path, e.g. `references/schema.md`.
    pub path: String,
    /// The file's contents.
    pub content: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SkillProposeRequest {
    /// Short skill name (the dedup key), e.g. "add a data provider".
    pub name: String,
    /// The whole procedure as markdown — steps, checks, gotchas.
    pub instructions_md: String,
    /// One line: what this skill is for and when to reach for it.
    pub summary: Option<String>,
    /// Task domain for the catalog, e.g. `database`, `testing`.
    pub domain: Option<String>,
    /// The rest of the bundle. Omit for a body-only skill. The whole bundle —
    /// body, front-matter and these files — is bounded together, not per file.
    #[serde(default)]
    pub resources: Option<Vec<SkillResourceInput>>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SkillProposeResponse {
    /// `created` (a fresh DRAFT waits for a maintainer to publish it),
    /// `new_version` (this improved a published skill — the draft waits, the
    /// published version keeps serving), or `duplicate` (collapsed onto an
    /// existing skill — see `maturity`).
    pub outcome: String,
    pub skill_id: Uuid,
    pub slug: String,
    /// The semver of the draft this created, on `created` / `new_version`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semver: Option<String>,
    /// On `new_version`: the version that keeps serving until the new draft is
    /// signed. Proof, in the response, that proposing did not disturb agents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_semver: Option<String>,
    /// Present on a duplicate: the maturity of the skill this collapsed onto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maturity: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/library/skills/propose",
    request_body = SkillProposeRequest,
    description = "Propose a skill bundle as a DRAFT (lib:propose). The counterpart to standards/propose for procedures. The outcome is only ever a draft — never served to agents until a named human publishes a version. A skill that is already published takes the proposal as its NEXT VERSION and keeps serving the signed one meanwhile; a skill with a draft already pending, or a deprecated one, collapses as a duplicate. Rate-limited per author and bounded as a whole bundle (body + front-matter + resources).",
    responses(
        (status = 200, body = SkillProposeResponse),
        (status = 400, description = "the bundle exceeds the total size bound"),
        (status = 429, description = "per-author hourly proposal budget spent"),
    )
)]
pub(crate) async fn skill_propose(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SkillProposeRequest>,
) -> Result<Json<SkillProposeResponse>, HttpError> {
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
    if req.instructions_md.trim().is_empty() {
        return Err(bad("instructions_md must not be empty".to_string()));
    }
    if let Some(s) = &req.summary {
        cap(s, MAX_PROPOSAL_STATEMENT, "summary")?;
    }
    // The bundle's total weight is the store's rule (one constant, both
    // surfaces); this handler only checks what the store cannot see — that
    // every resource is a named file.
    let resources = req.resources.unwrap_or_default();
    for r in &resources {
        if r.path.trim().is_empty() {
            return Err(bad("every resource needs a path".to_string()));
        }
    }
    let resources =
        serde_json::to_value(&resources).map_err(|e| internal(anyhow::Error::new(e)))?;

    let manifest = serde_json::json!({
        "name": req.name,
        "summary": req.summary,
        "domain": req.domain,
    });

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let outcome = library::propose_skill(
        &mut tx,
        &library::SkillProposal {
            org_id: principal.org_id,
            author: principal.user_id,
            name: req.name,
            description: req.summary,
            domain: req.domain,
            content_md: req.instructions_md,
            manifest,
            resources,
        },
        propose_per_hour(),
    )
    .await
    .map_err(internal)?;

    match outcome {
        library::SkillProposeOutcome::Created {
            skill_id,
            slug,
            semver,
            ..
        } => {
            tx.commit().await.map_err(internal)?;
            Ok(Json(SkillProposeResponse {
                outcome: "created".into(),
                skill_id,
                slug,
                semver: Some(semver),
                serving_semver: None,
                maturity: None,
            }))
        }
        library::SkillProposeOutcome::NewVersion {
            skill_id,
            slug,
            semver,
            serving_semver,
            ..
        } => {
            tx.commit().await.map_err(internal)?;
            Ok(Json(SkillProposeResponse {
                outcome: "new_version".into(),
                skill_id,
                slug,
                semver: Some(semver),
                serving_semver,
                maturity: None,
            }))
        }
        library::SkillProposeOutcome::Duplicate {
            skill_id,
            slug,
            maturity,
        } => Ok(Json(SkillProposeResponse {
            outcome: "duplicate".into(),
            skill_id,
            slug,
            semver: None,
            serving_semver: None,
            maturity: Some(maturity.as_str().into()),
        })),
        library::SkillProposeOutcome::RateLimited { per_hour } => Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("proposal budget spent: {per_hour} per author per hour — the next skill can wait sixty minutes"),
        )
            .into()),
        library::SkillProposeOutcome::TooLarge { chars, max } => Err(bad(format!(
            "the bundle is {chars} chars; the limit is {max} for body, front-matter and resources together — split it into several skills"
        ))),
    }
}

// ── the maintainer gate (lib:publish) ────────────────────────────────────

/// A stack or category is a filing token, not prose: `rust`, `typescript`,
/// `general`, `errors`. Longer than this and it is a sentence in the wrong box.
const MAX_FILING_TOKEN: usize = 40;

#[derive(Deserialize, ToSchema, Default)]
pub(crate) struct StandardUpdateRequest {
    /// Re-file under a real stack. Mining and the ratification bridge file
    /// every candidate under `general`; this is how a maintainer narrows it to
    /// `rust` or `typescript` so the stack's own page composes from it.
    /// Normalised to a slug token (`Rust` → `rust`).
    pub stack: Option<String>,
    /// Re-file under a category (`errors`, `testing`, …). Same normalisation.
    pub category: Option<String>,
    /// Re-word the rule. Writes a NEW revision — never an in-place edit.
    pub statement: Option<String>,
    pub rationale: Option<String>,
    /// Good/bad examples, verbatim markdown.
    pub examples_md: Option<String>,
    /// `mandatory` | `recommended` | `experimental`.
    pub enforcement: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StandardUpdateResponse {
    #[serde(flatten)]
    pub standard: StandardView,
    /// The new revision number when the WORDING changed; `null` when only the
    /// filing (stack/category) moved, or nothing changed at all. Where a rule
    /// is shelved is not a revision of what the org committed to.
    pub rev: Option<i32>,
}

#[utoipa::path(
    patch,
    path = "/v1/library/standards/{id}",
    params(("id" = Uuid, Path,)),
    request_body = StandardUpdateRequest,
    description = "Narrow, re-file, or re-word a rule during triage — the edit the gate was missing (mining files everything under `general`, and a maintainer could previously only judge a candidate, never fix where it lives or what it says). Takes lib:publish. A re-word appends a numbered revision and dirty-marks the standards page of an adopted rule (both pages when the stack moved); it is never a silent in-place mutation. Deprecated and rejected rules are terminal — 409, re-propose instead.",
    responses(
        (status = 200, body = StandardUpdateResponse),
        (status = 400),
        (status = 404),
        (status = 409, description = "the rule is deprecated or rejected — terminal"),
    )
)]
pub(crate) async fn standard_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<StandardUpdateRequest>,
) -> Result<Json<StandardUpdateResponse>, HttpError> {
    let principal = auth_of(&state, &headers, SCOPE_LIB_PUBLISH)
        .await?
        .principal;
    let bad = |m: String| HttpError::from((StatusCode::BAD_REQUEST, m));
    let text = |v: Option<String>, max: usize, field: &str| -> Result<Option<String>, HttpError> {
        match v {
            None => Ok(None),
            Some(v) if v.trim().is_empty() => Err(bad(format!("{field} must not be empty"))),
            Some(v) if v.chars().count() > max => Err(bad(format!("{field} exceeds {max} chars"))),
            Some(v) => Ok(Some(v.trim().to_string())),
        }
    };
    // A filing token is an identity, so it is normalised the same way a slug
    // is — otherwise "Rust" and "rust" become two shelves for one stack.
    let filing = |v: Option<String>, field: &str| -> Result<Option<String>, HttpError> {
        Ok(text(v, MAX_FILING_TOKEN, field)?.map(|v| library::slugify(&v)))
    };

    let enforcement = match req.enforcement.as_deref() {
        None => None,
        Some(e) => Some(Enforcement::parse(e).ok_or_else(|| {
            bad("enforcement must be mandatory | recommended | experimental".to_string())
        })?),
    };
    let patch = library::StandardPatch {
        stack: filing(req.stack, "stack")?,
        category: filing(req.category, "category")?,
        statement: text(req.statement, MAX_PROPOSAL_STATEMENT, "statement")?,
        rationale: text(req.rationale, MAX_PROPOSAL_RATIONALE, "rationale")?,
        detail_md: text(req.examples_md, MAX_PROPOSAL_EXAMPLES, "examples_md")?,
        enforcement,
    };

    let mut tx = state.store.scoped_tx(&principal).await.map_err(internal)?;
    let rev = match library::update_standard(&mut tx, id, &patch, principal.user_id)
        .await
        .map_err(internal)?
    {
        library::UpdateOutcome::Updated { rev } => rev,
        library::UpdateOutcome::NotFound => {
            return Err((StatusCode::NOT_FOUND, "standard not found".to_string()).into())
        }
        library::UpdateOutcome::Terminal { lifecycle } => {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "this rule is {} — terminal. Re-propose it instead of editing history.",
                    lifecycle.as_str()
                ),
            )
                .into())
        }
    };
    // Read it back inside the same transaction: the maintainer's next render
    // must show what the database actually holds, not what we hoped to write.
    let Some(after) = library::get_standard(&mut tx, id).await.map_err(internal)? else {
        return Err((StatusCode::NOT_FOUND, "standard not found".to_string()).into());
    };
    let pulse = pulse_index(&mut tx, Some(id)).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(Json(StandardUpdateResponse {
        standard: standard_view(&after, pulse.get(&id)),
        rev,
    }))
}

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
