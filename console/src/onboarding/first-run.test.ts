import { describe, expect, it } from "vitest";

import { DEMO_OBSERVATORY } from "@/observatory/observatory-data";

import { FORCE_ANALYTICS_VIEW, hasIngestedAnything, isFirstRun } from "./first-run";

describe("isFirstRun", () => {
  it("is true for a live org with an empty totals map (a fresh database)", () => {
    expect(isFirstRun({ live: true, totals: {} })).toBe(true);
  });

  it("is true for a live org whose canonical count is explicitly zero", () => {
    expect(isFirstRun({ live: true, totals: { canonical: 0, raw: 0 } })).toBe(true);
  });

  it("is true mid-pipeline — raw/candidate rows are not retrievable knowledge", () => {
    expect(isFirstRun({ live: true, totals: { raw: 12, candidate: 3, canonical: 0 } })).toBe(true);
  });

  it("is false as soon as one memory is canonical", () => {
    expect(isFirstRun({ live: true, totals: { canonical: 1 } })).toBe(false);
  });

  it("is false when the fetch fell back to fixtures — that is a server problem, not onboarding", () => {
    // The guard has to hold even though the fixture totals are non-empty:
    // an operator whose server is down must not be told to onboard a repo.
    expect(isFirstRun({ live: false, totals: {} })).toBe(false);
    expect(isFirstRun({ live: false, totals: DEMO_OBSERVATORY.totals })).toBe(false);
  });

  it("is false when the operator explicitly asked for the charts", () => {
    expect(isFirstRun({ live: true, totals: {}, view: FORCE_ANALYTICS_VIEW })).toBe(false);
  });

  it("ignores an unrelated ?view= value", () => {
    expect(isFirstRun({ live: true, totals: {}, view: "something-else" })).toBe(true);
  });

  it("does not fire on the demo fixture shape when live (it has canonical memories)", () => {
    expect(isFirstRun({ live: true, totals: DEMO_OBSERVATORY.totals })).toBe(false);
  });
});

describe("hasIngestedAnything", () => {
  it("is false for a fresh database", () => {
    expect(hasIngestedAnything({})).toBe(false);
    expect(hasIngestedAnything({ canonical: 0, raw: 0 })).toBe(false);
  });

  it("is true once any status carries a row", () => {
    expect(hasIngestedAnything({ raw: 1 })).toBe(true);
    expect(hasIngestedAnything({ candidate: 4, canonical: 0 })).toBe(true);
  });
});
