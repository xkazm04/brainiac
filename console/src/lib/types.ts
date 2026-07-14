// The REST payload types — GENERATED, not mirrored.
//
// `api-schema.d.ts` is produced by `npm run gen:api` from `openapi.json`,
// which the server itself emits (`brainiac openapi`) from the very structs
// its handlers serialize. So these names cannot drift from the API: change a
// response shape in Rust, regenerate, and TypeScript fails here until the
// console agrees.
//
// This file is the stable alias layer — consumers import friendly names from
// `@/lib/types` and never touch the generated file's `components["schemas"]`
// indirection. Add an alias when the server adds an endpoint.

import type { components } from "./api-schema";

type S = components["schemas"];

// ── retrieval ───────────────────────────────────────────────────────────
export type SearchHit = S["SearchHit"];
export type SearchResponse = S["SearchResponse"];

// ── governance: promotions ──────────────────────────────────────────────
export type PendingPromotion = S["PendingPromotion"];
export type PromotionMemory = S["PromotionMemory"];
export type PromotionProvenance = S["PromotionProvenance"];
export type ReviewedPromotion = S["ReviewDecisionResponse"];

// ── governance: contradictions ──────────────────────────────────────────
export type Contradiction = S["ContradictionRow"];
export type ContradictionMemory = S["ContradictionMemoryRef"];
/** Request-side vocabulary (a client constraint, not a response shape). */
export type ContradictionResolution = "supersede" | "coexist" | "dismiss";

// ── governance: disputed memories (feedback triage) ─────────────────────
export type FlaggedMemory = S["FlaggedMemory"];
export type FeedbackClaims = S["FeedbackClaims"];

// ── governance: audit ───────────────────────────────────────────────────
export type AuditEvent = S["AuditEvent"];

// ── memories ────────────────────────────────────────────────────────────
export type MemoryRow = S["MemoryRow"];
export type MemoriesList = S["MemoryListResponse"];
export type MemoryDetail = S["MemoryDetailResponse"];
export type ChainLink = S["ChainLink"];
export type ExpiringMemory = S["ExpiringMemory"];

// ── graph ───────────────────────────────────────────────────────────────
export type Graph = S["GraphResponse"];
export type GraphCanonical = S["GraphCanonical"];
export type GraphEntity = S["GraphEntity"];
export type GraphEdge = S["GraphEdge"];
export type GraphOverview = S["GraphOverviewResponse"];
export type CanonicalDetail = S["CanonicalDetailResponse"];

// ── analytics ───────────────────────────────────────────────────────────
export type Analytics = S["AnalyticsResponse"];
export type ObservatoryPayload = S["ObservatoryResponse"];
export type KnowledgeHealth = S["KnowledgeHealthResponse"];
export type KhPillars = S["KhPillars"];
export type KhSignals = S["KhSignals"];
export type KhAttention = S["KhAttention"];
export type KhTrendPoint = S["TrendPoint"];
export type PracticeDivergences = S["PracticeDivergenceResponse"];
export type PracticeDivergence = S["PracticeDivergence"];

// ── ops (sweeps) ────────────────────────────────────────────────────────
export type Sweeps = S["SweepsResponse"];
export type SweepSchedule = S["SweepSchedule"];

// ── ingest ──────────────────────────────────────────────────────────────
export type SourceFeedItem = S["SourceRow"];
export type PipelineRun = S["PipelineRunRow"];
export type QueueHealth = S["QueueHealthResponse"];

// ── keys / tokens ───────────────────────────────────────────────────────
export type ApiToken = S["TokenSummary"];
export type MintedToken = S["CreatedTokenResponse"];
export type OrgUser = S["OrgUser"];
export type TokenPreview = S["TokenPreviewResponse"];

// ── documents / the knowledge base (KB2, ARCHITECTURE §8) ───────────────
// Generated like everything else now that the /v1/docs endpoints ship.

/** A memory's lifecycle facet (migration 0015) — what a claim's standing is. */
export type MemoryLifecycle = "shipped" | "in_flight" | "proposed";

/** Revision publish policy (brainiac-core RevisionPolicy). */
export type RevisionPolicy = "auto_published" | "needs_review" | "rejected";

export type DocSummary = S["DocSummary"];
export type DocRevision = S["DocRevisionView"];
/** A revision awaiting a human — the page's unpublished future. */
export type DocPendingRevision = S["DocRevisionView"];
/** A memory resolved from `composed_from` — the provenance of one claim. */
export type DocCitation = S["Citation"];
export type DocDocument = S["DocSummary"];
/** GET /v1/docs/{slug} — the reader's whole payload, provenance included. */
export type DocDetail = S["DocDetailResponse"];
export type DocRevisionSummary = S["DocRevisionView"];
export type DocApproval = S["DocApproveResponse"];
