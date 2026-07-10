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

// ── shared formatting ───────────────────────────────────────────────────

/** Compact age like the Observatory's: 12m / 3.4h / 2.1d. */
export function formatAge(secs: number): string {
  if (secs <= 0) return "just now";
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86400) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86400).toFixed(1)}d`;
}
