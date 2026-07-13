//! The OpenAPI document — derived from the handlers themselves.
//!
//! Every path here is a `#[utoipa::path]` annotation on the real handler and
//! every schema is the struct that handler actually serializes, so the spec
//! cannot drift from the API: change a response shape and the spec changes
//! with it, in the same commit. `GET /openapi.json` serves it, and the
//! console generates its TypeScript types from that document
//! (`npm run gen:api`) rather than mirroring the payloads by hand.
//!
//! Auth: every `/v1` route requires `Authorization: Bearer <token>` — an
//! env-map token (all scopes) or a managed `brk_…` key with the read / write
//! / admin scope the route demands. `/health` and this document are open.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::{console, http};

pub struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .description(Some(
                            "Env-map token (unrestricted) or a managed `brk_…` \
                             key carrying the read/write/admin scope the route requires.",
                        ))
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Brainiac",
        description = "GitOps for organizational AI knowledge — capture, govern, and \
                       serve an organization's memory. Every read is RLS-scoped to the \
                       caller: an agent can never retrieve what its operator cannot.",
        version = env!("CARGO_PKG_VERSION"),
        license(name = "MIT"),
    ),
    servers((url = "http://127.0.0.1:8600", description = "Local")),
    modifiers(&BearerAuth),
    tags(
        (name = "system", description = "Liveness and the spec itself."),
        (name = "memories", description = "Retrieval, capture, and the memory ledger."),
        (name = "reviews", description = "Governance queues: promotions, contradictions, \
                                          disputed memories, and the audit trail."),
        (name = "graph", description = "The canonical entity graph and its cortex map."),
        (name = "analytics", description = "Governance health counters and the observatory."),
        (name = "ingest", description = "Source feed and pipeline runs."),
        (name = "queue", description = "Job queue health and dead-letter recovery (admin)."),
        (name = "tokens", description = "Managed API keys and their blast radius (admin)."),
        (name = "keys", description = "Org directory backing key minting (admin)."),
    ),
    paths(
        // system
        http::health,
        openapi_json,
        // memories
        http::search,
        http::memory_add,
        http::source_status,
        console::memories_list,
        console::memory_detail,
        console::memories_expiring,
        console::memory_reverify,
        // reviews
        http::pending_promotions,
        console::approve,
        console::reject,
        console::list_contradictions,
        console::resolve_contradiction,
        console::feedback_queue,
        console::resolve_feedback_claims,
        console::audit,
        // graph
        console::graph,
        console::graph_overview,
        console::graph_canonical,
        // analytics
        console::analytics,
        console::observatory,
        // ingest
        console::sources_list,
        console::pipeline_runs,
        // queue (admin)
        http::queue_health,
        http::queue_dead_letters,
        http::queue_requeue,
        // tokens + keys (admin)
        http::list_tokens,
        http::create_token,
        http::revoke_token,
        console::org_users,
        console::token_preview,
    ),
    components(schemas(
        // system
        http::HealthResponse,
        // memories
        http::SearchBody,
        http::AnchorRef,
        http::SearchHit,
        http::SearchResponse,
        http::MemoryAddBody,
        http::MemoryAcceptedResponse,
        http::SourceInfo,
        http::SourceJob,
        http::SourceResults,
        http::SourceStatusResponse,
        console::MemoryRow,
        console::MemoryListResponse,
        console::MemoryProvenance,
        console::MemoryEntityRef,
        console::MemoryPromotion,
        console::ChainLink,
        console::MemoryChain,
        console::MemoryDetailResponse,
        console::ExpiringMemory,
        console::ExpiringResponse,
        console::ReverifyBody,
        console::ReverifyResponse,
        // reviews
        http::PromotionMemory,
        http::PromotionProvenance,
        http::PendingPromotion,
        http::PromotionQueueResponse,
        console::ReviewDecisionResponse,
        console::ContradictionMemoryRef,
        console::ContradictionRow,
        console::ContradictionQueueResponse,
        console::ResolveBody,
        console::ResolveContradictionResponse,
        console::FeedbackClaims,
        console::FlaggedMemory,
        console::FeedbackQueueResponse,
        console::ResolveFeedbackBody,
        console::ResolveFeedbackResponse,
        console::AuditEvent,
        console::AuditResponse,
        console::StatusCount,
        // graph
        console::GraphCanonical,
        console::GraphEntity,
        console::GraphEdge,
        console::GraphResponse,
        console::TeamLobe,
        console::OverviewCanonical,
        console::TeamLink,
        console::GraphOverviewResponse,
        console::CanonicalSummary,
        console::SurfaceForm,
        console::CanonicalEdge,
        console::NeighborCanonical,
        console::AnchoredMemory,
        console::CanonicalDetailResponse,
        // analytics
        console::AnalyticsReviews,
        console::AnalyticsGraph,
        console::QueueDepth,
        console::AnalyticsResponse,
        console::WeeklyPoint,
        console::ObservatoryWeekly,
        console::KindTeamCount,
        console::TopEntity,
        console::ObservatoryReview,
        console::ObservatoryResponse,
        // ingest
        console::SourceRow,
        console::SourceFeedResponse,
        console::PipelineRunRow,
        console::PipelineRunsResponse,
        // queue
        http::AttemptsBucket,
        http::ArchivedCounts,
        http::QueueHealthResponse,
        http::DeadLetterEntry,
        http::DeadLetterListResponse,
        http::RequeueResponse,
        // tokens + keys
        http::CreateTokenBody,
        http::CreatedTokenResponse,
        http::TokenSummary,
        http::TokenListResponse,
        http::RevokeResponse,
        console::OrgUserTeam,
        console::OrgUser,
        console::OrgUsersResponse,
        console::PreviewBody,
        console::TokenVisibility,
        console::TokenPreviewResponse,
    ))
)]
pub struct ApiDoc;

/// The document itself. Open (like `/health`) — a client needs the contract
/// before it has a token, and the spec describes shapes, never data.
#[utoipa::path(
    get,
    path = "/openapi.json",
    tag = "system",
    description = "This OpenAPI document, derived from the handlers.",
    responses((status = 200, description = "OpenAPI 3.1 document")),
)]
pub(crate) async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec must actually describe the surface — a silently empty doc
    /// (missing `paths(...)` entries) would generate an empty client.
    #[test]
    fn document_covers_the_surface() {
        let doc = ApiDoc::openapi();
        let paths = &doc.paths.paths;
        assert!(
            paths.len() >= 30,
            "expected the full REST surface, got {} paths",
            paths.len()
        );
        for required in [
            "/health",
            "/v1/memories/search",
            "/v1/reviews/promotions",
            "/v1/reviews/feedback",
            "/v1/memories/expiring",
            "/v1/analytics",
            "/v1/graph",
            "/v1/tokens",
            "/v1/queue/health",
        ] {
            assert!(paths.contains_key(required), "spec is missing {required}");
        }
        let schemas = &doc.components.as_ref().expect("components").schemas;
        assert!(
            schemas.len() >= 60,
            "expected the response DTOs, got {} schemas",
            schemas.len()
        );
        assert!(schemas.contains_key("SearchResponse"));
        assert!(schemas.contains_key("FeedbackQueueResponse"));
    }

    /// The committed `openapi.json` (which the console generates its types
    /// from) must match what the handlers actually declare. If this fails,
    /// someone changed a response shape without running `brainiac openapi
    /// --out openapi.json` — regenerate and commit the diff.
    #[test]
    fn committed_document_is_current() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = repo_root.join("openapi.json");
        let Ok(committed) = std::fs::read_to_string(&path) else {
            panic!("openapi.json is missing — run `cargo run -p brainiac-server -- openapi`");
        };
        let current = ApiDoc::openapi().to_pretty_json().expect("serialize spec") + "\n";
        // EOL-insensitive: git autocrlf may smudge the working copy to CRLF
        // on Windows; the contract is the content, not the line endings.
        assert_eq!(
            committed.replace("\r\n", "\n"),
            current,
            "openapi.json is stale — regenerate with `cargo run -p brainiac-server -- openapi --out openapi.json` and re-run `npm run gen:api` in console/"
        );
    }
}
