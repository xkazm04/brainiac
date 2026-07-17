// Server-side brainiac REST client. SERVER ONLY — the bearer token must
// never reach the browser; every call happens inside server components or
// server actions. Config via env: BRAINIAC_API_URL (default the binary's
// default bind) + BRAINIAC_API_TOKEN (a token from BRAINIAC_TOKENS).

import "server-only";

import type {
  AddedRepo,
  Analytics,
  ApiToken,
  CanonicalDetail,
  CreatedProject,
  Contradiction,
  ContradictionResolution,
  DocApproval,
  EditSectionBody,
  EditSectionResponse,
  DocDetail,
  DocRevisionSummary,
  DocsListResponse,
  Graph,
  GraphOverview,
  KnowledgeHealth,
  LibrarySkill,
  LibraryStandard,
  MemoriesList,
  MemoryDetail,
  MemoryValidity,
  MintedToken,
  ObservatoryPayload,
  OnboardRequest,
  OnboardDecision,
  OrgUser,
  PendingPromotion,
  Project,
  PipelineRun,
  PracticeDivergences,
  QueueHealth,
  ReviewedPromotion,
  SearchHit,
  SkillDetail,
  SkillsList,
  SourceFeedItem,
  StandardDetail,
  StandardsList,
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

/** Per-request timeout (ms). A hung backend must never wedge a server component. */
const API_TIMEOUT_MS = Number(process.env.BRAINIAC_API_TIMEOUT_MS ?? "15000") || 15000;

async function call<T>(
  cfg: ApiConfig,
  method: "GET" | "POST" | "PUT" | "DELETE",
  path: string,
  body?: unknown,
): Promise<T> {
  const doFetch = cfg.fetchImpl ?? fetch;
  let res: Response;
  try {
    res = await doFetch(`${cfg.baseUrl}${path}`, {
      method,
      headers: {
        authorization: `Bearer ${cfg.token}`,
        ...(body !== undefined ? { "content-type": "application/json" } : {}),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
      cache: "no-store",
      // Bound every call: a backend that accepts the socket but never responds
      // would otherwise hang the awaiting server component indefinitely (fetch has
      // no default timeout), and a hang is neither an error nor a status — it
      // slips past every retry and every demo-fallback net. Surface it as an error.
      signal: AbortSignal.timeout(API_TIMEOUT_MS),
    });
  } catch (e) {
    if (e instanceof DOMException && e.name === "TimeoutError") {
      throw new ApiError(504, `request to ${path} timed out after ${API_TIMEOUT_MS}ms`);
    }
    throw new ApiError(0, e instanceof Error ? e.message : String(e));
  }
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

/**
 * The as-of skeleton: {id, valid_from, valid_to, status} for the whole visible
 * corpus under the (non-as_of) filter. Tiny — ~40 bytes/row, ~100KB at 660 —
 * so the archive holds it to scrub the time axis instantly while row content
 * pages server-side. Takes the same filter params as `listMemories`.
 */
export async function memoryValidity(
  cfg: ApiConfig,
  params: Record<string, string> = {},
): Promise<MemoryValidity> {
  const qs = new URLSearchParams(params).toString();
  return call(cfg, "GET", `/v1/memories/validity${qs ? `?${qs}` : ""}`);
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

// ── projects + developer onboarding (admin token) ───────────────────────

export async function listProjects(cfg: ApiConfig): Promise<Project[]> {
  const out = await call<{ projects: Project[] }>(cfg, "GET", "/v1/projects");
  return out.projects;
}

export async function createProject(cfg: ApiConfig, name: string): Promise<CreatedProject> {
  return call(cfg, "POST", "/v1/projects", { name });
}

export async function addProjectRepo(
  cfg: ApiConfig,
  projectId: string,
  remote: string,
): Promise<AddedRepo> {
  return call(cfg, "POST", `/v1/projects/${projectId}/repos`, { remote });
}

export async function removeProjectRepo(
  cfg: ApiConfig,
  projectId: string,
  repoId: string,
): Promise<void> {
  await call(cfg, "DELETE", `/v1/projects/${projectId}/repos/${repoId}`);
}

export async function listOnboardRequests(cfg: ApiConfig): Promise<OnboardRequest[]> {
  const out = await call<{ requests: OnboardRequest[] }>(cfg, "GET", "/v1/onboard/requests");
  return out.requests;
}

export async function decideOnboardRequest(
  cfg: ApiConfig,
  id: string,
  decision: "approve" | "deny",
): Promise<OnboardDecision> {
  return call(cfg, "POST", `/v1/onboard/requests/${id}/${decision}`);
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

// ── library (LB1/LB2): standards + skills ───────────────────────────────

/** All rules regardless of lifecycle — the console's board view. Agents use
 *  the adopted-only default; a maintainer's board must see proposals too. */
export async function listStandards(
  cfg: ApiConfig,
  lifecycle: "proposed" | "adopted" | "deprecated" | "all" = "all",
): Promise<LibraryStandard[]> {
  const out = await call<StandardsList>(
    cfg,
    "GET",
    `/v1/library/standards?lifecycle=${lifecycle}`,
  );
  return out.standards;
}

export async function getStandard(cfg: ApiConfig, id: string): Promise<StandardDetail> {
  return call(cfg, "GET", `/v1/library/standards/${encodeURIComponent(id)}`);
}

/** Adopt a proposed rule (lib:publish). `decree` signs for an evidence-free
 *  rule by name; without it the backend answers 409 for such a rule. */
export async function adoptStandard(
  cfg: ApiConfig,
  id: string,
  decree = false,
): Promise<{ adopted: boolean }> {
  return call(cfg, "POST", `/v1/library/standards/${encodeURIComponent(id)}/adopt`, {
    decree,
  });
}

export async function deprecateStandard(
  cfg: ApiConfig,
  id: string,
): Promise<{ adopted: boolean }> {
  return call(cfg, "POST", `/v1/library/standards/${encodeURIComponent(id)}/deprecate`);
}

/** Reject a proposed candidate — kept, not deleted: the mining sweep dedups
 *  against rejections, so saying no once means not being asked again. */
export async function rejectStandard(
  cfg: ApiConfig,
  id: string,
): Promise<{ adopted: boolean }> {
  return call(cfg, "POST", `/v1/library/standards/${encodeURIComponent(id)}/reject`);
}

export async function listSkills(cfg: ApiConfig): Promise<LibrarySkill[]> {
  const out = await call<SkillsList>(cfg, "GET", "/v1/library/skills");
  return out.skills;
}

export async function getSkillDetail(cfg: ApiConfig, slug: string): Promise<SkillDetail> {
  return call(cfg, "GET", `/v1/library/skills/${encodeURIComponent(slug)}`);
}

// ── documents (KB2) ─────────────────────────────────────────────────────

/** The wiki's browse query — mirrors GET /v1/docs' params. The server pages,
 *  facets and builds the space tree; the client never holds the whole corpus. */
export interface DocsQuery {
  /** Full-text search over title + slug. */
  q?: string;
  kind?: string;
  tag?: string;
  /** Behind its sources — the recomposing tab. */
  stale?: boolean;
  /** The folder = first slug segment. `""` is the un-namespaced (unfiled) space. */
  space?: string;
  status?: string;
  /** A revision is waiting on a human — the review tab. */
  needsReview?: boolean;
  /** Ask for the cross-filtered facet menu (the space directory + tab counts). */
  facets?: boolean;
  /** 1..200. */
  limit?: number;
  offset?: number;
}

/**
 * GET /v1/docs — the paginated, faceted envelope. Returns `total` (filtered
 * depth), `facets` (only when `facets:1`) and the current page of `documents`.
 * A `space` of `""` is meaningful (the un-namespaced bucket), so it is sent
 * whenever the key is present rather than only when truthy.
 */
export async function listDocs(
  cfg: ApiConfig,
  query: DocsQuery = {},
): Promise<DocsListResponse> {
  const qs = new URLSearchParams();
  if (query.q) qs.set("q", query.q);
  if (query.kind) qs.set("kind", query.kind);
  if (query.tag) qs.set("tag", query.tag);
  // The server deserializes these as real booleans — they must be the literal
  // `true`, not `1` (which 400s). `facets` is the exception the server accepts
  // as `1`, but `true` is honoured there too, so everything speaks one dialect.
  if (query.stale) qs.set("stale", "true");
  if (query.space !== undefined) qs.set("space", query.space);
  if (query.status) qs.set("status", query.status);
  if (query.needsReview) qs.set("needs_review", "true");
  if (query.facets) qs.set("facets", "true");
  if (query.limit !== undefined) qs.set("limit", String(query.limit));
  if (query.offset !== undefined) qs.set("offset", String(query.offset));
  const s = qs.toString();
  return call<DocsListResponse>(cfg, "GET", `/v1/docs${s ? `?${s}` : ""}`);
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

/**
 * Edit one section of a page (KB4). The response's `outcome` is the point: a
 * pinned section is `saved` (human prose, never regenerated over), a composed
 * section is `captured` (the edit becomes proposed knowledge and goes through
 * review — the text is not written into the page).
 */
export async function editDocSection(
  cfg: ApiConfig,
  slug: string,
  body: EditSectionBody,
): Promise<EditSectionResponse> {
  return call(cfg, "POST", `/v1/docs/${encodeURIComponent(slug)}/edit`, body);
}
