// Governance-surface client: the enriched review-queue, contradiction-queue
// and audit-trail payloads (crates/brainiac-server http.rs + console.rs).
// Lives beside api.ts rather than in it so the richer queue types stay
// grouped with the endpoints that produce them. SERVER ONLY, same rules.

import "server-only";

import { ApiError, type ApiConfig } from "./api";

async function call<T>(cfg: ApiConfig, path: string): Promise<T> {
  const doFetch = cfg.fetchImpl ?? fetch;
  const res = await doFetch(`${cfg.baseUrl}${path}`, {
    headers: { authorization: `Bearer ${cfg.token}` },
    cache: "no-store",
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || res.statusText);
  }
  return (await res.json()) as T;
}

async function post<T>(cfg: ApiConfig, path: string, body: unknown): Promise<T> {
  const doFetch = cfg.fetchImpl ?? fetch;
  const res = await doFetch(`${cfg.baseUrl}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${cfg.token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
    cache: "no-store",
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || res.statusText);
  }
  return (await res.json()) as T;
}

// ── promotions ──────────────────────────────────────────────────────────

export interface PromotionMemory {
  content: string;
  kind: string | null;
  status: string | null;
  confidence: number | null;
  team: string | null;
}

export interface PromotionProvenance {
  actor_kind: string;
  actor_id: string;
  model_ref: string | null;
  source_kind: string | null;
  source_ref: string | null;
}

export interface PromotionQueueItem {
  id: string;
  memory_id: string;
  from_status: string;
  to_status: string;
  policy_rule: string | null;
  age_secs: number;
  /** Null when the memory is not visible to the caller under RLS. */
  memory: PromotionMemory | null;
  provenance: PromotionProvenance | null;
}

export interface PromotionQueuePage {
  /** One page of the backlog, oldest first. */
  promotions: PromotionQueueItem[];
  /**
   * Every promotion awaiting review, independent of the page window — the real
   * backlog. `promotions.length` is only ever the size of the page: the server
   * caps it at 200 and defaults to 100, so rendering the array length as the
   * queue depth understates a 5k backlog as "100 waiting" and never corrects
   * itself. The server counts this over the whole table; the only way to get it
   * wrong is to throw it away, which is what this client used to do.
   */
  total: number;
}

/**
 * One page of the promotion review queue.
 *
 * `limit`/`offset` are sent explicitly rather than left to the server's
 * defaults: a caller that pages has to know what window it asked for in order
 * to say so in the UI, and a silent default is exactly how the page length got
 * mistaken for the backlog in the first place.
 */
export async function promotionQueue(
  cfg: ApiConfig,
  opts: { limit?: number; offset?: number } = {},
): Promise<PromotionQueuePage> {
  const params = new URLSearchParams();
  if (opts.limit !== undefined) params.set("limit", String(opts.limit));
  if (opts.offset !== undefined) params.set("offset", String(opts.offset));
  const qs = params.toString();
  return call(cfg, `/v1/reviews/promotions${qs ? `?${qs}` : ""}`);
}

/** What became of ONE promotion in a bulk decision. */
export interface BulkReviewRow {
  promotion_id: string;
  ok: boolean;
  /** The status this id would have returned on its own: 200/403/404/409/500. */
  status: number;
  memory_id: string | null;
  memory_status: string | null;
  error: string | null;
}

export interface BulkReviewResult {
  decided: number;
  failed: number;
  results: BulkReviewRow[];
}

/**
 * Decide many promotions in one request.
 *
 * The batch returns 200 even when rows fail — a mixed outcome is the NORMAL
 * one, not an error: a selection can span teams the token maintains and teams
 * it does not, and the server authorizes each item separately. So callers must
 * read `results`, not just the absence of a throw. Only a malformed batch
 * (unknown action, empty, over the server's 200 cap) throws.
 */
export async function bulkReviewPromotions(
  cfg: ApiConfig,
  ids: string[],
  action: "approve" | "reject",
): Promise<BulkReviewResult> {
  return post(cfg, "/v1/reviews/promotions/bulk", { ids, action });
}

// ── contradictions ──────────────────────────────────────────────────────

export type ContradictionStatus =
  | "open"
  | "resolved_supersede"
  | "resolved_coexist"
  | "dismissed"
  | "all";

export interface ContradictionQueueItem {
  id: string;
  memory_a: { id: string; content: string | null };
  memory_b: { id: string; content: string | null };
  detected_by: string;
  status: string;
  suggested_resolution: string | null;
  resolved_by: string | null;
  resolved_at: string | null;
  created_at: string;
  age_secs: number;
}

export interface ContradictionQueuePage {
  contradictions: ContradictionQueueItem[];
  /**
   * Rows matching the current filters, ignoring the page window — the real
   * backlog. `contradictions.length` is only ever the size of the page, so
   * rendering it as the queue depth understates the moment the backlog passes
   * `limit` (default 50).
   */
  total: number;
  counts: { status: string; count: number }[];
}

export async function contradictionQueue(
  cfg: ApiConfig,
  opts: {
    status?: ContradictionStatus;
    detectedBy?: string;
    minAgeHours?: number;
    limit?: number;
    offset?: number;
  } = {},
): Promise<ContradictionQueuePage> {
  const params = new URLSearchParams();
  if (opts.status) params.set("status", opts.status);
  if (opts.detectedBy) params.set("detected_by", opts.detectedBy);
  if (opts.minAgeHours !== undefined) params.set("min_age_hours", String(opts.minAgeHours));
  if (opts.limit !== undefined) params.set("limit", String(opts.limit));
  if (opts.offset !== undefined) params.set("offset", String(opts.offset));
  const qs = params.toString();
  return call(cfg, `/v1/reviews/contradictions${qs ? `?${qs}` : ""}`);
}

// ── audit trail ─────────────────────────────────────────────────────────

export type AuditKind =
  | "promotion_review"
  | "contradiction_resolution"
  | "feedback_resolution";

export interface AuditEvent {
  kind: AuditKind;
  id: string;
  memory_id: string;
  /** Only set for contradiction_resolution. */
  memory_b: string | null;
  outcome: string;
  detail: string | null;
  /** Null for policy (auto) decisions — no human actor. */
  actor_id: string | null;
  at: string;
}

export interface AuditPage {
  events: AuditEvent[];
  /**
   * The full (filtered) feed length, independent of the page window —
   * `events.length` is only ever the size of the page. A client that renders
   * it as the backlog understates the moment the feed passes `limit`
   * (default 50), the same bug `ContradictionQueuePage.total` exists to head
   * off above.
   */
  total: number;
}

/** Reverse-chronological governance feed: promotion reviews, contradiction
 *  resolutions, and dispute resolutions. See console.rs's `audit` handler —
 *  `kind` narrows to one action type, `total` always describes the same
 *  (filtered) set the page is drawn from. */
export async function auditTrail(
  cfg: ApiConfig,
  opts: { limit?: number; offset?: number; kind?: AuditKind } = {},
): Promise<AuditPage> {
  const params = new URLSearchParams();
  params.set("limit", String(opts.limit ?? 50));
  if (opts.offset !== undefined) params.set("offset", String(opts.offset));
  if (opts.kind) params.set("kind", opts.kind);
  const out = await call<{ total: number; events: AuditEvent[] }>(
    cfg,
    `/v1/audit?${params.toString()}`,
  );
  return { events: out.events, total: out.total };
}

// ── disputed memories (feedback triage) ─────────────────────────────────

export interface FlaggedMemory {
  memory_id: string;
  content: string;
  kind: string;
  status: string;
  team_id: string | null;
  valid_to: string | null;
  claims: { wrong: number; outdated: number };
  /** Reporter notes on the open claims (newest first, capped server-side). */
  notes: string[];
  oldest_claim_secs: number;
}

export type DisputeResolution = "reverified" | "deprecated" | "dismissed";

/** The triage queue: memories with unresolved wrong/outdated reports. */
export async function feedbackQueue(cfg: ApiConfig, limit = 50): Promise<FlaggedMemory[]> {
  const out = await call<{ flagged: FlaggedMemory[] }>(
    cfg,
    `/v1/reviews/feedback?limit=${limit}`,
  );
  return out.flagged;
}

/**
 * Answer the open claims against a memory. `reverified` extends its validity
 * window, `deprecated` ends it now, `dismissed` leaves the memory standing —
 * all three close every open claim on it.
 */
export async function resolveDispute(
  cfg: ApiConfig,
  memoryId: string,
  resolution: DisputeResolution,
  days?: number,
): Promise<{ memory_id: string; resolution: string; claims_closed: number }> {
  return post(cfg, `/v1/reviews/feedback/${memoryId}/resolve`, {
    resolution,
    ...(days ? { days } : {}),
  });
}
