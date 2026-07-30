import { describe, expect, it } from "vitest";

import { BAND_HUES, MODULE_BAND } from "./theme";
import {
  ADDRESS_SPECS,
  DEFAULT_MODULE,
  NAV_GROUPS,
  PRODUCT_ROUTES,
  RESERVED_KEYS,
  decodeAddress,
  encodeAddress,
  int,
  isPublicSurface,
  oneOf,
  parseModule,
  type ConsoleAddress,
} from "./routes";

/*
 * The registry consistency guard (LIBRARY-PLAN LB2 gate). The nav, the module
 * dispatcher, and the band system all key off this one registry; a segment
 * added to one and not the others fails HERE instead of rendering a blank
 * tab or an unstyled module.
 */

describe("the routes registry", () => {
  it("gives every route a segment, a label, and a resolvable band", () => {
    for (const r of PRODUCT_ROUTES) {
      expect(r.segment.length, r.path).toBeGreaterThan(0);
      expect(r.label.length, r.path).toBeGreaterThan(0);
      if (r.band !== "ground") {
        expect(BAND_HUES[r.band], `${r.segment} band`).toBeTypeOf("number");
      }
    }
  });

  it("keeps segments and labels unique — a duplicate label is two doors with one sign", () => {
    const segments = PRODUCT_ROUTES.map((r) => r.segment);
    expect(new Set(segments).size).toBe(segments.length);
    const labels = PRODUCT_ROUTES.map((r) => r.label);
    expect(new Set(labels).size).toBe(labels.length);
  });

  it("files every grouped route under a declared nav group", () => {
    const groups = new Set(NAV_GROUPS.map((g) => g.id));
    for (const r of PRODUCT_ROUTES) {
      if (r.group) expect(groups.has(r.group), `${r.segment} group`).toBe(true);
    }
    // …and no declared group is empty: an empty group renders a dangling header.
    for (const g of NAV_GROUPS) {
      expect(
        PRODUCT_ROUTES.some((r) => r.group === g.id),
        `group ${g.id} has routes`,
      ).toBe(true);
    }
  });

  it("hosts the library group with the standards board and the skills shelf", () => {
    const library = PRODUCT_ROUTES.filter((r) => r.group === "library");
    expect(library.map((r) => r.segment).sort()).toEqual(["skills", "standards"]);
    // The detector keeps its own door under knowledge — the Library owns the
    // word "standards" for the ARTIFACT; the drift board is the detector.
    const divergence = PRODUCT_ROUTES.find((r) => r.segment === "divergence");
    expect(divergence?.group).toBe("knowledge");
    expect(divergence?.label).not.toBe("standards");
  });

  it("resolves ?m= for the new modules and falls back for junk", () => {
    expect(parseModule("standards")).toBe("standards");
    expect(parseModule("skills")).toBe("skills");
    expect(parseModule("no-such-module")).toBe(DEFAULT_MODULE);
    expect(parseModule(undefined)).toBe(DEFAULT_MODULE);
  });

  it("keys every module's band accent in the theme", () => {
    // Chrome captions and skeleton accents read MODULE_BAND; a module missing
    // there renders unaccented and looks broken rather than erroring.
    for (const segment of ["standards", "skills"]) {
      expect(MODULE_BAND[segment], segment).toBeDefined();
    }
  });

  it("keeps the console private and the explainers public", () => {
    expect(isPublicSurface("/console")).toBe(false);
    for (const pub of ["/", "/pitch", "/kb", "/library", "/demo"]) {
      expect(isPublicSurface(pub), pub).toBe(true);
    }
  });
});

// ── the console address grammar ─────────────────────────────────────────

describe("console address", () => {
  it("round-trips a specific view", () => {
    const a: ConsoleAddress = {
      module: "memories",
      sel: "d41d8cd9-8f00-4000-8000-000000000001",
      asOf: "2026-03-11T00:00:00Z",
      filters: { kind: "pitfall", page: "2" },
    };
    const round = decodeAddress(paramsOf(encodeAddress(a)));
    expect(round).toEqual(a);
  });

  it("encodes the same view to the same string", () => {
    // Key order is fixed by the spec, not by insertion — otherwise two links to
    // the same view compare unequal and nothing downstream can dedupe them.
    const one = encodeAddress({ module: "memories", filters: { page: "2", kind: "pitfall" } });
    const two = encodeAddress({ module: "memories", filters: { kind: "pitfall", page: "2" } });
    expect(one).toBe(two);
  });

  it("drops filters the module never declared", () => {
    // The drift guard: a stray param cannot ride along and quietly become
    // load-bearing somewhere downstream.
    const a = decodeAddress({ m: "audit", kind: "promotion", q: "injected", sel: "x" });
    expect(a.filters).toEqual({ kind: "promotion" });
    expect(a.sel).toBe("x");
  });

  it("survives junk without 404ing", () => {
    const a = decodeAddress({ m: "not-a-module" });
    expect(a.module).toBe(DEFAULT_MODULE);
    expect(a.filters).toEqual({});
  });

  it("takes the first value of a repeated param", () => {
    const a = decodeAddress({ m: "memories", kind: ["pitfall", "fact"] });
    expect(a.filters.kind).toBe("pitfall");
  });

  it("never lets a module claim a reserved key", () => {
    // `sel` is the whole point of the grammar; a module filter named `sel`
    // would silently shadow the selection and un-share every link.
    for (const [module, keys] of Object.entries(ADDRESS_SPECS)) {
      for (const k of keys) {
        expect(RESERVED_KEYS, `${module} claims reserved key ${k}`).not.toContain(k);
      }
    }
  });

  it("declares a spec for every module in the nav", () => {
    // A module added to the nav without a spec would silently lose every
    // filter it reads on the first round trip.
    for (const r of PRODUCT_ROUTES) {
      expect(ADDRESS_SPECS, `no address spec for ${r.segment}`).toHaveProperty(r.segment);
    }
  });
});

describe("address coercion", () => {
  it("clamps junk offsets to the first page instead of NaN", () => {
    expect(int("3")).toBe(3);
    expect(int("-1")).toBe(0);
    expect(int("abc")).toBe(0);
    expect(int(undefined)).toBe(0);
    expect(int(undefined, 7)).toBe(7);
    // Fractions floor rather than fall back — the reconciliation of the two
    // dialects this replaced (audit floored, reviews rejected).
    expect(int("2.5")).toBe(2);
  });

  it("constrains enums and falls back rather than passing junk to the API", () => {
    expect(oneOf("pitfall", ["fact", "pitfall"] as const)).toBe("pitfall");
    expect(oneOf("nonsense", ["fact", "pitfall"] as const)).toBeUndefined();
    expect(oneOf("nonsense", ["fact", "pitfall"] as const, "fact")).toBe("fact");
  });
});

/** Parse an encoded address back into the param bag a Next page receives. */
function paramsOf(url: string): Record<string, string | string[] | undefined> {
  const q = new URLSearchParams(url.slice(url.indexOf("?") + 1));
  const out: Record<string, string> = {};
  q.forEach((v, k) => (out[k] = v));
  return out;
}
