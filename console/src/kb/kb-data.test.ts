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

const STATUSES: Status[] = ["shipped", "built_off", "in_progress", "roadmap"];

/** docs/KB-PLAN.md status log, as of 2026-07-14 (KB0–KB5 all landed). */
const PLAN_TRUTH: Record<string, Status> = {
  KB0: "shipped",
  KB1: "shipped",
  KB2: "shipped",
  // Built and tested — and switched off. Neither "shipped" nor "roadmap" is
  // true, and saying either would be the exact dishonesty this page argues
  // against, just pointed in a different direction.
  KB3: "built_off",
  KB4: "shipped",
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

  it("never claims an org is publishing when publishing is switched off", () => {
    // THE invariant that replaces the old "publishing is roadmap" one. The code
    // exists and the tests pass, and no org is publishing anything: kb_enabled
    // is false by default, no publish target exists, and external publishing
    // waits on the extraction-recall gate. Stamping any of this plain "shipped"
    // would tell a reader their wiki is being kept honest when it is not — the
    // same class of lie as calling an intent an architecture.
    expect(LADDER.find((p) => p.id === "KB3")?.status).toBe("built_off");
    expect(CONFLUENCE.status).toBe("built_off");
    expect(SCOPES.status).toBe("built_off");
    expect(PROPERTIES.find((p) => p.key === "health-gate")?.status).toBe("built_off");
    // The two compose stages that reach outside the building.
    for (const name of ["Gate", "Publish"]) {
      expect(COMPOSE_STAGES.find((s) => s.name === name)?.status).toBe("built_off");
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

  it("does not understate what actually shipped either", () => {
    // The honesty rule cuts both ways. A page that hides a real capability
    // behind a "roadmap" stamp is also lying, and it teaches a reader to stop
    // trusting the stamps — which is what makes the KB3 "built · not enabled"
    // stamp above worth anything.
    expect(PROPERTIES.find((p) => p.key === "projection")?.status).toBe("shipped");
    expect(PROPERTIES.find((p) => p.key === "round-trip")?.status).toBe("shipped");
    for (const name of ["Bind", "Cap", "Compose", "Diff & decide"]) {
      expect(COMPOSE_STAGES.find((s) => s.name === name)?.status).toBe("shipped");
    }
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
