import { afterEach, describe, expect, it, vi } from "vitest";

import {
  FUNNEL,
  analyticsEnabled,
  plausibleEventUrl,
  plausibleScriptUrl,
} from "./config";
import { trackActivation, trackServer } from "./server";

/*
 * What these tests are actually protecting:
 *
 *  1. UNCONFIGURED MUST BE SILENT. There is no domain yet at launch time. If
 *     analytics fired (or threw) without one, the first deploy would either
 *     error or beacon to a Plausible site that does not exist. "No-ops cleanly
 *     when unconfigured" is a launch requirement, not a nicety.
 *
 *  2. NO PERSONAL DATA IN THE BEACON. The privacy page asserts the funnel
 *     carries no user, org or memory identifier and that no end-user IP is
 *     forwarded. That is a published claim; it deserves an assertion.
 *
 *  3. FAILURE IS SWALLOWED. A telemetry beacon must never reject into a server
 *     action that a user is waiting on.
 */

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const enable = (domain = "brainiac.example") =>
  vi.stubEnv("NEXT_PUBLIC_PLAUSIBLE_DOMAIN", domain);

describe("configuration gating", () => {
  it("is disabled when no domain is set", () => {
    vi.stubEnv("NEXT_PUBLIC_PLAUSIBLE_DOMAIN", "");
    expect(analyticsEnabled()).toBe(false);
  });

  it("treats a whitespace-only domain as unset", () => {
    vi.stubEnv("NEXT_PUBLIC_PLAUSIBLE_DOMAIN", "   ");
    expect(analyticsEnabled()).toBe(false);
  });

  it("is enabled once a domain is set", () => {
    enable();
    expect(analyticsEnabled()).toBe(true);
  });

  it("defaults to Plausible cloud and strips a trailing slash from an override", () => {
    expect(plausibleScriptUrl()).toBe("https://plausible.io/js/script.js");
    vi.stubEnv("NEXT_PUBLIC_PLAUSIBLE_HOST", "https://stats.example.dev/");
    expect(plausibleEventUrl()).toBe("https://stats.example.dev/api/event");
  });
});

describe("trackServer", () => {
  it("sends nothing at all when unconfigured", async () => {
    vi.stubEnv("NEXT_PUBLIC_PLAUSIBLE_DOMAIN", "");
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    await trackServer(FUNNEL.activation, { via: "memory_add" });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("posts the event to the Plausible Events API when configured", async () => {
    enable();
    const fetchSpy = vi.fn().mockResolvedValue(new Response("ok"));
    vi.stubGlobal("fetch", fetchSpy);

    await trackServer(FUNNEL.activation, { via: "promotion" });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("https://plausible.io/api/event");
    expect(init.method).toBe("POST");
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    expect(body.name).toBe("activation");
    expect(body.domain).toBe("brainiac.example");
    expect(body.props).toEqual({ via: "promotion" });
  });

  it("forwards no client IP — the privacy page says end-user IPs are not shared", async () => {
    enable();
    const fetchSpy = vi.fn().mockResolvedValue(new Response("ok"));
    vi.stubGlobal("fetch", fetchSpy);

    await trackServer(FUNNEL.activation, { via: "memory_add" });

    const init = fetchSpy.mock.calls[0][1] as RequestInit;
    const headers = init.headers as Record<string, string>;
    const names = Object.keys(headers).map((k) => k.toLowerCase());
    expect(names).not.toContain("x-forwarded-for");
    // The Events API rejects a request with no user-agent outright.
    expect(names).toContain("user-agent");
  });

  it("carries no identifier beyond the low-cardinality shape prop", async () => {
    enable();
    const fetchSpy = vi.fn().mockResolvedValue(new Response("ok"));
    vi.stubGlobal("fetch", fetchSpy);

    await trackServer(FUNNEL.signupComplete, { created: true });

    const body = JSON.parse(
      String((fetchSpy.mock.calls[0][1] as RequestInit).body),
    ) as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual(["domain", "name", "props", "url"]);
    expect(body.props).toEqual({ created: true });
  });

  it("never rejects when the beacon fails", async () => {
    enable();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockRejectedValue(new Error("network is down")),
    );
    await expect(trackServer(FUNNEL.activation, { via: "memory_add" })).resolves
      .toBeUndefined();
  });

  it("trackActivation is fire-and-forget and does not throw synchronously", () => {
    enable();
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("nope")));
    expect(() => trackActivation("memory_add")).not.toThrow();
  });
});
