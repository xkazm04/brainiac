// Server-side brainiac REST client. SERVER ONLY — the bearer token must
// never reach the browser; every call happens inside server components or
// server actions. Config via env: BRAINIAC_API_URL (default the binary's
// default bind) + BRAINIAC_API_TOKEN (a token from BRAINIAC_TOKENS).

import "server-only";

import type {
  Analytics,
  ApiToken,
  CanonicalDetail,
  Contradiction,
  ContradictionResolution,
  DocApproval,
  DocDetail,
  DocRevisionSummary,
  DocSummary,
  Graph,
  GraphOverview,
  KnowledgeHealth,
  MemoriesList,
  MemoryDetail,
  MintedToken,
  ObservatoryPayload,
  OrgUser,
  PendingPromotion,
  PipelineRun,
  PracticeDivergences,
  QueueHealth,
  ReviewedPromotion,
  SearchHit,
  SourceFeedItem,
  SweepSchedule,
  Sweeps,
  TokenPreview,
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
  method: "GET" | "POST" | "PUT",
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
    // Errors are a JSON envelope `{error, code}` (see the server's HttpError).
    // Parse it for the message; fall back to the raw text (older servers
    // returned plain text) and finally the status line.
    const text = await res.text().catch(() => "");
    let message = text;
    if (text) {
      try {
        const parsed = JSON.parse(text) as { error?: unknown };
        if (typeof parsed.error === "string") {
          message = parsed.error;
        }
      } catch {
        // not JSON — keep the raw text (backward-compat).
      }
    }
    throw new ApiError(res.status, message || res.statusText);
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

export async function getKnowledgeHealth(cfg: ApiConfig): Promise<KnowledgeHealth> {
  return call(cfg, "GET", "/v1/analytics/knowledge-health");
}

export async function getPracticeDivergence(cfg: ApiConfig): Promise<PracticeDivergences> {
  return call(cfg, "GET", "/v1/analytics/practice-divergence");
}

// ── ops: sweep scheduling (admin token) ─────────────────────────────────
export async function getSweeps(cfg: ApiConfig): Promise<Sweeps> {
  return call(cfg, "GET", "/v1/ops/sweeps");
}

export async function updateSweep(
  cfg: ApiConfig,
  kind: string,
  patch: { enabled?: boolean; cadence_secs?: number },
): Promise<SweepSchedule> {
  return call(cfg, "PUT", `/v1/ops/sweeps/${kind}`, patch);
}

export async function runSweep(
  cfg: ApiConfig,
  kind: string,
): Promise<{ kind: string; queued: boolean; next_run_at: string | null }> {
  return call(cfg, "POST", `/v1/ops/sweeps/${kind}/run`);
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

export async function getSourcesFeed(cfg: ApiConfig, limit = 30): Promise<SourceFeedItem[]> {
  const out = await call<{ sources: SourceFeedItem[] }>(cfg, "GET", `/v1/sources?limit=${limit}`);
  return out.sources;
}

export async function getPipelineRuns(cfg: ApiConfig, limit = 40): Promise<PipelineRun[]> {
  const out = await call<{ runs: PipelineRun[] }>(cfg, "GET", `/v1/pipeline/runs?limit=${limit}`);
  return out.runs;
}

export async function getQueueHealth(cfg: ApiConfig): Promise<QueueHealth> {
  return call(cfg, "GET", "/v1/queue/health");
}

export async function submitMemory(
  cfg: ApiConfig,
  content: string,
): Promise<{ source_id: string; job_id: number }> {
  return call(cfg, "POST", "/v1/memories", { content });
}

export async function listTokens(cfg: ApiConfig): Promise<ApiToken[]> {
  const out = await call<{ tokens: ApiToken[] }>(cfg, "GET", "/v1/tokens");
  return out.tokens;
}

export async function createToken(
  cfg: ApiConfig,
  name: string,
  userId?: string,
  scopes?: string[],
): Promise<MintedToken> {
  return call(cfg, "POST", "/v1/tokens", {
    name,
    ...(userId ? { user_id: userId } : {}),
    ...(scopes ? { scopes } : {}),
  });
}

export async function revokeToken(cfg: ApiConfig, id: string): Promise<void> {
  await call(cfg, "POST", `/v1/tokens/${id}/revoke`);
}

export async function getOrgUsers(cfg: ApiConfig): Promise<OrgUser[]> {
  const out = await call<{ users: OrgUser[] }>(cfg, "GET", "/v1/org/users");
  return out.users;
}

export async function previewToken(cfg: ApiConfig, userId: string): Promise<TokenPreview> {
  return call(cfg, "POST", "/v1/tokens/preview", { user_id: userId });
}

export async function getGraphCanonical(
  cfg: ApiConfig,
  id: string,
): Promise<CanonicalDetail> {
  return call(cfg, "GET", `/v1/graph/canonical/${id}`);
}

// ── documents (KB2) ─────────────────────────────────────────────────────
export async function listDocs(cfg: ApiConfig): Promise<DocSummary[]> {
  const out = await call<{ documents: DocSummary[] }>(cfg, "GET", "/v1/docs");
  return out.documents;
}

export async function getDoc(cfg: ApiConfig, slug: string): Promise<DocDetail> {
  return call(cfg, "GET", `/v1/docs/${encodeURIComponent(slug)}`);
}

export async function getDocRevisions(
  cfg: ApiConfig,
  slug: string,
): Promise<DocRevisionSummary[]> {
  const out = await call<{ revisions: DocRevisionSummary[] }>(
    cfg,
    "GET",
    `/v1/docs/${encodeURIComponent(slug)}/revisions`,
  );
  return out.revisions;
}

/** Publish a pending revision. Maintainer-scoped; never wired to demo data. */
export async function approveDocRevision(
  cfg: ApiConfig,
  revisionId: string,
): Promise<DocApproval> {
  return call(cfg, "POST", `/v1/docs/revisions/${revisionId}/approve`);
}
