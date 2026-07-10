// Server-side brainiac REST client. SERVER ONLY — the bearer token must
// never reach the browser; every call happens inside server components or
// server actions. Config via env: BRAINIAC_API_URL (default the binary's
// default bind) + BRAINIAC_API_TOKEN (a token from BRAINIAC_TOKENS).

import "server-only";

import type {
  Analytics,
  CanonicalDetail,
  Contradiction,
  ContradictionResolution,
  Graph,
  GraphOverview,
  MemoriesList,
  MemoryDetail,
  ObservatoryPayload,
  PendingPromotion,
  ReviewedPromotion,
  SearchHit,
} from "./types";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export interface ApiConfig {
  baseUrl: string;
  token: string;
  /** Injectable for tests; defaults to global fetch. */
  fetchImpl?: typeof fetch;
}

export function configFromEnv(): ApiConfig {
  return {
    baseUrl: process.env.BRAINIAC_API_URL ?? "http://127.0.0.1:8600",
    token: process.env.BRAINIAC_API_TOKEN ?? "",
  };
}

async function call<T>(
  cfg: ApiConfig,
  method: "GET" | "POST",
  path: string,
  body?: unknown,
): Promise<T> {
  const doFetch = cfg.fetchImpl ?? fetch;
  const res = await doFetch(`${cfg.baseUrl}${path}`, {
    method,
    headers: {
      authorization: `Bearer ${cfg.token}`,
      ...(body !== undefined ? { "content-type": "application/json" } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
    cache: "no-store",
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiError(res.status, text || res.statusText);
  }
  return (await res.json()) as T;
}

export async function searchMemories(
  cfg: ApiConfig,
  query: string,
  k = 10,
  asOf?: string,
): Promise<SearchHit[]> {
  const out = await call<{ hits: SearchHit[] }>(cfg, "POST", "/v1/memories/search", {
    query,
    k,
    ...(asOf ? { as_of: asOf } : {}),
  });
  return out.hits;
}

export async function pendingPromotions(cfg: ApiConfig): Promise<PendingPromotion[]> {
  const out = await call<{ promotions: PendingPromotion[] }>(
    cfg,
    "GET",
    "/v1/reviews/promotions",
  );
  return out.promotions;
}

export async function reviewPromotion(
  cfg: ApiConfig,
  id: string,
  action: "approve" | "reject",
): Promise<ReviewedPromotion> {
  return call(cfg, "POST", `/v1/reviews/promotions/${id}/${action}`);
}

export async function listContradictions(cfg: ApiConfig): Promise<Contradiction[]> {
  const out = await call<{ contradictions: Contradiction[] }>(
    cfg,
    "GET",
    "/v1/reviews/contradictions",
  );
  return out.contradictions;
}

export async function resolveContradiction(
  cfg: ApiConfig,
  id: string,
  resolution: ContradictionResolution,
  winnerMemoryId?: string,
  note?: string,
): Promise<{ contradiction_id: string; status: string }> {
  return call(cfg, "POST", `/v1/reviews/contradictions/${id}/resolve`, {
    resolution,
    ...(winnerMemoryId ? { winner_memory_id: winnerMemoryId } : {}),
    ...(note ? { note } : {}),
  });
}

export async function getGraph(cfg: ApiConfig): Promise<Graph> {
  return call(cfg, "GET", "/v1/graph");
}

export async function getAnalytics(cfg: ApiConfig): Promise<Analytics> {
  return call(cfg, "GET", "/v1/analytics");
}

export async function getObservatory(cfg: ApiConfig): Promise<ObservatoryPayload> {
  return call(cfg, "GET", "/v1/analytics/observatory");
}

export async function getGraphOverview(cfg: ApiConfig): Promise<GraphOverview> {
  return call(cfg, "GET", "/v1/graph/overview");
}

export async function listMemories(
  cfg: ApiConfig,
  params: Record<string, string> = {},
): Promise<MemoriesList> {
  const qs = new URLSearchParams(params).toString();
  return call(cfg, "GET", `/v1/memories${qs ? `?${qs}` : ""}`);
}

export async function getMemoryDetail(cfg: ApiConfig, id: string): Promise<MemoryDetail> {
  return call(cfg, "GET", `/v1/memories/${id}`);
}

export async function getGraphCanonical(
  cfg: ApiConfig,
  id: string,
): Promise<CanonicalDetail> {
  return call(cfg, "GET", `/v1/graph/canonical/${id}`);
}
