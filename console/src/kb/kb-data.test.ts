import { describe, expect, it } from "vitest";

import {
  ASYMMETRY,
  COMPOSE_STAGES,
  CONFLUENCE,
  LADDER,
  PROPERTIES,
  SCOPES,
  STATUS_LABEL,
  type Status,
} from "./kb-data";

/*
 * The honesty guard.
 *
 * The KB page's whole argument is that a document layer must not present an
 * intent as shipped architecture. If this page did that about itself, the
 * argument is dead. These tests are the mechanical part of that promise: they
 * pin the status of every phase to the KB-PLAN status log, so a future edit that
 * quietly promotes an unbuilt phase to "shipped" fails CI instead of shipping.
 *
 * When a phase actually lands, the fix is to update BOTH this expectation and
 * docs/KB-PLAN.md — in that order of annoyance, deliberately.
 */

const STATUSES: Status[] = ["shipped", "in_progress", "roadmap"];

/** docs/KB-PLAN.md status log, as of 2026-07-14. */
const PLAN_TRUTH: Record<string, Status> = {
  KB0: "shipped",
  KB1: "in_progress",
  KB2: "roadmap",
  KB3: "roadmap",
  KB4: "roadmap",
};

describe("kb-data honesty rules", () => {
  it("stamps every phase exactly as the KB-PLAN status log does", () => {
    const actual = Object.fromEntries(LADDER.map((p) => [p.id, p.status]));
    expect(actual).toEqual(PLAN_TRUTH);
  });

  it("only claims a verification gate for phases that have actually shipped", () => {
    for (const phase of LADDER) {
      if (phase.gate) expect(phase.status).toBe("shipped");
    }
  });

  it("keeps every capability on a known status", () => {
    const stated = [
      ...PROPERTIES.map((p) => p.status),
      ...COMPOSE_STAGES.map((s) => s.status),
      CONFLUENCE.status,
      SCOPES.status,
    ];
    for (const s of stated) expect(STATUSES).toContain(s);
    for (const s of STATUSES) expect(STATUS_LABEL[s]).toBeTruthy();
  });

  it("never marks the unbuilt document layer as shipped", () => {
    // KB1 is in flight: nothing about composition, publishing or the health
    // breaker may carry a "shipped" stamp until the phase lands.
    const unbuilt = ["projection", "health-gate"];
    for (const key of unbuilt) {
      const p = PROPERTIES.find((x) => x.key === key);
      expect(p?.status).not.toBe("shipped");
    }
    expect(CONFLUENCE.status).toBe("roadmap");
    expect(SCOPES.status).toBe("roadmap");
    // Publishing stages cannot be shipped either.
    expect(COMPOSE_STAGES.filter((s) => s.status === "shipped")).toHaveLength(0);
  });

  it("cites a checkable source for every shipped property", () => {
    for (const p of PROPERTIES) {
      expect(p.evidence.length).toBeGreaterThan(0);
    }
  });

  it("forbids any allowed flow from a page back into canonical memory that skips the gate", () => {
    for (const flow of ASYMMETRY) {
      if (flow.to === "canonical memories" && flow.allowed) {
        expect(flow.gate).toBe("the same review gate");
      }
    }
    // The direct write-back and direct agent page-write must stay disallowed.
    expect(ASYMMETRY.some((f) => f.label === "direct write-back" && !f.allowed)).toBe(true);
    expect(ASYMMETRY.some((f) => f.label === "direct page write" && !f.allowed)).toBe(true);
  });
});
