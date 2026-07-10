import { describe, expect, it, vi } from "vitest";

import {
  ApiError,
  getAnalytics,
  getGraph,
  listContradictions,
  pendingPromotions,
  resolveContradiction,
  reviewPromotion,
  searchMemories,
  type ApiConfig,
} from "./api";

function mockFetch(status: number, payload: unknown) {
  return vi.fn(async () => ({
    ok: status >= 200 && status < 300,
    status,
    statusText: String(status),
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  })) as unknown as typeof fetch;
}

function cfg(fetchImpl: typeof fetch): ApiConfig {
  return { baseUrl: "http://brainiac.test:8600", token: "tok_test", fetchImpl };
}

describe("api client", () => {
  it("sends bearer token and JSON body on search", async () => {
    const f = mockFetch(200, { hits: [] });
    await searchMemories(cfg(f), "psp timeout", 25, "2026-06-01T00:00:00Z");
    const [url, init] = (f as ReturnType<typeof vi.fn>).mock.calls[0] as [
      string,
      RequestInit,
    ];
    expect(url).toBe("http://brainiac.test:8600/v1/memories/search");
    expect(init.method).toBe("POST");
    expect((init.headers as Record<string, string>).authorization).toBe("Bearer tok_test");
    expect(JSON.parse(init.body as string)).toEqual({
      query: "psp timeout",
      k: 25,
      as_of: "2026-06-01T00:00:00Z",
    });
  });

  it("unwraps list envelopes", async () => {
    const promos = [{ id: "p1", memory_id: "m1", to_status: "candidate", policy_rule: null }];
    expect(await pendingPromotions(cfg(mockFetch(200, { promotions: promos })))).toEqual(promos);

    const contradictions = [
      {
        id: "c1",
        memory_a: { id: "a", content: "10s" },
        memory_b: { id: "b", content: "30s" },
        detected_by: "test",
        suggested_resolution: null,
      },
    ];
    expect(
      await listContradictions(cfg(mockFetch(200, { contradictions }))),
    ).toEqual(contradictions);
  });

  it("builds review action URLs", async () => {
    const f = mockFetch(200, {
      promotion_id: "p1",
      memory_id: "m1",
      decision: "approved",
      memory_status: "candidate",
    });
    const out = await reviewPromotion(cfg(f), "p1", "approve");
    expect(out.decision).toBe("approved");
    const [url] = (f as ReturnType<typeof vi.fn>).mock.calls[0] as [string];
    expect(url).toBe("http://brainiac.test:8600/v1/reviews/promotions/p1/approve");
  });

  it("omits optional resolve fields when absent", async () => {
    const f = mockFetch(200, { contradiction_id: "c1", status: "dismissed" });
    await resolveContradiction(cfg(f), "c1", "dismiss");
    const [, init] = (f as ReturnType<typeof vi.fn>).mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({ resolution: "dismiss" });
  });

  it("includes winner on supersede", async () => {
    const f = mockFetch(200, { contradiction_id: "c1", status: "resolved_supersede" });
    await resolveContradiction(cfg(f), "c1", "supersede", "mem-w", "why");
    const [, init] = (f as ReturnType<typeof vi.fn>).mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({
      resolution: "supersede",
      winner_memory_id: "mem-w",
      note: "why",
    });
  });

  it("throws typed ApiError with status on failure", async () => {
    const err = await getAnalytics(cfg(mockFetch(403, { error: "nope" }))).catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(403);
  });

  it("passes through graph payloads", async () => {
    const graph = { canonicals: [], entities: [], edges: [] };
    expect(await getGraph(cfg(mockFetch(200, graph)))).toEqual(graph);
  });
});
