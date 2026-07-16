import { describe, expect, it, vi } from "vitest";

import { getGraphOverview, type ApiConfig } from "./api";
import { withDemoFallback } from "./demo-fallback";
import { DEMO_CORTEX } from "../../app/console/modules/graph/cortex-data";

describe("withDemoFallback", () => {
  const DEMO = { live: false as const, value: "fixture" };

  it("returns live data with live:true when the fetch succeeds", async () => {
    const res = await withDemoFallback(async () => ({ live: true, value: "real" }), DEMO);
    expect(res.live).toBe(true);
    expect(res.data).toEqual({ live: true, value: "real" });
  });

  it("falls back to the demo fixture with live:false when the fetch throws", async () => {
    const res = await withDemoFallback(async () => {
      throw new Error("offline");
    }, DEMO);
    expect(res.live).toBe(false);
    expect(res.data).toBe(DEMO);
  });

  it("runs the live fetch exactly once", async () => {
    const fetchLive = vi.fn<() => Promise<{ live: boolean }>>(async () => ({ live: true }));
    await withDemoFallback(fetchLive, { live: false });
    expect(fetchLive).toHaveBeenCalledTimes(1);
  });
});

// Page-level integration: exercise the graph page's real offline path — the
// real REST client hitting an unreachable server, wrapped exactly as the page
// wraps it — and assert it yields the demo fixture with live:false, which is
// precisely what gates the page-level <DemoBanner /> ({!live && <DemoBanner/>}).
describe("demo fallback — graph page offline path", () => {
  it("renders the demo cortex (live:false) when the server is unreachable", async () => {
    const cfg: ApiConfig = {
      baseUrl: "http://unreachable.invalid:8600",
      token: "t",
      fetchImpl: (async () => {
        throw new Error("ECONNREFUSED");
      }) as unknown as typeof fetch,
    };
    const { data, live } = await withDemoFallback(
      async () => ({ live: true as const, overview: await getGraphOverview(cfg) }),
      DEMO_CORTEX,
    );
    expect(live).toBe(false);
    expect(data).toBe(DEMO_CORTEX);
    // The fixture itself must carry live:false so the banner gate always fires.
    expect(data.live).toBe(false);
  });
});
