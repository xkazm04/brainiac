import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  AGENTS,
  CHECK_US,
  DRIFT_CAPTION,
  INTAKE,
  LADDER,
  LAYERS_INTRO,
  LOOP_LEDE,
  NEVER,
  PROPERTIES,
  RULE_STAGES,
  STATUS_LABEL,
  THESIS,
  THESIS_BODY,
  type Status,
} from "./library-data";

/*
 * The honesty guard, Library edition.
 *
 * This page has the hardest honesty job of the three public surfaces, and the
 * job has changed shape: when it was written almost nothing it described was
 * built, and the risk was a stamp quietly promoting itself to "shipped". The
 * whole ladder has since landed, so the risk inverted — prose that still tells
 * a visitor the feature is roadmap. Both directions are drift, and both fail
 * here now: the status tests below pin every stamp to the LIBRARY-PLAN status
 * log, and `section prose keeps up with the stamps` scans the section
 * components for unbuiltness claims the data no longer supports.
 *
 * That second test exists because the first one could not see it: ALL_PAGE_TEXT
 * flattens library-data.ts only, so three sections spent weeks telling readers
 * the Library was "almost all roadmap" while every status beside it said
 * shipped — on the page whose own argument is that unmeasured claims drift.
 *
 * When a phase changes state, the fix is to update BOTH this expectation and
 * docs/LIBRARY-PLAN.md — in that order of annoyance, deliberately.
 */

const STATUSES: Status[] = ["shipped", "in_progress", "roadmap"];

/** docs/LIBRARY-PLAN.md status log, as of 2026-07-15 (the full ladder landed). */
const PLAN_TRUTH: Record<string, Status> = {
  LB0: "shipped",
  LB1: "shipped",
  LB2: "shipped",
  LB3: "shipped",
  LB4: "shipped",
  LB5: "shipped",
};

/** Every user-visible string the page renders, flattened for the audience rule. */
const ALL_PAGE_TEXT = JSON.stringify([
  THESIS,
  THESIS_BODY,
  DRIFT_CAPTION,
  LAYERS_INTRO,
  LOOP_LEDE,
  CHECK_US,
  STATUS_LABEL,
  INTAKE,
  PROPERTIES,
  RULE_STAGES,
  AGENTS,
  NEVER,
  LADDER,
]);

describe("library-data honesty rules", () => {
  it("stamps every phase exactly as the LIBRARY-PLAN status log does", () => {
    const actual = Object.fromEntries(LADDER.map((p) => [p.id, p.status]));
    expect(actual).toEqual(PLAN_TRUTH);
  });

  it("never speaks to visitors in repo coordinates", () => {
    // The audience rule, inherited from /kb. A visitor cannot open the
    // repository mid-sentence, so a file path, a table name, or a section-sign
    // reference to a design document is noise at best and a credibility leak
    // at worst.
    const REPO_COORDINATES = [
      /(docs|crates|results|migrations|fixtures|src)\//, // paths
      /\.(md|rs|sql|yaml|json|tsx?)\b/, // file extensions (incl. SKILL.md)
      /§/, // section signs into documents the reader has never seen
      /\bLIBRARY-PLAN\b|\bKB-PLAN\b|\bARCHITECTURE\b/, // internal doc names
      /\b(practice_divergences|standard_versions|standard_provenance|library_usage_events|skill_versions|detail_md)\b/, // schema names
    ];
    for (const pattern of REPO_COORDINATES) {
      expect(ALL_PAGE_TEXT).not.toMatch(pattern);
    }
  });

  it("stamps exactly what runs — no more, no less", () => {
    // The whole layer now runs: the detector, the rule-as-atom substrate, the
    // attribution constraint, skill serving, and — with the health follow-up —
    // vital signs that raise themselves on the leadership report rather than
    // waiting for someone to open a board.
    //
    // If a future edit adds a property or a station, it lands here as roadmap
    // and STAYS there until someone updates this list on purpose. That is the
    // point: overclaiming is the failure this page argues against, and
    // understating what runs teaches the reader the stamps mean nothing.
    for (const p of PROPERTIES) {
      expect(p.status, p.key).toBe("shipped");
    }
    for (const s of RULE_STAGES) {
      expect(s.status, s.name).toBe("shipped");
    }
    // …and the one thing the loop must never claim: nothing normative changes
    // without a named human, INCLUDING taking a rule away. A future "auto-
    // retire dormant rules" feature would be a Never-list violation wearing a
    // convenience costume, so the copy must keep saying so.
    expect(LOOP_LEDE).toMatch(/never|named human/i);
    // The agent surface is fully live, including proposals — and the
    // lib:propose copy must state the two guards that tame the noisy channel
    // (the rate limit and the collapse-onto-existing dedup), because those
    // guards ARE the promise that made shipping this last safe.
    expect(AGENTS.status).toBe("shipped");
    const propose = AGENTS.rows.find((r) => r.scope === "lib:propose")?.body ?? "";
    expect(propose).toMatch(/rate-limited/i);
    expect(propose).toMatch(/collapsed|dedup/i);
  });

  it("keeps every capability on a known status", () => {
    const stated = [
      ...PROPERTIES.map((p) => p.status),
      ...RULE_STAGES.map((s) => s.status),
      ...LADDER.map((p) => p.status),
      AGENTS.status,
    ];
    for (const s of stated) expect(STATUSES).toContain(s);
    for (const s of STATUSES) expect(STATUS_LABEL[s]).toBeTruthy();
  });

  it("cites evidence or a commitment for every property", () => {
    for (const p of PROPERTIES) {
      expect(p.evidence.length).toBeGreaterThan(0);
      // Only the shipped property may claim something is running.
      if (p.status !== "shipped") {
        expect(p.evidence).not.toMatch(/running today|verified|measured/i);
      }
    }
  });

  it("forbids any path into an adopted rule that skips the named human", () => {
    for (const flow of INTAKE) {
      if (flow.to === "adopted rule" && flow.allowed) {
        expect(flow.gate).toBe("a named human");
      }
      // Candidate-producing flows must pass through triage.
      if (flow.to === "rule candidate" && flow.allowed) {
        expect(flow.gate).toMatch(/triage/);
      }
    }
    // The direct write must stay disallowed.
    expect(INTAKE.some((f) => f.label === "direct write" && !f.allowed)).toBe(true);
  });

  it("section prose keeps up with the stamps", () => {
    // The gap this closes: ALL_PAGE_TEXT covers library-data.ts, so nothing was
    // watching the JSX. Where every status is "shipped", no section may still
    // describe the feature as unbuilt — in prose or in a hardcoded stamp.
    const everythingShipped = [...LADDER, ...PROPERTIES, ...RULE_STAGES].every(
      (x) => x.status === "shipped",
    );
    if (!everythingShipped) return; // mixed state: the stamps carry the nuance.

    const dir = path.resolve(__dirname, "sections");
    const files = readdirSync(dir).filter((f) => f.endsWith(".tsx"));
    expect(files.length, "sections must be discoverable").toBeGreaterThan(4);

    // Claims of unbuiltness that the data no longer supports.
    const stale: RegExp[] = [
      /almost\s+(?:all|none)[^.]{0,40}roadmap/i,
      /(?:stay|stays|remain|remains)\s+stamped\s+roadmap/i,
      /mostly\s+roadmap/i,
      /not\s+built\s+yet/i,
      /nothing\s+(?:here|it\s+describes)\s+is\s+built/i,
      // A hardcoded roadmap stamp is the same claim in component form.
      /<Stamp\s+status=["']roadmap["']/,
    ];

    for (const f of files) {
      const text = readFileSync(path.join(dir, f), "utf8");
      for (const re of stale) {
        expect(
          re.test(text),
          `${f} claims the Library is unbuilt (${re}) while every status says shipped`,
        ).toBe(false);
      }
    }
  });

  it("keeps the telemetry promise in the refusals", () => {
    // "Never a leaderboard" is the Library's load-bearing trust commitment —
    // per-team aggregation is what makes usage telemetry adoptable at all. If
    // it ever leaves the never-list, that is a product decision someone must
    // make in the open, not an edit that slips through.
    expect(NEVER.some((n) => /leaderboard/i.test(n.title))).toBe(true);
    expect(ALL_PAGE_TEXT).toMatch(/never (?:per|by) person/i);
  });
});
