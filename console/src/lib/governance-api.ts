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

export async function promotionQueue(cfg: ApiConfig): Promise<PromotionQueueItem[]> {
  const out = await call<{ promotions: PromotionQueueItem[] }>(
    cfg,
    "/v1/reviews/promotions",
  );
  return out.promotions;
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

export interface AuditEvent {
  kind: "promotion_review" | "contradiction_resolution";
  id: string;
  memory_id: string;
  memory_b: string | null;
  outcome: string;
  detail: string | null;
  actor_id: string | null;
  at: string;
}

export async function auditTrail(cfg: ApiConfig, limit = 50): Promise<AuditEvent[]> {
  const out = await call<{ events: AuditEvent[] }>(cfg, `/v1/audit?limit=${limit}`);
  return out.events;
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
