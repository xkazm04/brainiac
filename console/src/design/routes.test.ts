import { describe, expect, it } from "vitest";

import { BAND_HUES, MODULE_BAND } from "./theme";
import {
  DEFAULT_MODULE,
  NAV_GROUPS,
  PRODUCT_ROUTES,
  isPublicSurface,
  parseModule,
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
